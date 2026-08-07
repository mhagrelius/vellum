//! The open file.
//!
//! Reading and writing are the two places this crate touches the outside world,
//! and both are converted to a typed outcome at the seam: above [`open`] and
//! [`Document::save`] there is no `io::Error` and nothing to catch.
//!
//! [`open`]: Document::open

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Everything that can go wrong opening or saving, in the words the banner
/// would use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    NotFound,
    /// The file exists and cannot be read or written — permissions, a full
    /// disk, a disconnected share.
    Denied(String),
    /// Not text. Opening it as text and writing it back would destroy it.
    NotText,
    /// Save was asked for on a document that has never had a path.
    NoPath,
}

impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(formatter, "the file no longer exists"),
            Self::Denied(why) => write!(formatter, "{why}"),
            Self::NotText => write!(formatter, "this is not a text file"),
            Self::NoPath => write!(formatter, "this document has not been saved yet"),
        }
    }
}

impl std::error::Error for DocumentError {}

/// A Markdown document, open or unsaved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    path: Option<PathBuf>,
    text: String,
    /// The text as it is on disk. Comparing against it is what "modified"
    /// means, so typing a character and deleting it again leaves the document
    /// clean — a dirty flag set on the first keystroke never comes back down.
    saved: String,
    modified_at: Option<SystemTime>,
}

impl Default for Document {
    fn default() -> Self {
        Self::blank()
    }
}

impl Document {
    pub fn blank() -> Self {
        Self {
            path: None,
            text: String::new(),
            saved: String::new(),
            modified_at: None,
        }
    }

    pub fn open(path: &Path) -> Result<Self, DocumentError> {
        let bytes = std::fs::read(path).map_err(|err| classify(&err))?;
        // Anything that is not UTF-8 is not a Markdown file, and opening it
        // lossily would turn the bytes it could not read into question marks and
        // then save them back over the original.
        let text = String::from_utf8(bytes).map_err(|_| DocumentError::NotText)?;
        let modified_at = std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok();

        Ok(Self {
            path: Some(path.to_path_buf()),
            saved: text.clone(),
            text,
            modified_at,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: String) {
        self.text = text;
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn modified_at(&self) -> Option<SystemTime> {
        self.modified_at
    }

    pub fn is_modified(&self) -> bool {
        self.text != self.saved
    }

    pub fn save(&mut self) -> Result<(), DocumentError> {
        let path = self.path.clone().ok_or(DocumentError::NoPath)?;
        self.save_as(&path)
    }

    /// Write the document, then adopt `path` as its own.
    ///
    /// Through a temporary file in the same directory and a rename, so a save
    /// interrupted by a crash or a full disk leaves the previous version intact
    /// rather than a half-written file. Same directory because a rename across
    /// filesystems is a copy, and would lose the guarantee.
    pub fn save_as(&mut self, path: &Path) -> Result<(), DocumentError> {
        let directory = path.parent().unwrap_or(Path::new("."));
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let temporary = directory.join(format!(".{name}.vellum-tmp"));

        std::fs::write(&temporary, self.text.as_bytes()).map_err(|err| classify(&err))?;
        if let Err(err) = std::fs::rename(&temporary, path) {
            // Leaving a dotfile beside the document after a failure is its own
            // small bug report; take it away and report the real error.
            let _ = std::fs::remove_file(&temporary);
            return Err(classify(&err));
        }

        self.saved = self.text.clone();
        self.path = Some(path.to_path_buf());
        self.modified_at = std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok();
        Ok(())
    }

    /// What the window is called: the file name, or the first heading of an
    /// unsaved document, or nothing yet.
    pub fn title(&self) -> String {
        if let Some(name) = self.path.as_ref().and_then(|path| path.file_name()) {
            return name.to_string_lossy().into_owned();
        }
        first_heading(&self.text).unwrap_or_else(|| "Untitled".to_string())
    }

    /// The directory, with the home directory written as `~`, for the window
    /// subtitle. Empty for a document that has never been saved.
    pub fn location(&self) -> String {
        let Some(parent) = self.path.as_ref().and_then(|path| path.parent()) else {
            return String::new();
        };
        let shown = parent.to_string_lossy();
        match std::env::var("HOME") {
            Ok(home) if !home.is_empty() && shown.starts_with(&home) => {
                format!("~{}", &shown[home.len()..])
            }
            _ => shown.into_owned(),
        }
    }
}

/// The heading an unsaved document names itself by.
fn first_heading(text: &str) -> Option<String> {
    let parsed = quill::parse(text);
    let chars: Vec<char> = text.chars().collect();
    parsed
        .spans
        .iter()
        .find(|span| matches!(span.style, quill::Style::Heading(_)))
        .map(|span| {
            chars[span.start..span.end.min(chars.len())]
                .iter()
                .collect::<String>()
                .trim()
                .to_string()
        })
        .filter(|heading| !heading.is_empty())
}

fn classify(err: &std::io::Error) -> DocumentError {
    match err.kind() {
        std::io::ErrorKind::NotFound => DocumentError::NotFound,
        _ => DocumentError::Denied(err.to_string()),
    }
}

/// "just now", "4 min ago", "yesterday" — for the window subtitle.
///
/// Takes `now` rather than reading the clock, so the boundaries can be tested
/// instead of waited for.
pub fn relative_time(then: SystemTime, now: SystemTime) -> String {
    let Ok(elapsed) = now.duration_since(then) else {
        // A file stamped in the future is a clock that has been changed or a
        // share whose time is off. Neither is worth a phrase of its own.
        return "just now".to_string();
    };

    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;

    let seconds = elapsed.as_secs();
    match seconds {
        0..=44 => "just now".to_string(),
        seconds if seconds < HOUR => plural(seconds / MINUTE, "min"),
        seconds if seconds < DAY => plural(seconds / HOUR, "hour"),
        seconds if seconds < 2 * DAY => "yesterday".to_string(),
        seconds if seconds < 30 * DAY => plural(seconds / DAY, "day"),
        _ => "a while ago".to_string(),
    }
}

fn plural(count: u64, unit: &str) -> String {
    let count = count.max(1);
    if count == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{count} {unit}s ago")
    }
}

/// Convenience for callers with a `Duration` rather than two instants.
pub fn relative_to_now(then: SystemTime) -> String {
    relative_time(then, SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ago(seconds: u64) -> std::time::Duration {
        std::time::Duration::from_secs(seconds)
    }

    #[test]
    fn a_blank_document_is_unmodified_and_untitled() {
        let mut document = Document::blank();
        assert!(!document.is_modified());
        assert_eq!(document.title(), "Untitled");
        assert_eq!(document.location(), "");
        assert_eq!(document.save(), Err(DocumentError::NoPath));
    }

    /// Typing a character and taking it away again leaves nothing to save. A
    /// flag set on the first keystroke would leave the close prompt asking about
    /// a document that matches the disk.
    #[test]
    fn modification_is_a_comparison_not_a_flag() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let path = directory.path().join("notes.md");
        std::fs::write(&path, "# Notes\n").expect("write");

        let mut document = Document::open(&path).expect("open");
        assert!(!document.is_modified());

        document.set_text("# Notes\nmore".to_string());
        assert!(document.is_modified());

        document.set_text("# Notes\n".to_string());
        assert!(!document.is_modified());
    }

    #[test]
    fn saving_writes_the_text_and_clears_the_modification() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let path = directory.path().join("notes.md");
        std::fs::write(&path, "old\n").expect("write");

        let mut document = Document::open(&path).expect("open");
        document.set_text("new\n".to_string());
        document.save().expect("save");

        assert_eq!(std::fs::read_to_string(&path).expect("read"), "new\n");
        assert!(!document.is_modified());
    }

    /// The temporary file the atomic save writes through must not be left
    /// behind, or the directory fills with dotfiles.
    #[test]
    fn saving_leaves_no_temporary_file_behind() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let path = directory.path().join("notes.md");

        let mut document = Document::blank();
        document.set_text("written\n".to_string());
        document.save_as(&path).expect("save");

        let left: Vec<String> = std::fs::read_dir(directory.path())
            .expect("list")
            .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        assert_eq!(left, ["notes.md"]);
    }

