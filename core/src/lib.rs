//! Vellum's half that does not draw.
//!
//! Everything here is plain Rust over `&str`, `usize` character offsets and
//! plain data: no GTK, no display, no main loop, and therefore unit-testable
//! anywhere. It is its own crate rather than a module so that a frontend on
//! another platform can link it without linking libadwaita — which is the only
//! reliable way to keep a "backend" from quietly growing widget-shaped
//! assumptions.
//!
//! The split falls where it does because the interesting parts of a Markdown
//! editor are not the widgets:
//!
//! - [`style`] — the eight reading styles, as data. Adding one is adding a row
//!   to a table, in either frontend, with no rendering code touched.
//! - [`format`] — what Bold does to a selection, including undoing itself. Pure
//!   functions of text and two offsets.
//! - [`outline`] and [`decoration`] — the sidebar and the drawn rules, derived
//!   from one scan of the document.
//! - [`stats`] — the word count and the reading estimate.
//! - [`document`] — the open file, and the only place this crate touches disk.
//!
//! What is *not* here is the live-preview policy: which syntax characters are
//! revealed, and when. [`quill`] reports which characters are syntax and leaves
//! the decision to the view, because a reading mode that hides everything and an
//! editing mode that reveals the construct under the caret are two answers to
//! the same question, and both are presentation.

pub mod decoration;
pub mod document;
pub mod format;
pub mod outline;
pub mod stats;
pub mod style;

pub use document::Document;
pub use format::Command;
pub use style::{Mode, ReadingStyle};
