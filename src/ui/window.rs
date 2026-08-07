//! The window: outline, page, and the chrome around them.
//!
//! Everything visible here except the page follows the platform — libadwaita's
//! header bar, its split view, its popovers, its colours. Only the document
//! carries the reading style, because that is the one surface where the point
//! is *not* to look like every other window on the desktop.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::gio;
use gtk::glib::{self, clone, prelude::ToVariant};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use crate::model::style::{self, Mode};
use crate::model::{format, outline, stats, Command, Document};
use crate::ui::document_view::{DocumentView, ViewMode};
use crate::ui::outline_panel::OutlinePanel;
use crate::ui::style_popover::{Appearance, StylePopover};

/// Below this the outline stops being a panel and becomes an overlay.
const COLLAPSE_WIDTH: &str = "max-width: 700sp";

mod imp {
    use super::*;

    pub struct Window {
        pub document: RefCell<Document>,
        pub view: RefCell<Option<DocumentView>>,
        pub outline: RefCell<Option<OutlinePanel>>,
        pub split: RefCell<Option<adw::OverlaySplitView>>,
        pub title: RefCell<Option<adw::WindowTitle>>,
        pub content_header: RefCell<Option<adw::HeaderBar>>,
        pub style_popover: RefCell<Option<StylePopover>>,
        pub mode_group: RefCell<Option<adw::ToggleGroup>>,
        pub search_bar: RefCell<Option<gtk::SearchBar>>,
        pub search_entry: RefCell<Option<gtk::SearchEntry>>,
        pub search_toggle: RefCell<Option<gtk::ToggleButton>>,
        pub actions: RefCell<Option<gio::SimpleActionGroup>>,
        pub toasts: RefCell<Option<adw::ToastOverlay>>,
        pub save_banner: RefCell<Option<adw::Banner>>,
        pub status: RefCell<Option<StatusLabels>>,

        pub settings: RefCell<Option<gio::Settings>>,
        pub appearance: Cell<Appearance>,
        pub reading_style: Cell<&'static crate::model::ReadingStyle>,
        /// Set while the window is writing widget state, so the handlers it
        /// trips do not report those writes back as user choices.
        pub loading: Cell<bool>,
        /// Handler on the process-wide style manager, which outlives this
        /// window and so must be disconnected when it goes.
        pub style_handler: RefCell<Option<glib::SignalHandlerId>>,
    }

    /// Hand-written rather than derived: a reading style has no meaningful
    /// zero value, so the default is the one the app opens with.
    impl Default for Window {
        fn default() -> Self {
            Self {
                document: RefCell::default(),
                view: RefCell::default(),
                outline: RefCell::default(),
                split: RefCell::default(),
                title: RefCell::default(),
                content_header: RefCell::default(),
                style_popover: RefCell::default(),
                mode_group: RefCell::default(),
                search_bar: RefCell::default(),
                search_entry: RefCell::default(),
                search_toggle: RefCell::default(),
                actions: RefCell::default(),
                toasts: RefCell::default(),
                save_banner: RefCell::default(),
                status: RefCell::default(),
                settings: RefCell::default(),
                appearance: Cell::default(),
                reading_style: Cell::new(style::from_id(style::DEFAULT_ID)),
                loading: Cell::default(),
                style_handler: RefCell::default(),
            }
        }
    }

    #[derive(Default)]
    pub struct StatusLabels {
        pub words: gtk::Label,
        pub reading: gtk::Label,
        pub caret: gtk::Label,
        pub style: gtk::Label,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "VellumWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }

        fn dispose(&self) {
            if let Some(id) = self.style_handler.borrow_mut().take() {
                adw::StyleManager::default().disconnect(id);
            }
        }
    }

    impl WidgetImpl for Window {}

    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            let window = self.obj();
            if !self.document.borrow().is_modified() {
                window.remember_geometry();
                return glib::Propagation::Proceed;
            }
            window.confirm_discard(CloseAction::Close);
            glib::Propagation::Stop
        }
    }

    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

/// What to do once the "you have unsaved changes" question is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseAction {
    Close,
    OpenAnother,
    StartAnother,
}

impl Window {
    pub fn new(app: &impl IsA<gtk::Application>) -> Self {
        glib::Object::builder()
            .property("application", app.as_ref())
            .build()
    }

