//! The parts of a rendered document that are drawn rather than typed.
//!
//! A text tag can set a colour, a weight or a background, and cannot draw a
//! line. Everything a reading style asks for that is a *rule* — the bar down
//! the side of a quote, the hairline under a second-level heading, a thematic
//! break, the frame round a fenced block — is therefore painted by the view,
//! over line ranges named here.
//!
//! Lines rather than character offsets, because that is the coordinate a text
//! view can turn into a rectangle without laying the document out twice.

use quill::{LineState, Parsed, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoration {
    /// The bar down the left of a block quote. Consecutive quoted lines are one
    /// bar: three separate ones with gaps is not what a quote looks like.
    QuoteBar { first_line: usize, last_line: usize },
    /// The rule beneath a second-level heading, for the styles that ask for one.
    HeadingRule { line: usize },
    /// `---` on a line of its own.
    ThematicBreak { line: usize },
    /// The frame round a fenced code block, fences included.
    CodeFrame { first_line: usize, last_line: usize },
}

/// Every decoration in the document, in no particular order.
pub fn decorations(text: &str, parsed: &Parsed) -> Vec<Decoration> {
    let lines = line_index(text);
    let line_of = |offset: usize| match lines.binary_search(&offset) {
        Ok(line) => line,
        Err(next) => next.saturating_sub(1),
    };

    let mut quoted = Vec::new();
    let mut out = Vec::new();

    for span in &parsed.spans {
        match span.style {
            Style::Quote => quoted.push(line_of(span.start)),
            Style::Heading(2) => out.push(Decoration::HeadingRule {
                line: line_of(span.start),
            }),
            Style::Rule => out.push(Decoration::ThematicBreak {
                line: line_of(span.start),
            }),
            _ => {}
        }
    }

    quoted.sort_unstable();
    quoted.dedup();
    for run in runs(&quoted) {
        out.push(Decoration::QuoteBar {
            first_line: run.0,
            last_line: run.1,
        });
    }

    // A line's state is what it *begins* in, so the run of `Fence` lines starts
    // one line below the opening fence and ends on the closing one. An
    // unterminated fence runs to the end of the document, and is framed anyway —
    // it is what the document says, and hiding it would leave the block
    // unmarked while it is being typed.
    let fenced: Vec<usize> = parsed
        .line_states
        .iter()
        .enumerate()
        .filter(|(_, state)| **state == LineState::Fence)
        .map(|(line, _)| line)
        .collect();
    for run in runs(&fenced) {
        out.push(Decoration::CodeFrame {
            first_line: run.0.saturating_sub(1),
            last_line: run.1,
        });
    }

    out
}

/// Character offset of the start of every line.
fn line_index(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (offset, character) in text.chars().enumerate() {
        if character == '\n' {
            starts.push(offset + 1);
        }
    }
    starts
}

/// Collapse a sorted list of line numbers into inclusive runs.
fn runs(lines: &[usize]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for line in lines {
        match out.last_mut() {
            Some(run) if run.1 + 1 == *line => run.1 = *line,
            _ => out.push((*line, *line)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decorations_of(text: &str) -> Vec<Decoration> {
        decorations(text, &quill::parse(text))
    }

    #[test]
    fn consecutive_quoted_lines_are_one_bar() {
        let text = "> first\n> second\n> third\n";
        assert_eq!(
            decorations_of(text),
            [Decoration::QuoteBar {
                first_line: 0,
                last_line: 2
            }]
        );
    }

    #[test]
    fn quotes_split_by_prose_are_separate_bars() {
        let text = "> one\n\nProse.\n\n> two\n";
        let bars: Vec<Decoration> = decorations_of(text)
            .into_iter()
            .filter(|d| matches!(d, Decoration::QuoteBar { .. }))
            .collect();
        assert_eq!(
            bars,
            [
                Decoration::QuoteBar {
                    first_line: 0,
                    last_line: 0
                },
                Decoration::QuoteBar {
                    first_line: 4,
                    last_line: 4
                }
            ]
        );
    }

    #[test]
    fn a_fence_is_framed_from_its_opening_to_its_closing_line() {
        let text = "Prose.\n\n```rust\nlet x = 1;\nlet y = 2;\n```\n\nMore.\n";
        let frames: Vec<Decoration> = decorations_of(text)
            .into_iter()
            .filter(|d| matches!(d, Decoration::CodeFrame { .. }))
            .collect();
        assert_eq!(
            frames,
            [Decoration::CodeFrame {
                first_line: 2,
                last_line: 5
            }]
        );
    }

    /// Half-typed is the normal state of a document being written, and the
    /// frame must not vanish while the closing fence is still being reached.
    #[test]
    fn an_unterminated_fence_is_framed_to_the_end() {
        let text = "```\nstill typing\n";
        let frames: Vec<Decoration> = decorations_of(text)
            .into_iter()
            .filter(|d| matches!(d, Decoration::CodeFrame { .. }))
            .collect();
        assert_eq!(
            frames,
            [Decoration::CodeFrame {
                first_line: 0,
                last_line: 2
            }]
        );
    }

    #[test]
    fn second_level_headings_and_breaks_are_found_by_line() {
        let text = "# One\n\n## Two\n\n---\n\n### Three\n";
        let mut found: Vec<Decoration> = decorations_of(text);
        found.sort_by_key(|d| format!("{d:?}"));
        assert_eq!(
            found,
            [
                Decoration::HeadingRule { line: 2 },
                Decoration::ThematicBreak { line: 4 },
            ]
        );
    }

    /// The opening `---` of a document is frontmatter, not a break — the
    /// scanner draws that distinction and this must not undo it.
    #[test]
    fn leading_frontmatter_is_not_a_thematic_break() {
        let text = "---\ntitle: Notes\n---\n\nBody.\n";
        assert!(decorations_of(text).is_empty());
    }

    #[test]
    fn a_document_of_plain_prose_has_nothing_drawn() {
        assert!(decorations_of("Just prose.\n\nTwo paragraphs.\n").is_empty());
    }
}
