//! What the status bar counts.

use quill::Parsed;

/// The reading speed the estimate assumes. Silent prose in a language you read
/// fluently; the figure most reading-time indicators settle on.
pub const WORDS_PER_MINUTE: usize = 220;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub words: usize,
    /// Never zero: a document with one word in it takes a minute of somebody's
    /// attention, and "0 min read" is not information.
    pub reading_minutes: usize,
}

/// Count a document.
pub fn stats(text: &str) -> Stats {
    stats_with(text, &quill::parse(text))
}

/// [`stats`], reusing a scan the caller already has.
///
/// The counts are taken from the *stripped* text, so `**bold**` is one word
/// rather than one word wrapped in punctuation, a table reads as its cells, and
/// a frontmatter block does not inflate the estimate with metadata nobody
/// reads.
pub fn stats_with(text: &str, parsed: &Parsed) -> Stats {
    let prose = quill::strip_with(text, parsed);
    let words = prose
        .split_whitespace()
        .filter(|word| is_word(word))
        .count();

    Stats {
        words,
        reading_minutes: if words == 0 {
            0
        } else {
            1.max((words as f64 / WORDS_PER_MINUTE as f64).round() as usize)
        },
    }
}

/// Whether a whitespace-separated token counts as a word.
///
/// A bare `-` left by a stripped bullet, or the `|` of a table, is punctuation
/// standing alone. Counting it puts the total above what anyone would get
/// counting by hand, which is the only check a word count ever gets.
fn is_word(token: &str) -> bool {
    token.chars().any(|c| c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_counts_as_written() {
        assert_eq!(stats("one two three").words, 3);
        assert_eq!(stats("").words, 0);
        assert_eq!(stats("   \n\n  ").words, 0);
    }

    /// The count is of what is read, not of what is typed: syntax characters
    /// are not words, and must not split one word into two.
    #[test]
    fn markdown_syntax_is_not_counted() {
        assert_eq!(stats("A **bold** claim.").words, 3);
        assert_eq!(stats("# Heading").words, 1);
        assert_eq!(stats("- milk\n- bread\n").words, 2);
        assert_eq!(stats("[link text](https://example.com)").words, 2);
    }

    #[test]
    fn an_empty_document_has_no_reading_time() {
        assert_eq!(stats("").reading_minutes, 0);
    }

    #[test]
    fn a_short_document_still_takes_a_minute() {
        assert_eq!(stats("Hello.").reading_minutes, 1);
    }

    #[test]
    fn reading_time_scales_with_length() {
        let words = "word ".repeat(WORDS_PER_MINUTE * 3);
        assert_eq!(stats(&words).words, WORDS_PER_MINUTE * 3);
        assert_eq!(stats(&words).reading_minutes, 3);

        // To the nearest minute, so the figure tracks the length rather than
        // jumping a whole minute on one extra word.
        let over = "word ".repeat(WORDS_PER_MINUTE * 3 + 1);
        assert_eq!(stats(&over).reading_minutes, 3);
        let most_of_another = "word ".repeat(WORDS_PER_MINUTE * 3 + WORDS_PER_MINUTE / 2 + 1);
        assert_eq!(stats(&most_of_another).reading_minutes, 4);
    }

    /// Frontmatter is metadata. Counting it puts a reading time on the file
    /// header, which is not prose anyone reads.
    #[test]
    fn frontmatter_does_not_count() {
        let text = "---\ntitle: Notes\ntags: one two three four\n---\n\nBody text here.\n";
        assert_eq!(stats(text).words, 3);
    }
}