    /// The page, for tests and the preview harness to drive the way a reader
    /// would. `None` only before construction has finished.
    pub fn view(&self) -> Option<DocumentView> {
        self.imp().view.borrow().clone()
    }

    /// Show `path`, replacing whatever is open.
    pub fn open_path(&self, path: &Path) {
        match Document::open(path) {
            Ok(document) => self.adopt(document),
            Err(err) => {
                self.toast(&format!("Could not open {}: {err}", path.display()));
            }
        }
    }

    // ---- construction ---------------------------------------------------

    fn build(&self) {
        let imp = self.imp();
        imp.loading.set(true);

        self.set_title(Some("Vellum"));
        self.set_default_size(1100, 780);
        self.set_size_request(360, 320);

        self.load_settings();
        self.install_actions();

        let view = DocumentView::new();
        let outline_panel = OutlinePanel::new();
        let split = adw::OverlaySplitView::builder()
            .sidebar_width_fraction(0.21)
            .max_sidebar_width(320.0)
            .min_sidebar_width(220.0)
            .show_sidebar(true)
            .build();

        split.set_sidebar(Some(&self.build_sidebar(&outline_panel)));
        split.set_content(Some(&self.build_content(&view)));

        // The outline stops fitting beside the page long before the page stops
        // being readable, so past that width it overlays instead.
        if let Ok(condition) = adw::BreakpointCondition::parse(COLLAPSE_WIDTH) {
            let breakpoint = adw::Breakpoint::new(condition);
            breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
            self.add_breakpoint(breakpoint);
        }

        let banner = adw::Banner::builder().revealed(false).build();
        banner.add_css_class("error");

        let stack = gtk::Box::new(gtk::Orientation::Vertical, 0);
        stack.append(&banner);
        stack.append(&split);
        split.set_vexpand(true);

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&stack));
        self.set_content(Some(&toasts));

        imp.view.replace(Some(view.clone()));
        imp.outline.replace(Some(outline_panel.clone()));
        imp.split.replace(Some(split.clone()));
        imp.toasts.replace(Some(toasts));
        imp.save_banner.replace(Some(banner));

        self.wire(&view, &outline_panel, &split);
        self.follow_color_scheme();
        self.apply_reading_style();
        self.restore_view_state(&view, &split);
        self.refresh_all();

        imp.loading.set(false);
        view.grab_focus();
    }

    /// Put back the mode and the outline the window was last closed in.
    ///
    /// Separate from `load_settings`, which runs before there is a widget to
    /// tell: the reading style and the window size are properties of the window
    /// itself, and these two are properties of things it has not built yet.
    fn restore_view_state(&self, view: &DocumentView, split: &adw::OverlaySplitView) {
        let settings = self.imp().settings.borrow().clone();

        // The mode is settled whether or not there are settings to read it
        // from, so that the header toggle and the formatting commands agree
        // with the page in both cases.
        let mode = settings
            .as_ref()
            .map(|settings| ViewMode::from_id(&settings.string("view-mode")))
            .unwrap_or_default();
        view.set_view_mode(mode);
        if let Some(group) = self.imp().mode_group.borrow().clone() {
            group.set_active_name(Some(mode.id()));
        }
        self.set_formatting_enabled(mode.editable());

        if let Some(settings) = settings {
            split.set_show_sidebar(settings.boolean("show-outline"));
        }
    }

    fn build_sidebar(&self, panel: &OutlinePanel) -> adw::ToolbarView {
        let header = adw::HeaderBar::builder()
            .show_end_title_buttons(false)
            .title_widget(&adw::WindowTitle::new("Outline", ""))
            .build();

        let collapse = gtk::Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Hide Outline")
            .action_name("win.toggle-outline")
            .build();
        header.pack_start(&collapse);

        let search = gtk::ToggleButton::builder()
            .icon_name("edit-find-symbolic")
            .tooltip_text("Find in Document")
            .build();
        header.pack_end(&search);

        let toolbar = adw::ToolbarView::builder().content(panel).build();
        toolbar.add_top_bar(&header);
        toolbar.add_css_class("outline-sidebar");

        // The toggle drives the search bar the content pane owns, so it is
        // bound once that exists — see `wire`.
        self.imp().search_toggle.replace(Some(search));
        toolbar
    }

    fn build_content(&self, view: &DocumentView) -> adw::ToolbarView {
        let imp = self.imp();

        let header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .build();

        let show_sidebar = gtk::Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Show Outline")
            .action_name("win.toggle-outline")
            .build();
        header.pack_start(&show_sidebar);

        let modes = adw::ToggleGroup::builder().build();
        for mode in [ViewMode::Live, ViewMode::Source, ViewMode::Reading] {
            modes.add(
                adw::Toggle::builder()
                    .name(mode.id())
                    .label(mode.label())
                    .tooltip(match mode {
                        ViewMode::Live => "Hide Markdown except where the cursor is",
                        ViewMode::Source => "Show all Markdown",
                        ViewMode::Reading => "Hide all Markdown; read only",
                    })
                    .build(),
            );
        }
        header.pack_start(&modes);
        imp.mode_group.replace(Some(modes));

        let title = adw::WindowTitle::new("Untitled", "");
        header.set_title_widget(Some(&title));
        imp.title.replace(Some(title));

        let menu = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Main Menu")
            .primary(true)
            .menu_model(&primary_menu())
            .build();
        header.pack_end(&menu);

        let popover = StylePopover::new();
        let style_button = gtk::MenuButton::builder()
            .icon_name("preferences-desktop-appearance-symbolic")
            .tooltip_text("Reading Style")
            .popover(&popover)
            .build();
        header.pack_end(&style_button);
        imp.style_popover.replace(Some(popover));

        let save = gtk::Button::builder()
            .icon_name("document-save-symbolic")
            .tooltip_text("Save")
            .action_name("win.save")
            .build();
        header.pack_end(&save);

        // Search slides out from under the header, the way it does everywhere
        // else on the desktop.
        let entry = gtk::SearchEntry::builder()
            .hexpand(true)
            .placeholder_text("Find in document")
            .build();
        let search_bar = gtk::SearchBar::builder()
            .child(&entry)
            .key_capture_widget(self)
            .build();
        search_bar.connect_entry(&entry);
        imp.search_entry.replace(Some(entry));
        imp.search_bar.replace(Some(search_bar.clone()));

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(view)
            .build();

        let toolbar = adw::ToolbarView::builder()
            .top_bar_style(adw::ToolbarStyle::Raised)
            .content(&scroller)
            .build();
        toolbar.add_top_bar(&header);
        toolbar.add_top_bar(&search_bar);
        toolbar.add_bottom_bar(&self.build_status_bar());
        imp.content_header.replace(Some(header));
        toolbar
    }

    fn build_status_bar(&self) -> gtk::Widget {
        let labels = imp::StatusLabels::default();
        let bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(14)
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(12)
            .margin_end(12)
            .css_classes(["toolbar", "status-bar"])
            .build();

        for label in [&labels.words, &labels.reading] {
            label.add_css_class("caption");
            label.add_css_class("dimmed");
            label.add_css_class("numeric");
            bar.append(label);
        }

        let spacer = gtk::Box::builder().hexpand(true).build();
        bar.append(&spacer);

        for label in [&labels.caret, &labels.style] {
            label.add_css_class("caption");
            label.add_css_class("dimmed");
            label.add_css_class("numeric");
            bar.append(label);
        }

        self.imp().status.replace(Some(labels));
        bar.upcast()
    }

    /// Everything that has to happen when something changes.
    fn wire(&self, view: &DocumentView, panel: &OutlinePanel, split: &adw::OverlaySplitView) {
        let buffer = view.buffer();

        buffer.connect_changed(clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                if window.imp().loading.get() {
                    return;
                }
                window.imp().document.borrow_mut().set_text(window.text());
                window.refresh_all();
            }
        ));

        buffer.connect_mark_set(clone!(
            #[weak(rename_to = window)]
            self,
            move |buffer, _iter, mark| {
                if mark == &buffer.get_insert() {
                    window.refresh_caret();
                }
            }
        ));

        // The outline follows the reader down the document.
        if let Some(adjustment) = view.vadjustment() {
            adjustment.connect_value_changed(clone!(
                #[weak(rename_to = window)]
                self,
                move |_| window.refresh_active_heading()
            ));
        }

        panel.connect_closure(
            "heading-chosen",
            false,
            glib::closure_local!(
                #[watch(rename_to = window)]
                self,
                move |_panel: OutlinePanel, offset: u64| {
                    if let Some(view) = window.imp().view.borrow().clone() {
                        view.scroll_to_offset(offset as usize);
                    }
                }
            ),
        );

        // The two toggles are two positions of one control, so each reflects
        // whatever the other did.
        let sidebar_shown = |window: &Self, shown: bool| {
            if let Some(header) = window.imp().content_header.borrow().clone() {
                // libadwaita puts the window controls on the outermost header
                // bar of each side. With the outline away, this one is both.
                header.set_show_start_title_buttons(!shown);
            }
        };
        sidebar_shown(self, split.shows_sidebar());
        split.connect_show_sidebar_notify(clone!(
            #[weak(rename_to = window)]
            self,
            move |split| {
                sidebar_shown(&window, split.shows_sidebar() && !split.is_collapsed());
                window.remember("show-outline", &split.shows_sidebar().to_variant());
            }
        ));
        split.connect_collapsed_notify(clone!(
            #[weak(rename_to = window)]
            self,
            move |split| sidebar_shown(&window, split.shows_sidebar() && !split.is_collapsed())
        ));

        if let Some(modes) = self.imp().mode_group.borrow().clone() {
            modes.connect_active_name_notify(clone!(
                #[weak(rename_to = window)]
                self,
                move |modes| {
                    if window.imp().loading.get() {
                        return;
                    }
                    let Some(name) = modes.active_name() else {
                        return;
                    };
                    window.set_view_mode(ViewMode::from_id(&name));
                }
            ));
        }

        if let Some(entry) = self.imp().search_entry.borrow().clone() {
            entry.connect_search_changed(clone!(
                #[weak(rename_to = window)]
                self,
                move |entry| window.find(&entry.text(), true, false)
            ));
            entry.connect_next_match(clone!(
                #[weak(rename_to = window)]
                self,
                move |entry| window.find(&entry.text(), true, true)
            ));
            entry.connect_previous_match(clone!(
                #[weak(rename_to = window)]
                self,
                move |entry| window.find(&entry.text(), false, true)
            ));
        }
        if let (Some(bar), Some(toggle)) = (
            self.imp().search_bar.borrow().clone(),
            self.imp().search_toggle.borrow().clone(),
        ) {
            bar.bind_property("search-mode-enabled", &toggle, "active")
                .bidirectional()
                .sync_create()
                .build();
        }

        // The standard context menu already carries Cut, Copy, Paste, Select
        // All and the undo pair. The formatting commands are appended to it
        // rather than replacing it, so nothing the platform provides is lost.
        view.set_extra_menu(Some(&format_menu()));
    }

    // ---- actions --------------------------------------------------------

    fn install_actions(&self) {
        let actions = gio::SimpleActionGroup::new();

        let toggle_outline = gio::SimpleAction::new("toggle-outline", None);
        toggle_outline.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                if let Some(split) = window.imp().split.borrow().clone() {
                    split.set_show_sidebar(!split.shows_sidebar());
                }
            }
        ));
        actions.add_action(&toggle_outline);

        let reading_style = gio::SimpleAction::new_stateful(
            "reading-style",
            Some(glib::VariantTy::STRING),
            &style::DEFAULT_ID.to_variant(),
        );
        reading_style.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |action, parameter| {
                let Some(id) = parameter.and_then(|value| value.get::<String>()) else {
                    return;
                };
                action.set_state(&id.to_variant());
                window.imp().reading_style.set(style::from_id(&id));
                window.remember("reading-style", &id.to_variant());
                window.apply_reading_style();
            }
        ));
        actions.add_action(&reading_style);

        let appearance = gio::SimpleAction::new_stateful(
            "appearance",
            Some(glib::VariantTy::STRING),
            &Appearance::default().id().to_variant(),
        );
        appearance.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |action, parameter| {
                let Some(id) = parameter.and_then(|value| value.get::<String>()) else {
                    return;
                };
                action.set_state(&id.to_variant());
                let appearance = Appearance::from_id(&id);
                window.imp().appearance.set(appearance);
                window.remember("appearance", &id.to_variant());
                // The chrome and the page change together. Setting the colour
                // scheme moves the chrome, and the notify it raises brings the
                // page with it.
                adw::StyleManager::default().set_color_scheme(appearance.color_scheme());
                window.apply_reading_style();
            }
        ));
        actions.add_action(&appearance);

        let mode = gio::SimpleAction::new_stateful(
            "mode",
            Some(glib::VariantTy::STRING),
            &ViewMode::default().id().to_variant(),
        );
        mode.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |_, parameter| {
                let Some(id) = parameter.and_then(|value| value.get::<String>()) else {
                    return;
                };
                window.set_view_mode(ViewMode::from_id(&id));
            }
        ));
        actions.add_action(&mode);

        let format = gio::SimpleAction::new("format", Some(glib::VariantTy::STRING));
        format.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |_, parameter| {
                let Some(id) = parameter.and_then(|value| value.get::<String>()) else {
                    return;
                };
                let (Some(command), Some(view)) =
                    (Command::from_id(&id), window.imp().view.borrow().clone())
                else {
                    return;
                };
                view.apply_command(command);
            }
        ));
        actions.add_action(&format);

        let pick_style = gio::SimpleAction::new("pick-style", None);
        pick_style.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                if let Some(popover) = window.imp().style_popover.borrow().clone() {
                    popover.popup();
                }
            }
        ));
        actions.add_action(&pick_style);

        let find = gio::SimpleAction::new("find", None);
        find.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| {
                if let Some(bar) = window.imp().search_bar.borrow().clone() {
                    bar.set_search_mode(true);
                    if let Some(entry) = window.imp().search_entry.borrow().clone() {
                        entry.grab_focus();
                    }
                }
            }
        ));
        actions.add_action(&find);

        for (name, handler) in [
            ("new", CloseAction::StartAnother),
            ("open", CloseAction::OpenAnother),
        ] {
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(clone!(
                #[weak(rename_to = window)]
                self,
                move |_, _| {
                    if window.imp().document.borrow().is_modified() {
                        window.confirm_discard(handler);
                    } else {
                        window.proceed(handler);
                    }
                }
            ));
            actions.add_action(&action);
        }

        let save = gio::SimpleAction::new("save", None);
        save.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| window.save(None)
        ));
        actions.add_action(&save);

        let save_as = gio::SimpleAction::new("save-as", None);
        save_as.connect_activate(clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| window.choose_save_path()
        ));
        actions.add_action(&save_as);

        self.insert_action_group("win", Some(&actions));
        self.imp().actions.replace(Some(actions));
    }

    // ---- state ----------------------------------------------------------

    fn load_settings(&self) {
        let imp = self.imp();
        // Running from the build tree, before `install.sh` has compiled the
        // schema, is a normal thing to do. Without a schema the app opens with
        // its defaults and forgets them again, rather than aborting.
        let settings = gio::SettingsSchemaSource::default()
            .and_then(|source| source.lookup(crate::APP_ID, true))
            .map(|_| gio::Settings::new(crate::APP_ID));

        if let Some(settings) = &settings {
            imp.reading_style
                .set(style::from_id(&settings.string("reading-style")));
            imp.appearance
                .set(Appearance::from_id(&settings.string("appearance")));
            self.set_default_size(
                settings.int("window-width").max(360),
                settings.int("window-height").max(320),
            );
            if settings.boolean("window-maximized") {
                self.maximize();
            }
        }
        imp.settings.replace(settings);

        adw::StyleManager::default().set_color_scheme(imp.appearance.get().color_scheme());
    }

    fn remember(&self, key: &str, value: &glib::Variant) {
        if self.imp().loading.get() {
            return;
        }
        if let Some(settings) = self.imp().settings.borrow().as_ref() {
            let _ = settings.set_value(key, value);
        }
    }

    fn remember_geometry(&self) {
        let Some(settings) = self.imp().settings.borrow().clone() else {
            return;
        };
        let _ = settings.set_boolean("window-maximized", self.is_maximized());
        if !self.is_maximized() {
            let _ = settings.set_int("window-width", self.width());
            let _ = settings.set_int("window-height", self.height());
        }
    }

    /// Track the desktop's light/dark preference, for as long as the user has
    /// not overridden it.
    fn follow_color_scheme(&self) {
        let manager = adw::StyleManager::default();
        let id = manager.connect_dark_notify(clone!(
            #[weak(rename_to = window)]
            self,
            move |_| window.apply_reading_style()
        ));
        self.imp().style_handler.replace(Some(id));
    }

    fn mode(&self) -> Mode {
        self.imp()
            .appearance
            .get()
            .resolve(adw::StyleManager::default().is_dark())
    }

    fn apply_reading_style(&self) {
        let imp = self.imp();
        let style = imp.reading_style.get();
        let mode = self.mode();

        if let Some(view) = imp.view.borrow().clone() {
            view.set_reading_style(style, mode);
        }
        if let Some(popover) = imp.style_popover.borrow().clone() {
            popover.set_selected(style.id);
            popover.set_appearance(imp.appearance.get());
            popover.set_mode(mode);
        }
        if let Some(status) = imp.status.borrow().as_ref() {
            status.style.set_label(style.label);
        }
        // The action carries the state the popover reflects, and the style can
        // arrive from settings or a keyboard shortcut as well as from a card.
        if let Some(actions) = imp.actions.borrow().clone() {
            gio::prelude::ActionGroupExt::change_action_state(
                &actions,
                "reading-style",
                &style.id.to_variant(),
            );
        }
    }

    fn set_view_mode(&self, mode: ViewMode) {
        let imp = self.imp();
        let Some(view) = imp.view.borrow().clone() else {
            return;
        };
        if view.view_mode() == mode {
            return;
        }
        view.set_view_mode(mode);
        self.remember("view-mode", &mode.id().to_variant());

        let was_loading = imp.loading.replace(true);
        if let Some(group) = imp.mode_group.borrow().clone() {
            group.set_active_name(Some(mode.id()));
        }
        imp.loading.set(was_loading);

        // Reading mode is a reader. The formatting commands have nothing to
        // write into, so they go insensitive rather than silently doing
        // nothing when chosen.
        self.set_formatting_enabled(mode.editable());
    }

    fn set_formatting_enabled(&self, enabled: bool) {
        let Some(actions) = self.imp().actions.borrow().clone() else {
            return;
        };
        if let Some(action) = gio::prelude::ActionMapExt::lookup_action(&actions, "format")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(enabled);
        }
    }

    // ---- the document ---------------------------------------------------

    fn text(&self) -> String {
        self.imp()
            .view
            .borrow()
            .as_ref()
            .map(|view| view.text())
            .unwrap_or_default()
    }

    fn adopt(&self, document: Document) {
        let imp = self.imp();
        imp.loading.set(true);
        if let Some(view) = imp.view.borrow().clone() {
            view.buffer().set_text(document.text());
            view.buffer().place_cursor(&view.buffer().start_iter());
        }
        imp.document.replace(document);
        imp.loading.set(false);
        self.set_save_error(None);
        self.refresh_all();
    }

    fn save(&self, path: Option<PathBuf>) {
        let imp = self.imp();
        imp.document.borrow_mut().set_text(self.text());

        let result = match path {
            Some(path) => imp.document.borrow_mut().save_as(&path),
            None if imp.document.borrow().path().is_some() => imp.document.borrow_mut().save(),
            None => {
                self.choose_save_path();
                return;
            }
        };

        match result {
            Ok(()) => {
                self.set_save_error(None);
                self.refresh_title();
            }
            // A document that will not save is an ongoing condition, not an
            // event: a toast is missed while typing, and the cost of missing it
            // is the work since the last save.
            Err(err) => self.set_save_error(Some(&err.to_string())),
        }
    }

    fn choose_save_path(&self) {
        let dialog = gtk::FileDialog::builder()
            .title("Save Document")
            .initial_name(
                self.imp()
                    .document
                    .borrow()
                    .path()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled.md".to_string()),
            )
            .filters(&markdown_filters())
            .build();

        dialog.save(
            Some(self),
            gio::Cancellable::NONE,
            clone!(
                #[weak(rename_to = window)]
                self,
                move |result| match result {
                    Ok(file) =>
                        if let Some(path) = file.path() {
                            window.save(Some(path));
                        },
                    Err(err) =>
                        if !err.matches(gtk::DialogError::Dismissed) {
                            window.toast(&format!("Could not save: {}", err.message()));
                        },
                }
            ),
        );
    }

    fn choose_open_path(&self) {
        let dialog = gtk::FileDialog::builder()
            .title("Open Document")
            .filters(&markdown_filters())
            .build();

        dialog.open(
            Some(self),
            gio::Cancellable::NONE,
            clone!(
                #[weak(rename_to = window)]
                self,
                move |result| match result {
                    Ok(file) =>
                        if let Some(path) = file.path() {
                            window.open_path(&path);
                        },
                    Err(err) =>
                        if !err.matches(gtk::DialogError::Dismissed) {
                            window.toast(&format!("Could not open: {}", err.message()));
                        },
                }
            ),
        );
    }

    fn proceed(&self, action: CloseAction) {
        match action {
            CloseAction::Close => {
                self.remember_geometry();
                self.destroy();
            }
            CloseAction::OpenAnother => self.choose_open_path(),
            CloseAction::StartAnother => self.adopt(Document::blank()),
        }
    }

    /// Ask before losing work. Cancel first, the specific verb last, and the
    /// destructive one marked as such.
    fn confirm_discard(&self, action: CloseAction) {
        let name = self.imp().document.borrow().title();
        let dialog = adw::AlertDialog::new(
            Some(&format!("Save changes to {name}?")),
            Some("Your changes will be lost if you do not save them."),
        );
        dialog.add_response("cancel", "_Cancel");
        dialog.add_response("discard", "_Discard");
        dialog.add_response("save", "_Save");
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");

        dialog.connect_response(
            None,
            clone!(
                #[weak(rename_to = window)]
                self,
                move |_, response| match response {
                    "discard" => window.proceed(action),
                    "save" => {
                        window.save(None);
                        // Only carry on once it is actually on disk; a failed
                        // save must not take the document away with it.
                        if !window.imp().document.borrow().is_modified() {
                            window.proceed(action);
                        }
                    }
                    _ => {}
                }
            ),
        );
        dialog.present(Some(self));
    }

    // ---- what the chrome says -------------------------------------------

    fn refresh_all(&self) {
        let Some(view) = self.imp().view.borrow().clone() else {
            return;
        };
        let text = view.text();
        let parsed = view.parsed();

        if let Some(panel) = self.imp().outline.borrow().clone() {
            panel.set_headings(outline::outline(&text, &parsed));
        }
        if let Some(status) = self.imp().status.borrow().as_ref() {
            let counted = stats::stats_with(&text, &parsed);
            status.words.set_label(&match counted.words {
                1 => "1 word".to_string(),
                words => format!("{words} words"),
            });
            status.reading.set_label(&match counted.reading_minutes {
                0 => String::new(),
                minutes => format!("{minutes} min read"),
            });
        }
        self.refresh_title();
        self.refresh_caret();
        self.refresh_active_heading();
    }

    fn refresh_title(&self) {
        let imp = self.imp();
        let document = imp.document.borrow();
        let name = document.title();
        let modified = document.is_modified();

        let subtitle = match (document.location(), document.modified_at()) {
            (location, Some(when)) if !location.is_empty() => format!(
                "{location} — edited {}",
                crate::model::document::relative_to_now(when)
            ),
            (location, _) if !location.is_empty() => location,
            _ => "Not saved yet".to_string(),
        };

        if let Some(title) = imp.title.borrow().as_ref() {
            title.set_title(&if modified {
                format!("{name} •")
            } else {
                name.clone()
            });
            title.set_subtitle(&subtitle);
        }
        self.set_title(Some(&name));
    }

    fn refresh_caret(&self) {
        let (Some(view), status) = (self.imp().view.borrow().clone(), self.imp().status.borrow())
        else {
            return;
        };
        if let Some(status) = status.as_ref() {
            let (line, column) = view.caret_position();
            status.caret.set_label(&format!("Ln {line}, Col {column}"));
        }
    }

    fn refresh_active_heading(&self) {
        let (Some(view), Some(panel)) = (
            self.imp().view.borrow().clone(),
            self.imp().outline.borrow().clone(),
        ) else {
            return;
        };
        let text = view.text();
        let headings = outline::outline(&text, &view.parsed());
        panel.set_active(outline::active(&headings, view.top_offset()));
    }

    fn set_save_error(&self, message: Option<&str>) {
        let Some(banner) = self.imp().save_banner.borrow().clone() else {
            return;
        };
        match message {
            Some(message) => {
                banner.set_title(&format!("Not saved — {message}"));
                banner.set_revealed(true);
            }
            None => banner.set_revealed(false),
        }
    }

    fn toast(&self, message: &str) {
        if let Some(overlay) = self.imp().toasts.borrow().clone() {
            overlay.add_toast(adw::Toast::new(message));
        }
    }

    /// Find `needle`, from the caret onwards.
    ///
    /// `advance` is false while the query is still being typed, so the match
    /// under the caret stays put rather than the view running away a character
    /// at a time.
    fn find(&self, needle: &str, forwards: bool, advance: bool) {
        let Some(view) = self.imp().view.borrow().clone() else {
            return;
        };
        if needle.is_empty() {
            return;
        }
        let buffer = view.buffer();
        let from = match buffer.selection_bounds() {
            // Searching on from the current match, rather than finding it
            // again, is what makes Enter mean "next".
            Some((_, end)) if advance && forwards => end,
            Some((start, _)) => start,
            None => buffer.iter_at_mark(&buffer.get_insert()),
        };

        let flags = gtk::TextSearchFlags::CASE_INSENSITIVE | gtk::TextSearchFlags::VISIBLE_ONLY;
        let found = if forwards {
            from.forward_search(needle, flags, None)
                .or_else(|| buffer.start_iter().forward_search(needle, flags, None))
        } else {
            from.backward_search(needle, flags, None)
                .or_else(|| buffer.end_iter().backward_search(needle, flags, None))
        };

        if let Some((start, end)) = found {
            buffer.select_range(&start, &end);
            view.scroll_to_iter(&mut start.clone(), 0.15, false, 0.0, 0.0);
        }
    }
}

