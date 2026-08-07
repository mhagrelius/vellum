//! The formatting commands behind the right-click menu.
//!
//! A command is a pure function of the document text and the selection: in,
//! text and two character offsets; out, one [`Replacement`] and where the
//! selection lands afterwards. Nothing here knows what a text buffer is, which
//! is what lets the same fifteen commands drive a GTK context menu and a macOS
//! one without either owning the meaning of "bold".
//!
//! What each command *writes* is [`quill`]'s decision, not this module's — the
//! scanner that styles the document is the only thing that can say which
//! characters make a bold run. A command that wrote its own syntax would sooner
//! or later write something the view renders as plain prose.

use quill::{Edit, Format};

/// One command from the formatting menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Bold,
    Italic,
    Strikethrough,
    Code,
    Link,
    /// Take a heading back to body text.
    Paragraph,
    Heading(u8),
    Quote,
    Bullet,
    Task,
    CodeBlock,
    Table,
    Rule,
}

/// The text a command replaces, and what it replaces it with.
///
/// One replacement, never a list: a frontend applies it inside a single undo
/// step, and a command that needed two edits would be two entries in the undo
/// history for one menu item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    /// Character offsets into the document, before the edit.
    pub start: usize,
    pub end: usize,
    pub text: String,
    /// Where the selection sits afterwards, in offsets into the document
    /// *after* the edit. Equal offsets mean a caret.
    pub selection: (usize, usize),
}

/// Every command, in the order the menu offers them.
pub const ALL: &[Command] = &[
    Command::Bold,
    Command::Italic,
    Command::Strikethrough,
    Command::Code,
    Command::Link,
    Command::Paragraph,
    Command::Heading(1),
    Command::Heading(2),
    Command::Heading(3),
    Command::Quote,
    Command::Bullet,
    Command::Task,
    Command::CodeBlock,
    Command::Table,
    Command::Rule,
];

impl Command {
    /// The corresponding scanner format, for the commands the scanner names.
    ///
    /// `Paragraph` has none: it is the absence of a format, and removing syntax
    /// is not something the scanner writes.
    pub fn format(self) -> Option<Format> {
        Some(match self {
            Self::Bold => Format::Bold,
            Self::Italic => Format::Italic,
            Self::Strikethrough => Format::Strikethrough,
            Self::Code => Format::Code,
            Self::Link => Format::Link,
            Self::Heading(level) => Format::Heading(level),
            Self::Quote => Format::Quote,
            Self::Bullet => Format::Bullet,
            Self::Task => Format::Task,
            Self::CodeBlock => Format::CodeBlock,
            Self::Table => Format::Table,
            Self::Rule => Format::Rule,
            Self::Paragraph => return None,
        })
    }

    /// Stable across releases: it is the target of the menu action, so a menu
    /// built from [`ALL`] needs no table of its own.
    pub fn id(self) -> &'static str {
        match self {
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Strikethrough => "strikethrough",
            Self::Code => "code",
            Self::Link => "link",
            Self::Paragraph => "paragraph",
            Self::Heading(1) => "heading-1",
            Self::Heading(2) => "heading-2",
            Self::Heading(_) => "heading-3",
            Self::Quote => "quote",
            Self::Bullet => "bullet",
            Self::Task => "task",
            Self::CodeBlock => "code-block",
            Self::Table => "table",
            Self::Rule => "rule",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        ALL.iter().copied().find(|command| command.id() == id)
    }

    /// What the menu item says. Header capitalisation, as the platform wants.
    pub fn label(self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::Strikethrough => "Strikethrough",
            Self::Code => "Inline Code",
            Self::Link => "Insert Link…",
            Self::Paragraph => "Paragraph",
            Self::Heading(1) => "Heading 1",
            Self::Heading(2) => "Heading 2",
            Self::Heading(_) => "Heading 3",
            Self::Quote => "Block Quote",
            Self::Bullet => "Bulleted List",
            Self::Task => "Task List",
            Self::CodeBlock => "Code Block",
            Self::Table => "Table",
            Self::Rule => "Horizontal Rule",
        }
    }

    /// The syntax it writes, for the menu to show beside the label — so the app
    /// teaches the Markdown rather than hiding it.
    pub fn syntax(self) -> &'static str {
        match self.format() {
            Some(format) => format.syntax(),
            None => "",
        }
    }

    /// The accelerator, in the platform-neutral `Ctrl`/`Shift` spelling. A
    /// frontend translates it to whatever its toolkit writes.
    pub fn accelerator(self) -> Option<&'static str> {
        Some(match self {
            Self::Bold => "Ctrl+B",
            Self::Italic => "Ctrl+I",
            Self::Code => "Ctrl+E",
            Self::Link => "Ctrl+K",
            Self::Paragraph => "Ctrl+0",
            Self::Heading(1) => "Ctrl+1",
            Self::Heading(2) => "Ctrl+2",
            Self::Heading(_) => "Ctrl+3",
            _ => return None,
        })
    }
}

