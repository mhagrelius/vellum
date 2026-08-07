//! The GNOME frontend.
//!
//! The chrome is libadwaita's, unmodified: header bars, a split view, popovers,
//! the platform's own colours and spacing. `style.css` holds only what has no
//! style class of its own. The one surface that does not follow the desktop is
//! [`document_view`], which is the point of the app.

pub mod application;
pub mod document_view;
pub mod outline_panel;
pub mod style_popover;
pub mod window;

pub use application::Application;
pub use document_view::{DocumentView, ViewMode};
pub use window::Window;
