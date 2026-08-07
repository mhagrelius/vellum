//! The application: one window per document, and the actions that belong to
//! the app rather than to any window.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use gtk::{gdk, gio};

use crate::ui::window::Window;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Application;

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "VellumApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn startup(&self) {
            // Chain up first: the toolkit initialises in the parent handler,
            // and a window built before it does not work.
            self.parent_startup();
            self.obj().load_css();
            self.obj().install_actions();
        }

        fn activate(&self) {
            self.parent_activate();
            self.obj().present_window(None);
        }

        /// Files from the command line, the file manager, or a `.desktop`
        /// launch. One window each, so two documents are two windows.
        fn open(&self, files: &[gio::File], _hint: &str) {
            for file in files {
                self.obj().present_window(file.path().as_deref());
            }
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl Application {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", crate::APP_ID)
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .build()
    }

    fn present_window(&self, path: Option<&std::path::Path>) {
        let window = Window::new(self);
        if let Some(path) = path {
            window.open_path(path);
        }
        window.present();
    }

    fn load_css(&self) {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(include_str!("style.css"));
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn install_actions(&self) {
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| app.quit())
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| app.show_about())
            .build();
        let shortcuts = gio::ActionEntry::builder("shortcuts")
            .activate(|app: &Self, _, _| app.show_shortcuts())
            .build();
        self.add_action_entries([quit, about, shortcuts]);

        for (action, accels) in accelerators() {
            self.set_accels_for_action(action, accels);
        }
    }

    fn active(&self) -> Option<Window> {
        self.active_window().and_downcast::<Window>()
    }

    fn show_about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("Vellum")
            .application_icon(crate::APP_ID)
            .version(env!("CARGO_PKG_VERSION"))
            .developer_name("Matthew Hagrelius")
            .license_type(gtk::License::Gpl30)
            .comments(
                "A Markdown reader and editor that shows a document as it would be \
                 published, in one of eight typographic reading styles — while it is \
                 still the file you are editing.",
            )
            .website("https://github.com/mhagrelius/vellum")
            .issue_url("https://github.com/mhagrelius/vellum/issues")
            .build();
        about.present(self.active().as_ref());
    }

    fn show_shortcuts(&self) {
        let dialog = adw::ShortcutsDialog::new();

        let file = adw::ShortcutsSection::new(Some("Document"));
        file.add(adw::ShortcutsItem::new("New", "<Control>n"));
        file.add(adw::ShortcutsItem::new("Open", "<Control>o"));
        file.add(adw::ShortcutsItem::new("Save", "<Control>s"));
        file.add(adw::ShortcutsItem::new("Save As", "<Control><Shift>s"));
        file.add(adw::ShortcutsItem::new("Close Window", "<Control>w"));
        dialog.add(file);

        let view = adw::ShortcutsSection::new(Some("View"));
        view.add(adw::ShortcutsItem::new("Live Mode", "<Control>1"));
        view.add(adw::ShortcutsItem::new("Source Mode", "<Control>2"));
        view.add(adw::ShortcutsItem::new("Reading Mode", "<Control>3"));
        view.add(adw::ShortcutsItem::new("Show or Hide Outline", "F9"));
        view.add(adw::ShortcutsItem::new(
            "Reading Style",
            "<Control><Shift>t",
        ));
        view.add(adw::ShortcutsItem::new("Find in Document", "<Control>f"));
        dialog.add(view);

        let formatting = adw::ShortcutsSection::new(Some("Formatting"));
        formatting.add(adw::ShortcutsItem::new("Bold", "<Control>b"));
        formatting.add(adw::ShortcutsItem::new("Italic", "<Control>i"));
        formatting.add(adw::ShortcutsItem::new("Inline Code", "<Control>e"));
        formatting.add(adw::ShortcutsItem::new("Insert Link", "<Control>k"));
        formatting.add(adw::ShortcutsItem::new("Heading 1, 2, 3", "<Alt>1"));
        formatting.add(adw::ShortcutsItem::new("Paragraph", "<Alt>0"));
        dialog.add(formatting);

        dialog.present(self.active().as_ref());
    }
}

/// Every accelerator in one table, so the shortcuts dialog and the bindings
/// cannot drift apart without it being visible here.
///
/// The view modes take Ctrl+1…3 and the headings take Alt+1…3: a reader
/// switches modes far more often than a writer changes a heading level, and
/// Ctrl+number is the pair of hands that reaches for a mode.
fn accelerators() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("app.quit", &["<Control>q"]),
        ("win.new", &["<Control>n"]),
        ("win.open", &["<Control>o"]),
        ("win.save", &["<Control>s"]),
        ("win.save-as", &["<Control><Shift>s"]),
        ("win.find", &["<Control>f"]),
        ("win.toggle-outline", &["F9"]),
        ("win.pick-style", &["<Control><Shift>t"]),
        ("win.mode::live", &["<Control>1"]),
        ("win.mode::source", &["<Control>2"]),
        ("win.mode::reading", &["<Control>3"]),
        ("win.format::bold", &["<Control>b"]),
        ("win.format::italic", &["<Control>i"]),
        ("win.format::code", &["<Control>e"]),
        ("win.format::link", &["<Control>k"]),
        ("win.format::paragraph", &["<Alt>0"]),
        ("win.format::heading-1", &["<Alt>1"]),
        ("win.format::heading-2", &["<Alt>2"]),
        ("win.format::heading-3", &["<Alt>3"]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two actions on one accelerator is a binding that silently does the
    /// wrong one.
    #[test]
    fn no_accelerator_is_bound_twice() {
        let mut seen: Vec<&str> = accelerators()
            .iter()
            .flat_map(|(_, accels)| accels.iter().copied())
            .collect();
        let count = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), count, "an accelerator is bound to two actions");
    }

    /// Every formatting accelerator has to name a command that exists, or the
    /// key press reaches an action with a target nothing answers to.
    #[test]
    fn formatting_accelerators_name_real_commands() {
        for (action, _) in accelerators() {
            let Some(target) = action.strip_prefix("win.format::") else {
                continue;
            };
            assert!(
                crate::model::Command::from_id(target).is_some(),
                "{action} names no command"
            );
        }
    }

    #[test]
    fn mode_accelerators_name_real_modes() {
        for (action, _) in accelerators() {
            let Some(target) = action.strip_prefix("win.mode::") else {
                continue;
            };
            assert_eq!(
                crate::ui::document_view::ViewMode::from_id(target).id(),
                target,
                "{action} names no view mode"
            );
        }
    }
}