/// Work out the single edit `command` makes to `text` at `selection`.
///
/// `None` when there is nothing to do. Offsets are characters, and are clamped
/// rather than trusted: a stale selection from a buffer that changed underneath
/// must not index past the end of a string.
pub fn apply(text: &str, selection: (usize, usize), command: Command) -> Option<Replacement> {
    let chars: Vec<char> = text.chars().collect();
    let low = selection.0.min(selection.1).min(chars.len());
    let high = selection.0.max(selection.1).min(chars.len());

    match command {
        Command::Paragraph => prefix(&chars, (low, high), "", true),
        command => match command.format()?.edit() {
            Edit::Wrap { before, after } => wrap(&chars, (low, high), &before, &after),
            Edit::Prefix { prefix: marker } => prefix(
                &chars,
                (low, high),
                &marker,
                matches!(command, Command::Heading(_)),
            ),
            Edit::Block { text, caret } => block(&chars, high, &text, caret),
        },
    }
}

/// Put `before` and `after` around the selection — or take them off again, if
/// they are already there.
///
/// Pressing Bold twice must give back what you started with. The markers can be
/// on either side of the selection boundary depending on how the selection was
/// made, so both arrangements count as already-bold.
fn wrap(
    chars: &[char],
    (low, high): (usize, usize),
    before: &str,
    after: &str,
) -> Option<Replacement> {
    let opening: Vec<char> = before.chars().collect();
    let closing: Vec<char> = after.chars().collect();

    let outside = low >= opening.len()
        && high + closing.len() <= chars.len()
        && chars[low - opening.len()..low] == opening[..]
        && chars[high..high + closing.len()] == closing[..];
    if outside {
        let inner: String = chars[low..high].iter().collect();
        let start = low - opening.len();
        return Some(Replacement {
            start,
            end: high + closing.len(),
            selection: (start, start + (high - low)),
            text: inner,
        });
    }

    let inside = high - low >= opening.len() + closing.len()
        && chars[low..low + opening.len()] == opening[..]
        && chars[high - closing.len()..high] == closing[..];
    if inside {
        let inner: String = chars[low + opening.len()..high - closing.len()]
            .iter()
            .collect();
        let length = inner.chars().count();
        return Some(Replacement {
            start: low,
            end: high,
            selection: (low, low + length),
            text: inner,
        });
    }

    let selected: String = chars[low..high].iter().collect();
    Some(Replacement {
        start: low,
        end: high,
        text: format!("{before}{selected}{after}"),
        // Inside the markers, over what was selected. With nothing selected
        // that is a caret between them, ready to type into.
        selection: (low + opening.len(), low + opening.len() + (high - low)),
    })
}

