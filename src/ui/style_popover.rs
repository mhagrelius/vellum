//! The reading-style picker: a grid of live specimens, and light/dark.
//!
//! Thumbnails rather than a list of names, because a reading style is a thing
//! you recognise by sight and cannot recall by name. Each one is drawn in the
//! real face, on the real page colour, with the real heading treatment — so
//! what the grid shows is what the document becomes, not a swatch standing in
//! for it.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::{self, clone, prelude::ToVariant};
use gtk::{gdk, graphene, gsk, pango};
use std::cell::{Cell, RefCell};

use crate::model::style::{self, Mode, ReadingStyle, Rgba};

/// How the document decides between light and dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    /// Follow the desktop, which is what an app should do unless told not to.
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

    /// The mode this resolves to, given what the desktop is currently set to.
    pub fn resolve(self, system_is_dark: bool) -> Mode {
        match self {
            Self::Light => Mode::Light,
            Self::Dark => Mode::Dark,
            Self::System if system_is_dark => Mode::Dark,
            Self::System => Mode::Light,
        }
    }

    /// What `AdwStyleManager` should be told, so that the chrome around the
    /// document changes with the page rather than after it.
    pub fn color_scheme(self) -> adw::ColorScheme {
        match self {
            Self::System => adw::ColorScheme::Default,
            Self::Light => adw::ColorScheme::ForceLight,
            Self::Dark => adw::ColorScheme::ForceDark,
        }
    }
}

const THUMBNAIL_WIDTH: i32 = 128;
const THUMBNAIL_HEIGHT: i32 = 78;

// ---- the popover --------------------------------------------------------

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct StylePopover {
        pub cards: RefCell<Vec<(&'static str, gtk::Button)>>,
        pub thumbnails: RefCell<Vec<StyleThumbnail>>,
        pub appearance: RefCell<Option<adw::ToggleGroup>>,
        /// Set while the popover is writing widget state, so the handlers it
        /// trips do not report those writes back as choices.
        pub syncing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StylePopover {
        const NAME: &'static str = "VellumStylePopover";
        type Type = super::StylePopover;
        type ParentType = gtk::Popover;
    }

    impl ObjectImpl for StylePopover {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }
    }

    impl WidgetImpl for StylePopover {}
    impl PopoverImpl for StylePopover {}
}

glib::wrapper! {
    pub struct StylePopover(ObjectSubclass<imp::StylePopover>)
        @extends gtk::Popover, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native,
                    gtk::ShortcutManager;
}

impl Default for StylePopover {
    fn default() -> Self {
        Self::new()
    }
}

impl StylePopover {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Ring the card that is in use.
    pub fn set_selected(&self, id: &str) {
        for (style_id, card) in self.imp().cards.borrow().iter() {
            if *style_id == id {
                card.add_css_class("selected");
            } else {
                card.remove_css_class("selected");
            }
        }
    }

    pub fn set_appearance(&self, appearance: Appearance) {
        let imp = self.imp();
        let Some(group) = imp.appearance.borrow().clone() else {
            return;
        };
        imp.syncing.set(true);
        group.set_active_name(Some(appearance.id()));
        imp.syncing.set(false);
    }

    /// Redraw every specimen in `mode`, so the grid shows what choosing a card
    /// would actually produce right now.
    pub fn set_mode(&self, mode: Mode) {
        for thumbnail in self.imp().thumbnails.borrow().iter() {
            thumbnail.set_mode(mode);
        }
    }