fn markdown_filters() -> gio::ListStore {
    let markdown = gtk::FileFilter::new();
    markdown.set_name(Some("Markdown"));
    for pattern in ["*.md", "*.markdown", "*.mdown", "*.mkd", "*.txt"] {
        markdown.add_pattern(pattern);
    }
    let all = gtk::FileFilter::new();
    all.set_name(Some("All Files"));
    all.add_pattern("*");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&markdown);
    filters.append(&all);
    filters
}

/// Appended to the text view's own context menu, so Cut, Copy, Paste, Select
/// All and Undo stay where the platform put them.
fn format_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let mut section = gio::Menu::new();
    let mut previous: Option<&'static str> = None;
    for command in format::ALL {
        // The model lists the commands in menu order; the groups are the three
        // kinds — inline, block-level, and inserted blocks.
        let group = match command {
            Command::Bold
            | Command::Italic
            | Command::Strikethrough
            | Command::Code
            | Command::Link => "inline",
            Command::Paragraph | Command::Heading(_) => "heading",
            _ => "block",
        };
        if previous.is_some_and(|last| last != group) {
            menu.append_section(None, &section);
            section = gio::Menu::new();
        }
        previous = Some(group);

        let item = gio::MenuItem::new(Some(command.label()), None);
        item.set_action_and_target_value(Some("win.format"), Some(&command.id().to_variant()));
        section.append_item(&item);
    }
    menu.append_section(None, &section);

    let styles = gio::Menu::new();
    styles.append(Some("Reading Style…"), Some("win.pick-style"));
    menu.append_section(None, &styles);

    menu
}

fn primary_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let file = gio::Menu::new();
    file.append(Some("_New"), Some("win.new"));
    file.append(Some("_Open…"), Some("win.open"));
    file.append(Some("_Save"), Some("win.save"));
    file.append(Some("Save _As…"), Some("win.save-as"));
    menu.append_section(None, &file);

    let view = gio::Menu::new();
    view.append(Some("_Find…"), Some("win.find"));
    view.append(Some("_Outline"), Some("win.toggle-outline"));
    view.append(Some("_Reading Style…"), Some("win.pick-style"));
    menu.append_section(None, &view);

    let app = gio::Menu::new();
    app.append(Some("_Keyboard Shortcuts"), Some("app.shortcuts"));
    app.append(Some("_About Vellum"), Some("app.about"));
    menu.append_section(None, &app);

    menu
}