/// Put `marker` at the start of every selected line — or take it off, if every
/// one already has it.
///
/// `replaces_heading` is set for the commands that own the whole start of the
/// line: asking for Heading 2 on a line that is already Heading 1 should give
/// `## `, not `## # `.
fn prefix(
    chars: &[char],
    (low, high): (usize, usize),
    marker: &str,
    replaces_heading: bool,
) -> Option<Replacement> {
    let starts = line_starts(chars);
    let first = line_of(&starts, low);
    // A selection ending exactly at a line start stops on the line above: the
    // newline was swept up by dragging, not by choosing that line.
    let last = if high > low && starts.contains(&high) {
        line_of(&starts, high.saturating_sub(1))
    } else {
        line_of(&starts, high)
    };

    let region_start = starts[first];
    let region_end = line_end(chars, &starts, last);

    let lines: Vec<&[char]> = (first..=last)
        .map(|line| &chars[starts[line]..line_end(chars, &starts, line)])
        .collect();

    // A selection of nothing but blank lines still means the line it is on —
    // otherwise Quote on an empty document would do nothing at all.
    let all_blank = lines.iter().all(|line| is_blank(line));
    let touched = |line: &[char]| all_blank || !is_blank(line);

    let has_marker = |line: &[char]| !marker.is_empty() && starts_with(line, marker);
    let removing = !marker.is_empty()
        && lines
            .iter()
            .filter(|line| touched(line))
            .all(|line| has_marker(line));

    let mut rebuilt: Vec<String> = Vec::with_capacity(lines.len());
    let mut deltas: Vec<isize> = Vec::with_capacity(lines.len());
    for line in &lines {
        let old = line.len();
        let body: Vec<char> = if removing {
            line[marker.chars().count()..].to_vec()
        } else if replaces_heading {
            line[heading_prefix(line)..].to_vec()
        } else {
            line.to_vec()
        };

        // A line that already carries the marker keeps exactly one. Selecting a
        // quote and an unquoted line beneath it means "quote both", not "quote
        // the quote again".
        let rewritten: String = if removing || !touched(line) || has_marker(line) {
            body.iter().collect()
        } else {
            format!("{}{}", marker, body.iter().collect::<String>())
        };
        deltas.push(rewritten.chars().count() as isize - old as isize);
        rebuilt.push(rewritten);
    }

    let text = rebuilt.join("\n");
    if text == chars[region_start..region_end].iter().collect::<String>() {
        return None;
    }

    // Offsets move only by what happened at the start of their own line and the
    // lines above it, so a caret keeps its column and a selection keeps its
    // shape. The exception is a selection that began at a line start: indenting
    // whole lines has to leave the whole lines selected, markers included, or
    // pressing the button twice would quote what the first press wrote.
    let moved = |offset: usize, hold_at_line_start: bool| {
        let line = line_of(&starts, offset).clamp(first, last) - first;
        let above: isize = deltas[..line].iter().sum();
        let new_line_start = starts[first + line] as isize + above;
        if hold_at_line_start {
            return new_line_start.max(0) as usize;
        }
        let column = offset.saturating_sub(starts[first + line]) as isize;
        (new_line_start + (column + deltas[line]).max(0)).max(0) as usize
    };

    let whole_lines = low != high && starts.contains(&low);
    Some(Replacement {
        start: region_start,
        end: region_end,
        text,
        selection: (moved(low, whole_lines), moved(high, false)),
    })
}

/// Insert a block on lines of its own, below the line the selection ends on.
fn block(chars: &[char], high: usize, inserted: &str, caret: usize) -> Option<Replacement> {
    let starts = line_starts(chars);
    let line = line_of(&starts, high);
    let start = starts[line];
    let end = line_end(chars, &starts, line);

    // Onto the blank line if there is one, below the paragraph if there is not,
    // and never in the middle of a sentence.
    let (at, lead) = if is_blank(&chars[start..end]) {
        (start, "")
    } else if end < chars.len() {
        (end + 1, "")
    } else {
        (chars.len(), "\n")
    };

    let text = format!("{lead}{inserted}");
    let caret = at + lead.chars().count() + caret;
    Some(Replacement {
        start: at,
        end: at,
        text,
        selection: (caret, caret),
    })
}

/// How many characters of `# ` … `###### ` this line opens with.
fn heading_prefix(line: &[char]) -> usize {
    let hashes = line.iter().take_while(|c| **c == '#').count();
    if (1..=6).contains(&hashes) && line.get(hashes) == Some(&' ') {
        hashes + 1
    } else {
        0
    }
}

fn starts_with(line: &[char], marker: &str) -> bool {
    let marker: Vec<char> = marker.chars().collect();
    line.len() >= marker.len() && line[..marker.len()] == marker[..]
}

fn is_blank(line: &[char]) -> bool {
    line.iter().all(|c| c.is_whitespace())
}

fn line_starts(chars: &[char]) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (offset, character) in chars.iter().enumerate() {
        if *character == '\n' {
            starts.push(offset + 1);
        }
    }
    starts
}

fn line_of(starts: &[usize], offset: usize) -> usize {
    match starts.binary_search(&offset) {
        Ok(line) => line,
        Err(next) => next.saturating_sub(1),
    }
}