    fn build(&self) {
        let imp = self.imp();
        self.add_css_class("style-popover");

        let column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        column.append(
            &gtk::Label::builder()
                .label("Reading Style")
                .xalign(0.0)
                .css_classes(["heading"])
                .build(),
        );

        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(2)
            .max_children_per_line(2)
            .row_spacing(8)
            .column_spacing(8)
            .homogeneous(true)
            .build();

        let mut cards = Vec::new();
        let mut thumbnails = Vec::new();
        for style in style::ALL {
            let thumbnail = StyleThumbnail::new(style);

            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(6)
                .build();
            content.append(&thumbnail);
            content.append(
                &gtk::Label::builder()
                    .label(style.label)
                    .css_classes(["caption-heading"])
                    .build(),
            );

            let card = gtk::Button::builder()
                .child(&content)
                .css_classes(["style-card", "flat"])
                .tooltip_text(style.label)
                .action_name("win.reading-style")
                .action_target(&style.id.to_variant())
                .build();
            card.update_property(&[gtk::accessible::Property::Label(style.label)]);

            grid.append(&card);
            cards.push((style.id, card));
            thumbnails.push(thumbnail);
        }
        imp.cards.replace(cards);
        imp.thumbnails.replace(thumbnails);

        // The grid scrolls: eight styles fit, and a ninth should not make the
        // popover taller than the window.
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(360)
            .child(&grid)
            .build();
        column.append(&scroller);

        column.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let appearance_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        appearance_row.append(
            &gtk::Label::builder()
                .label("Appearance")
                .xalign(0.0)
                .hexpand(true)
                .build(),
        );

        let group = adw::ToggleGroup::builder().build();
        for (appearance, label) in [
            (Appearance::System, "System"),
            (Appearance::Light, "Light"),
            (Appearance::Dark, "Dark"),
        ] {
            group.add(
                adw::Toggle::builder()
                    .name(appearance.id())
                    .label(label)
                    .build(),
            );
        }
        group.connect_active_name_notify(clone!(
            #[weak(rename_to = popover)]
            self,
            move |group| {
                if popover.imp().syncing.get() {
                    return;
                }
                let Some(name) = group.active_name() else {
                    return;
                };
                let _ = gtk::prelude::WidgetExt::activate_action(
                    &popover,
                    "win.appearance",
                    Some(&name.as_str().to_variant()),
                );
            }
        ));
        appearance_row.append(&group);
        imp.appearance.replace(Some(group));

        column.append(&appearance_row);
        self.set_child(Some(&column));
    }
}

// ---- one specimen -------------------------------------------------------

mod thumbnail_imp {
    use super::*;

    pub struct StyleThumbnail {
        pub style: Cell<&'static ReadingStyle>,
        pub mode: Cell<Mode>,
    }

