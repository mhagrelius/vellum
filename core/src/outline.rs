//! The heading outline shown in the sidebar.

use quill::{Parsed, Style};

/// One heading, as the sidebar needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1–6.
    pub level: u8,
    /// The heading without its hashes, for a row label.
    pub text: String,
    /// Character offset of the start of the heading's *line*, which is where
    /// clicking the row scrolls to. The scanner reports the content offset,
    /// after the hashes; scrolling to that would put the marker off-screen.
    pub offset: usize,
    /// Zero-based line number.
    pub line: usize,
}

/// Every heading in the document, in document order.
pub fn outline(text: &str, parsed: &Parsed) -> Vec<Heading> {
    let chars: Vec<char> = text.chars().collect();

    let mut headings: Vec<Heading> = parsed
        .spans
        .iter()
        .filter_map(|span| match span.style {
            Style::Heading(level) => {
                let content: String = chars
                    .get(span.start..span.end.min(chars.len()))?
                    .iter()
                    .collect();
                Some(Heading {
                    level,
                    text: content.trim().to_string(),
                    offset: line_start(&chars, span.start),
                    line: 0,
                })
            }
            _ => None,
        })
        .collect();

    // The scanner emits spans in the order it meets them, which is document
    // order — but a heading's inline spans are emitted alongside it, so sorting
    // is what guarantees the sidebar matches the page after any future change to
    // the scan order.
    headings.sort_by_key(|heading| heading.offset);

    let mut line = 0usize;
    let mut at = 0usize;
    for heading in &mut headings {
        while at < heading.offset {
            if chars[at] == '\n' {
                line += 1;
            }
            at += 1;
        }
        heading.line = line;
    }

    headings
}

/// The row the sidebar should mark active for a caret or viewport at `offset`:
/// the last heading at or before it.
pub fn active(headings: &[Heading], offset: usize) -> Option<usize> {
    headings
        .iter()
        .rposition(|heading| heading.offset <= offset)
}

fn line_start(chars: &[char], offset: usize) -> usize {
    chars[..offset.min(chars.len())]
        .iter()
        .rposition(|c| *c == '\n')
        .map(|newline| newline + 1)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline_of(text: &str) -> Vec<Heading> {
        outline(text, &quill::parse(text))
    }

    #[test]
    fn headings_come_back_in_order_with_their_levels() {
        let text = "# Title\n\nProse.\n\n## First\n\n### Deeper\n\n## Second\n";
        let headings = outline_of(text);

        let shape: Vec<(u8, &str)> = headings
            .iter()
            .map(|heading| (heading.level, heading.text.as_str()))
            .collect();
        assert_eq!(
            shape,
            [(1, "Title"), (2, "First"), (3, "Deeper"), (2, "Second"),]
        );
    }

    /// Scrolling to a heading must land on the hashes, not after them, or the
    /// line arrives at the top of the viewport already half off it.
    #[test]
    fn the_offset_is_the_line_start_not_the_content() {
        let text = "Prose.\n\n## Section\n";
        let heading = &outline_of(text)[0];
        assert_eq!(heading.offset, 8);
        assert_eq!(text.chars().nth(heading.offset), Some('#'));
        assert_eq!(heading.line, 2);
    }

    #[test]
    fn a_document_with_no_headings_has_an_empty_outline() {
        assert!(outline_of("Just prose.\n\nAnd more.\n").is_empty());
        assert!(outline_of("").is_empty());
    }

    /// A `#` inside a fence is code, and the scanner already knows that — this
    /// asserts the outline inherits it rather than re-deriving headings itself.
    #[test]
    fn hashes_inside_a_code_fence_are_not_headings() {
        let text = "# Real\n\n```sh\n# a comment\n```\n\n## Also real\n";
        let levels: Vec<u8> = outline_of(text).iter().map(|h| h.level).collect();
        assert_eq!(levels, [1, 2]);
    }

    #[test]
    fn the_active_row_is_the_last_heading_at_or_before_the_offset() {
        let text = "# One\n\nProse.\n\n## Two\n\nMore.\n";
        let headings = outline_of(text);

        assert_eq!(active(&headings, 0), Some(0));
        assert_eq!(active(&headings, 10), Some(0));
        // Exactly on the second heading's line.
        assert_eq!(active(&headings, headings[1].offset), Some(1));
        assert_eq!(active(&headings, text.chars().count()), Some(1));
    }

    /// Before the first heading there is no active row — a document that opens
    /// with prose should not light up a section it is not in.
    #[test]
    fn nothing_is_active_above_the_first_heading() {
        let text = "An opening paragraph.\n\n# One\n";
        assert_eq!(active(&outline_of(text), 0), None);
    }

    /// Character offsets, not bytes: an emoji above a heading would shift every
    /// row's target and scroll the sidebar to the wrong place.
    #[test]
    fn offsets_count_characters_not_bytes() {
        let text = "🌍🌍🌍\n\n## Section\n";
        let heading = &outline_of(text)[0];
        assert_eq!(heading.offset, 5);
        assert_eq!(text.chars().nth(heading.offset), Some('#'));
    }
}