fn line_end(chars: &[char], starts: &[usize], line: usize) -> usize {
    starts
        .get(line + 1)
        .map(|next| next - 1)
        .unwrap_or(chars.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply a command and return the document it produces, with `|` marking a
    /// caret and `[…]` a selection — the whole result of one menu click in one
    /// readable string.
    fn run(text: &str, selection: (usize, usize), command: Command) -> String {
        let Some(edit) = apply(text, selection, command) else {
            return text.to_string();
        };
        let chars: Vec<char> = text.chars().collect();
        let mut out: String = chars[..edit.start].iter().collect();
        out.push_str(&edit.text);
        out.extend(chars[edit.end..].iter());

        let marked: Vec<char> = out.chars().collect();
        let (low, high) = edit.selection;
        assert!(high <= marked.len(), "selection past the end of {out:?}");
        let mut shown: String = marked[..low].iter().collect();
        if low == high {
            shown.push('|');
        } else {
            shown.push('[');
            shown.extend(marked[low..high].iter());
            shown.push(']');
        }
        shown.extend(marked[high..].iter());
        shown
    }

    /// The offsets of `word` in `text`, so a test names what it selected
    /// instead of counting characters.
    fn find(text: &str, word: &str) -> (usize, usize) {
        let start = text
            .chars()
            .collect::<String>()
            .find(word)
            .expect("present");
        let start = text[..start].chars().count();
        (start, start + word.chars().count())
    }

    #[test]
    fn wrapping_a_selection_keeps_it_selected() {
        assert_eq!(
            run(
                "A bold claim.",
                find("A bold claim.", "bold"),
                Command::Bold
            ),
            "A **[bold]** claim."
        );
    }

    #[test]
    fn wrapping_nothing_leaves_a_caret_between_the_markers() {
        assert_eq!(run("A  claim.", (2, 2), Command::Italic), "A *|* claim.");
    }

    /// Bold twice is not bold-inside-bold; it is what you started with.
    #[test]
    fn a_second_press_takes_the_formatting_off() {
        let text = "A **bold** claim.";
        assert_eq!(
            run(text, find(text, "bold"), Command::Bold),
            "A [bold] claim."
        );
        // …and the same when the markers were swept into the selection.
        assert_eq!(
            run(text, find(text, "**bold**"), Command::Bold),
            "A [bold] claim."
        );
    }

    #[test]
    fn every_inline_command_round_trips() {
        for command in [
            Command::Bold,
            Command::Italic,
            Command::Strikethrough,
            Command::Code,
        ] {
            let text = "one two three";
            let selection = find(text, "two");
            let on = apply(text, selection, command).expect("an edit");

            let mut applied: String = text.chars().take(on.start).collect();
            applied.push_str(&on.text);
            applied.extend(text.chars().skip(on.end));
            assert_ne!(applied, text, "{command:?} wrote nothing");

            let off = apply(&applied, on.selection, command).expect("an edit");
            let mut back: String = applied.chars().take(off.start).collect();
            back.push_str(&off.text);
            back.extend(applied.chars().skip(off.end));
            assert_eq!(back, text, "{command:?} does not undo itself");
        }
    }

    #[test]
    fn a_heading_replaces_the_level_already_there() {
        // The caret was before the T and stays before the T.
        assert_eq!(run("# Title", (2, 2), Command::Heading(2)), "## |Title");
        assert_eq!(run("### Deep", (5, 5), Command::Heading(1)), "# D|eep");
    }

    #[test]
    fn the_same_heading_twice_gives_back_a_paragraph() {
        assert_eq!(run("## Section", (4, 4), Command::Heading(2)), "S|ection");
    }

    #[test]
    fn paragraph_removes_whatever_heading_is_there() {
        assert_eq!(run("#### Deep", (6, 6), Command::Paragraph), "D|eep");
        // And does nothing to a line that is already body text.
        assert_eq!(apply("Body text", (0, 0), Command::Paragraph), None);
    }

    #[test]
    fn a_prefix_applies_to_every_selected_line() {
        let text = "one\ntwo\nthree\n";
        assert_eq!(
            run(text, (0, text.chars().count()), Command::Quote),
            "[> one\n> two\n> three\n]"
        );
    }

    /// Every line already quoted means the menu item is an unquote.
    #[test]
    fn a_prefix_every_line_has_comes_off() {
        let text = "> one\n> two\n";
        assert_eq!(run(text, (0, 12), Command::Quote), "[one\ntwo\n]");
    }

    /// One unquoted line among quoted ones means the user wants them all
    /// quoted, not the quoted ones unquoted.
    #[test]
    fn a_prefix_only_some_lines_have_goes_on_the_rest() {
        let text = "> one\ntwo\n";
        assert_eq!(run(text, (0, 10), Command::Quote), "[> one\n> two\n]");
    }

    /// Dragging to the start of the next line is how a whole line gets
    /// selected; it must not quote the line below as well.
    #[test]
    fn a_selection_ending_at_a_line_start_stops_on_the_line_above() {
        let text = "one\ntwo\n";
        assert_eq!(run(text, (0, 4), Command::Bullet), "[- one\n]two\n");
    }

    #[test]
    fn a_caret_keeps_its_column_when_a_prefix_goes_on() {
        // Caret before "two"; after "- " goes on it is still before "two".
        assert_eq!(run("one two", (4, 4), Command::Bullet), "- one |two");
    }

    #[test]
    fn blank_lines_between_paragraphs_are_left_alone() {
        let text = "one\n\ntwo\n";
        assert_eq!(
            run(text, (0, text.chars().count()), Command::Bullet),
            "[- one\n\n- two\n]"
        );
    }

    /// Quote on an empty document is the one case where the blank line is the
    /// point, and must not be skipped as blank.
    #[test]
    fn a_command_on_an_empty_line_still_writes_its_marker() {
        assert_eq!(run("", (0, 0), Command::Quote), "> |");
    }

    #[test]
    fn a_block_goes_below_the_paragraph_it_was_invoked_from() {
        assert_eq!(
            run("Prose.\nMore.\n", (2, 2), Command::CodeBlock),
            "Prose.\n```\n|\n```\nMore.\n"
        );
    }

    #[test]
    fn a_block_at_the_end_of_a_document_gets_the_newline_it_needs() {
        assert_eq!(run("Prose.", (2, 2), Command::Rule), "Prose.\n---\n|");
    }

    #[test]
    fn a_block_lands_on_a_blank_line_rather_than_below_it() {
        // The caret lands at the start of the first cell, ready to type a
        // column heading.
        let out = run("Prose.\n\n", (7, 7), Command::Table);
        assert!(out.starts_with("Prose.\n| |Column"), "{out:?}");
        assert!(out.contains("|--------|--------|"), "{out:?}");
    }

    /// Offsets are characters. A selection measured in bytes would cut an
    /// accented word in half and corrupt the document.
    #[test]
    fn offsets_are_characters_not_bytes() {
        let text = "café au lait";
        assert_eq!(
            run(text, find(text, "au"), Command::Bold),
            "café **[au]** lait"
        );
    }

    /// A selection left over from a document that has since shrunk must clamp,
    /// not panic — this is what a menu item clicked after an undo looks like.
    #[test]
    fn offsets_past_the_end_are_clamped() {
        for command in ALL {
            let _ = apply("short", (900, 1200), *command);
        }
    }

    #[test]
    fn every_command_has_a_unique_id_that_round_trips() {
        let mut ids: Vec<&str> = ALL.iter().map(|command| command.id()).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "two commands share an id");

        for command in ALL {
            assert_eq!(Command::from_id(command.id()), Some(*command));
        }
        assert_eq!(Command::from_id("nonsense"), None);
    }

    /// Every command must write syntax the scanner styles — the same guarantee
    /// quill makes for its own formats, extended to the one command that is
    /// this crate's rather than quill's.
    #[test]
    fn every_command_produces_something_the_scanner_understands() {
        for command in ALL {
            let Some(edit) = apply("sample text", (0, 6), *command) else {
                assert_eq!(*command, Command::Paragraph, "{command:?} did nothing");
                continue;
            };
            let mut applied: String = "sample text".chars().take(edit.start).collect();
            applied.push_str(&edit.text);
            applied.extend("sample text".chars().skip(edit.end));

            if let Some(expected) = command.format().and_then(|format| format.style()) {
                let styles: Vec<quill::Style> = quill::parse(&applied)
                    .spans
                    .iter()
                    .map(|s| s.style)
                    .collect();
                assert!(
                    styles.contains(&expected),
                    "{command:?} wrote {applied:?}, which parses as {styles:?}"
                );
            }
        }
    }
}