    impl Default for StyleThumbnail {
        fn default() -> Self {
            Self {
                style: Cell::new(style::from_id(style::DEFAULT_ID)),
                mode: Cell::new(Mode::Light),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StyleThumbnail {
        const NAME: &'static str = "VellumStyleThumbnail";
        type Type = super::StyleThumbnail;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for StyleThumbnail {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj()
                .set_size_request(THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT);
        }
    }

    impl WidgetImpl for StyleThumbnail {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let style = self.style.get();
            let palette = style.palette(self.mode.get());
            let typography = &style.typography;

            let width = widget.width() as f32;
            let height = widget.height() as f32;
            let bounds = graphene::Rect::new(0.0, 0.0, width, height);

            let page = gsk::RoundedRect::from_rect(bounds, 6.0);
            snapshot.push_rounded_clip(&page);
            snapshot.append_color(&rgba(palette.page), &bounds);

            let inset = 10.0;
            let mut y = 9.0;

            // The heading, in the style's own face, at a size that keeps its
            // proportion to the body without overflowing a 128-pixel card.
            let heading_size = (7.0 * typography.h1_scale).min(18.0);
            let layout = widget.create_pango_layout(Some(if typography.h1_uppercase {
                "ASPECT"
            } else {
                "Aspect"
            }));
            let mut font =
                pango::FontDescription::from_string(&first_family(typography.heading_font));
            font.set_absolute_size(heading_size as f64 * pango::SCALE as f64);
            font.set_weight(match typography.heading_weight {
                weight if weight >= 700 => pango::Weight::Bold,
                weight if weight >= 600 => pango::Weight::Semibold,
                _ => pango::Weight::Normal,
            });
            layout.set_font_description(Some(&font));

            let heading_width = layout.pixel_size().0 as f32;
            let heading_x = if typography.title_align == crate::model::style::Align::Center {
                ((width - heading_width) / 2.0).max(inset)
            } else {
                inset
            };
            snapshot.save();
            snapshot.translate(&graphene::Point::new(heading_x, y));
            snapshot.append_layout(&layout, &rgba(palette.title));
            snapshot.restore();
            y += layout.pixel_size().1 as f32 + 4.0;

            // A rule under the heading for the styles that have one, so the
            // card shows that too.
            if typography.h2_rule {
                snapshot.append_color(
                    &rgba(palette.rule_strong),
                    &graphene::Rect::new(inset, y, width - inset * 2.0, 1.0),
                );
                y += 6.0;
            }

            // Body copy, as bars: the leading and the measure are what the card
            // is really showing, and four lines of unreadable text would only
            // be noise.
            let leading = (3.0 * typography.line_height).max(4.5);
            let bar = 3.0_f32.min(typography.body_size / 5.0);
            let measure =
                (width - inset * 2.0) * (typography.measure as f32 / 704.0).clamp(0.72, 1.0);
            let mut line = 0;
            while y + bar < height - 8.0 && line < 5 {
                // A ragged last line, so the block reads as prose rather than
                // as a table — unless the style justifies, in which case it
                // does not.
                let last = y + bar + leading >= height - 8.0;
                let length = if last && !typography.justify {
                    measure * 0.55
                } else {
                    measure
                };
                snapshot.append_color(
                    &rgba(palette.dim),
                    &graphene::Rect::new(inset, y, length, bar),
                );
                y += bar + leading;
                line += 1;
            }

            snapshot.pop();

            // A hairline round the card, so a white page on a white popover
            // still reads as a page.
            snapshot.append_border(&page, &[1.0; 4], &[rgba(palette.rule); 4]);
        }
    }
}

glib::wrapper! {
    pub struct StyleThumbnail(ObjectSubclass<thumbnail_imp::StyleThumbnail>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl StyleThumbnail {
    pub fn new(style: &'static ReadingStyle) -> Self {
        let thumbnail: Self = glib::Object::builder().build();
        thumbnail.imp().style.set(style);
        thumbnail
    }

    pub fn set_mode(&self, mode: Mode) {
        if self.imp().mode.get() == mode {
            return;
        }
        self.imp().mode.set(mode);
        self.queue_draw();
    }
}

/// The first family in a CSS-style list, which is what Pango wants.
fn first_family(families: &str) -> String {
    families
        .split(',')
        .next()
        .unwrap_or("sans-serif")
        .trim()
        .to_string()
}

fn rgba(colour: Rgba) -> gdk::RGBA {
    gdk::RGBA::new(colour.red(), colour.green(), colour.blue(), colour.alpha())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_round_trips_through_its_id() {
        for appearance in [Appearance::System, Appearance::Light, Appearance::Dark] {
            assert_eq!(Appearance::from_id(appearance.id()), appearance);
        }
        assert_eq!(Appearance::from_id("nonsense"), Appearance::System);
    }

    /// The whole point of the default: the document follows the desktop until
    /// somebody says otherwise, and then it stops following it.
    #[test]
    fn only_the_system_setting_follows_the_desktop() {
        assert_eq!(Appearance::System.resolve(true), Mode::Dark);
        assert_eq!(Appearance::System.resolve(false), Mode::Light);
        assert_eq!(Appearance::Light.resolve(true), Mode::Light);
        assert_eq!(Appearance::Dark.resolve(false), Mode::Dark);
    }

    #[test]
    fn a_family_list_reduces_to_its_first_face() {
        assert_eq!(first_family("PT Serif, Georgia, serif"), "PT Serif");
        assert_eq!(first_family("monospace"), "monospace");
    }
}