    #[test]
    fn save_as_adopts_the_new_path() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let path = directory.path().join("renamed.md");

        let mut document = Document::blank();
        document.set_text("body\n".to_string());
        document.save_as(&path).expect("save");

        assert_eq!(document.path(), Some(path.as_path()));
        assert_eq!(document.title(), "renamed.md");
        assert!(document.modified_at().is_some());
    }

    #[test]
    fn opening_something_that_is_not_there_says_so() {
        let directory = tempfile::tempdir().expect("a temp dir");
        assert_eq!(
            Document::open(&directory.path().join("absent.md")),
            Err(DocumentError::NotFound)
        );
    }

    /// Opening a binary file lossily and saving it back would replace every
    /// byte it could not decode with a question mark.
    #[test]
    fn opening_a_binary_file_is_refused_rather_than_mangled() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let path = directory.path().join("image.md");
        std::fs::write(&path, [0xff, 0xfe, 0x00, 0x01]).expect("write");
        assert_eq!(Document::open(&path), Err(DocumentError::NotText));
    }

    #[test]
    fn an_unsaved_document_is_named_by_its_first_heading() {
        let mut document = Document::blank();
        document.set_text("Some prose first.\n\n# The Real Title\n".to_string());
        assert_eq!(document.title(), "The Real Title");
    }

    #[test]
    fn elapsed_time_reads_as_a_phrase() {
        let now = SystemTime::UNIX_EPOCH + ago(10 * 24 * 60 * 60);
        let said = |seconds| relative_time(now - ago(seconds), now);

        assert_eq!(said(0), "just now");
        assert_eq!(said(44), "just now");
        assert_eq!(said(60), "1 min ago");
        assert_eq!(said(4 * 60), "4 mins ago");
        assert_eq!(said(60 * 60), "1 hour ago");
        assert_eq!(said(5 * 60 * 60), "5 hours ago");
        assert_eq!(said(25 * 60 * 60), "yesterday");
        assert_eq!(said(3 * 24 * 60 * 60), "3 days ago");
        assert_eq!(said(400 * 24 * 60 * 60), "a while ago");
    }

    /// A file whose timestamp is in the future — a clock change, or a network
    /// share — must not produce a phrase about negative time.
    #[test]
    fn a_timestamp_in_the_future_reads_as_just_now() {
        let now = SystemTime::UNIX_EPOCH + ago(1000);
        assert_eq!(relative_time(now + ago(500), now), "just now");
    }
}
