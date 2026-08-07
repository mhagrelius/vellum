//! Vellum: a Markdown reader and editor for the GNOME desktop.
//!
//! The crate is the GNOME *frontend*. Everything that is not a widget lives in
//! [`model`], which is its own crate and links no toolkit — the reading styles,
//! the outline, the word count, the formatting commands and the file on disk.
//! A frontend for another platform links that crate and writes its own `ui/`;
//! nothing in `model` would have to move.
//!
//! The line between the two is drawn at *presentation*, not at convenience. The
//! eight reading styles are data in `model` because a style is a typographic
//! system, not a stylesheet; how a text view turns `h1_scale: 2.15` into a tag
//! is this crate's problem, and a `NSTextView` would solve it differently. Which
//! Markdown characters are syntax is [`quill`]'s answer for everyone; *when to
//! show them* is a mode, and modes are here.

/// The half that does not draw, under the name it would have had as a module.
pub use vellum_core as model;

pub mod ui;

/// Used for D-Bus, the desktop file and GSettings.
pub const APP_ID: &str = "us.hagreli.Vellum";
