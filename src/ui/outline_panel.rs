//! The heading outline, in the sidebar.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::subclass::Signal;
use gtk::glib::{self, clone};
use gtk::pango;
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use crate::model::outline::Heading;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct OutlinePanel {
        pub list: RefCell<Option<gtk::ListBox>>,
        pub headings: RefCell<Vec<Heading>>,
        pub active: Cell<Option<usize>>,
        /// Set while the panel is selecting a row itself, so that following the
        /// scroll position does not read as the user having clicked it.
        pub syncing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OutlinePanel {
        const NAME: &'static str = "VellumOutlinePanel";
        type Type = super::OutlinePanel;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for OutlinePanel {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // A row was chosen. Carries the character offset of the
                    // heading's line, which is what the view scrolls to.
                    Signal::builder("heading-chosen")
                        .param_types([u64::static_type()])
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for OutlinePanel {}
    impl BoxImpl for OutlinePanel {}
}

glib::wrapper! {
    pub struct OutlinePanel(ObjectSubclass<imp::OutlinePanel>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for OutlinePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl OutlinePanel {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("orientation", gtk::Orientation::Vertical)
            .build()
    }

    /// Rebuild the outline.
    ///
    /// The rows are thrown away and made again rather than diffed: the outline
    /// of a document being edited changes shape on almost every keystroke that
    /// touches a heading, and a handful of rows is cheaper to rebuild than to
    /// reconcile.
    pub fn set_headings(&self, headings: Vec<Heading>) {
        let imp = self.imp();
        let Some(list) = imp.list.borrow().clone() else {
            return;
        };

        if *imp.headings.borrow() == headings {
            return;
        }

        while let Some(row) = list.first_child() {
            list.remove(&row);
        }
        for heading in &headings {
            list.append(&Self::row(heading));
        }
        imp.headings.replace(headings);
        // The row that was active may no longer exist.
        let active = imp.active.get();
        imp.active.set(None);
        self.set_active(active);
    }

    /// Mark the section the reader is in, without scrolling the document.
    pub fn set_active(&self, active: Option<usize>) {
        let imp = self.imp();
        let Some(list) = imp.list.borrow().clone() else {
            return;
        };
        let active = active.filter(|index| *index < imp.headings.borrow().len());
        if imp.active.get() == active {
            return;
        }
        imp.active.set(active);

        imp.syncing.set(true);
        match active.and_then(|index| list.row_at_index(index as i32)) {
            Some(row) => list.select_row(Some(&row)),
            None => list.select_row(gtk::ListBoxRow::NONE),
        }
        imp.syncing.set(false);
    }

    fn build(&self) {
        let imp = self.imp();
        self.add_css_class("outline-panel");

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        list.update_property(&[gtk::accessible::Property::Label("Document outline")]);

        list.set_placeholder(Some(&Self::placeholder()));

        list.connect_row_selected(clone!(
            #[weak(rename_to = panel)]
            self,
            move |_, row| {
                let imp = panel.imp();
                if imp.syncing.get() {
                    return;
                }
                let Some(row) = row else { return };
                let index = row.index();
                if index < 0 {
                    return;
                }
                let offset = imp
                    .headings
                    .borrow()
                    .get(index as usize)
                    .map(|heading| heading.offset as u64);
                if let Some(offset) = offset {
                    imp.active.set(Some(index as usize));
                    panel.emit_by_name::<()>("heading-chosen", &[&offset]);
                }
            }
        ));

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        self.append(&scroller);

        imp.list.replace(Some(list));
    }

    fn row(heading: &Heading) -> gtk::ListBoxRow {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        // Sub-headings step in, so the shape of the document is visible in the
        // shape of the list.
        let indent = i32::from(heading.level.saturating_sub(1)) * 12;
        let label = gtk::Label::builder()
            .label(&heading.text)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .margin_start(indent)
            .build();
        if heading.level == 1 {
            label.add_css_class("heading");
        }
        row.append(&label);

        let level = gtk::Label::builder()
            .label(format!("H{}", heading.level))
            .css_classes(["caption", "dimmed", "numeric"])
            .build();
        row.append(&level);

        let list_row = gtk::ListBoxRow::builder().child(&row).build();
        list_row.update_property(&[gtk::accessible::Property::Label(&format!(
            "Heading level {}: {}",
            heading.level, heading.text
        ))]);
        list_row
    }

    /// What the sidebar says about a document with no headings. Compact rather
    /// than an `AdwStatusPage`: this is a 260-pixel column, and a full status
    /// page in it reads as an error.
    fn placeholder() -> gtk::Widget {
        let box_ = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .valign(gtk::Align::Center)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();
        box_.append(
            &gtk::Image::builder()
                .icon_name("view-list-symbolic")
                .pixel_size(32)
                .css_classes(["dimmed"])
                .build(),
        );
        box_.append(
            &gtk::Label::builder()
                .label("No headings")
                .wrap(true)
                .justify(gtk::Justification::Center)
                .css_classes(["caption", "dimmed"])
                .build(),
        );
        box_.upcast()
    }
}
