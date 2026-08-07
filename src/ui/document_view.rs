//! The page: one `GtkTextView` showing the Markdown source, styled in place.
//!
//! There is one widget, never two swapped. The document is always its own
//! source and always looks rendered, because the syntax characters carry a tag
//! whose `invisible` property is on — so the caret lands where you clicked, the
//! scroll position never jumps between modes, and what is saved is what is on
//! screen. The three modes differ only in *which* markers are revealed, which is
//! [`ViewMode`] and nothing else.
//!
//! Two things a text tag cannot do, this widget draws itself in [`snapshot`]:
//! the page colour (so decorations can sit under the text rather than over it)
//! and every rule a reading style asks for — the bar beside a quote, the
//! hairline under a second-level heading, a thematic break, the frame round a
//! fenced block. The line ranges come from `model::decoration`, which derives
//! them from the same scan that produced the tags.
//!
//! [`snapshot`]: gtk::subclass::prelude::WidgetImpl::snapshot

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::{self, clone};
use gtk::{gdk, graphene, gsk, pango};
use std::cell::{Cell, RefCell};

use crate::model::decoration::{decorations, Decoration};
use crate::model::format;
use crate::model::style::{Align, Mode, Palette, ReadingStyle, Rgba};
use crate::model::Command;

/// How much of the document's Markdown is on show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// The syntax of the construct holding the caret, and nothing else. The
    /// mode this app exists for — but not the one it opens in.
    Live,
    /// Every marker, for when the Markdown itself is what you are working on.
    Source,
    /// None of it, and no caret: the document as it would be published.
    ///
    /// The default. A document is opened to be read, and the caret starts at
    /// the top of it — which in live mode is inside the first heading, so the
    /// first thing on screen would be its `# `. Rendered on arrival, and one
    /// click or `Ctrl+1` away from editing.
    #[default]
    Reading,
}

impl ViewMode {
    /// Stable across releases: it is the `AdwToggle` name and the GSettings
    /// value.
    pub fn id(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Source => "source",
            Self::Reading => "reading",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "live" => Self::Live,
            "source" => Self::Source,
            _ => Self::Reading,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Live => "Live",
            Self::Source => "Source",
            Self::Reading => "Reading",
        }
    }

    /// Reading is a reader. Typing into a document whose syntax is hidden
    /// everywhere would let you delete a marker you cannot see.
    pub fn editable(self) -> bool {
        !matches!(self, Self::Reading)
    }
}

/// The smallest gap between the text column and the window edge, so the page
/// never runs into the scrollbar on a narrow window.
const MIN_SIDE_MARGIN: i32 = 32;
/// Room above the first line and below the last. The generous bottom is what
/// lets the end of a document be typed at eye level rather than at the floor of
/// the window.
const TOP_MARGIN: i32 = 56;
const BOTTOM_MARGIN: i32 = 140;
/// How far a quote is indented, and where its bar sits inside that.
const QUOTE_INDENT: i32 = 26;
const QUOTE_BAR_INSET: f32 = 8.0;
/// Where a top-level list item starts, and how much each nested level adds.
const LIST_INDENT: i32 = 28;
const LIST_STEP: i32 = 24;

/// Hands out the CSS name each view is selected by. Never reused, so a name
/// cannot outlive its widget and be picked up by the next one.
static VIEWS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

mod imp {
    use super::*;

    pub struct DocumentView {
        pub style: Cell<&'static ReadingStyle>,
        pub mode: Cell<Mode>,
        pub view_mode: Cell<ViewMode>,
        /// The scan the tags were applied from, kept so that moving the caret
        /// can re-decide which markers are revealed without parsing again.
        pub parsed: RefCell<quill::Parsed>,
        /// Rule-like spans that are syntax but not markers — a `---` on its own
        /// line is styled, not marked, because the scanner leaves the decision
        /// to the view. Here the decision is to hide it and draw the rule.
        pub pseudo_markers: RefCell<Vec<quill::Marker>>,
        pub decorations: RefCell<Vec<Decoration>>,
        /// The gap between the window edge and the text column, recomputed on
        /// every allocation so the measure stays centred.
        pub side_margin: Cell<i32>,
        /// This view's contribution to the display's CSS, replaced whenever the
        /// reading style changes.
        pub provider: gtk::CssProvider,
    }

    impl Default for DocumentView {
        fn default() -> Self {
            Self {
                style: Cell::new(crate::model::style::from_id(
                    crate::model::style::DEFAULT_ID,
                )),
                mode: Cell::new(Mode::Light),
                view_mode: Cell::new(ViewMode::default()),
                parsed: RefCell::new(quill::Parsed::default()),
                pseudo_markers: RefCell::new(Vec::new()),
                decorations: RefCell::new(Vec::new()),
                side_margin: Cell::new(MIN_SIDE_MARGIN),
                provider: gtk::CssProvider::new(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DocumentView {
        const NAME: &'static str = "VellumDocumentView";
        type Type = super::DocumentView;
        type ParentType = gtk::TextView;
    }

    impl ObjectImpl for DocumentView {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }

        fn dispose(&self) {
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &self.provider);
            }
        }
    }

    impl WidgetImpl for DocumentView {
        /// The page, then the text, then the rules.
        ///
        /// Order is the whole trick. The page colour is painted here rather
        /// than set as a CSS background so that a code block's fill can go over
        /// it and still be *under* the text the parent draws; the rules go last
        /// because they belong on top of everything, and they live in the
        /// margins where there is nothing to cover.
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let view = self.obj();
            let palette = *self.style.get().palette(self.mode.get());
            let width = view.width() as f32;
            let height = view.height() as f32;

            snapshot.append_color(
                &rgba(palette.page),
                &graphene::Rect::new(0.0, 0.0, width, height),
            );
            view.draw_block_fills(snapshot, &palette);
            self.parent_snapshot(snapshot);
            view.draw_rules(snapshot, &palette);
        }

        /// Keep the text column at the style's measure, centred, however wide
        /// the window gets. A line of prose 1400 pixels long is unreadable
        /// whatever face it is set in.
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);

            let measure = self.style.get().typography.measure;
            let side = ((width - measure) / 2).max(MIN_SIDE_MARGIN);
            if self.side_margin.get() != side {
                self.side_margin.set(side);
                self.obj().apply_margins();
            }
        }
    }

    impl TextViewImpl for DocumentView {}
}

glib::wrapper! {
    pub struct DocumentView(ObjectSubclass<imp::DocumentView>)
        @extends gtk::TextView, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                    gtk::Scrollable;
}

impl Default for DocumentView {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentView {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn view_mode(&self) -> ViewMode {
        self.imp().view_mode.get()
    }

    pub fn reading_style(&self) -> &'static ReadingStyle {
        self.imp().style.get()
    }

    pub fn set_view_mode(&self, mode: ViewMode) {
        let imp = self.imp();
        if imp.view_mode.get() == mode {
            return;
        }
        imp.view_mode.set(mode);
        self.apply_view_mode();
    }

    /// Make the widget match whatever mode it is in.
    ///
    /// Separate from [`set_view_mode`], which only runs on a *change*: the view
    /// is constructed already in a mode, and a text view left at its own
    /// defaults would be editable with a blinking caret in it while claiming to
    /// be a reader.
    ///
    /// [`set_view_mode`]: Self::set_view_mode
    fn apply_view_mode(&self) {
        let mode = self.imp().view_mode.get();
        self.set_editable(mode.editable());
        self.set_cursor_visible(mode.editable());
        // …and paint the caret in nothing as well. `cursor-visible` reads back
        // as false in reading mode and a caret is drawn at the insert mark
        // regardless; it does honour `caret-color`, so that is what takes it
        // away. A reader with a blinking bar in it is not a reader.
        self.imp().provider.load_from_string(&self.stylesheet());
        self.refresh_markers();
    }

    /// Repaint the whole page in `style`.
    ///
    /// Instant, with no transition: a cross-fade between two typographic
    /// systems reads as the app thinking about it, and an instant repaint reads
    /// as the app having already done it.
    pub fn set_reading_style(&self, style: &'static ReadingStyle, mode: Mode) {
        let imp = self.imp();
        imp.style.set(style);
        imp.mode.set(mode);

        imp.provider.load_from_string(&self.stylesheet());
        self.configure_tags();

        let typography = &style.typography;
        let leading = (typography.body_size * (typography.line_height - 1.0)).round() as i32;
        self.set_pixels_inside_wrap(leading);
        self.set_pixels_above_lines(leading / 2);
        self.set_pixels_below_lines(leading - leading / 2);
        self.set_justification(if typography.justify {
            gtk::Justification::Fill
        } else {
            gtk::Justification::Left
        });

        self.queue_resize();
        self.queue_draw();
    }

    /// Apply a formatting command to the selection, as one undo step.
    pub fn apply_command(&self, command: Command) {
        if !self.is_editable() {
            return;
        }
        let buffer = self.buffer();
        let (low, high) = match buffer.selection_bounds() {
            Some((start, end)) => (start.offset() as usize, end.offset() as usize),
            None => {
                let caret = buffer.iter_at_mark(&buffer.get_insert()).offset() as usize;
                (caret, caret)
            }
        };
        // Hidden characters included: the offsets the model works in are
        // offsets into the document, and the document contains its syntax
        // whether or not this mode is showing it.
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        let Some(edit) = format::apply(&text, (low, high), command) else {
            return;
        };

        buffer.begin_user_action();
        let mut from = buffer.iter_at_offset(edit.start as i32);
        let mut to = buffer.iter_at_offset(edit.end as i32);
        buffer.delete(&mut from, &mut to);
        buffer.insert(&mut from, &edit.text);
        let start = buffer.iter_at_offset(edit.selection.0 as i32);
        let end = buffer.iter_at_offset(edit.selection.1 as i32);
        buffer.select_range(&start, &end);
        buffer.end_user_action();

        self.scroll_mark_onscreen(&buffer.get_insert());
        self.grab_focus();
    }

    /// Bring `offset` to the top of the viewport, for an outline click.
    pub fn scroll_to_offset(&self, offset: usize) {
        let buffer = self.buffer();
        let iter = buffer.iter_at_offset(offset as i32);
        self.scroll_to_iter(&mut iter.clone(), 0.0, true, 0.0, 0.0);
        if self.is_editable() {
            buffer.place_cursor(&iter);
            self.grab_focus();
        }
    }

    /// The character offset of the first line in view, for the outline to
    /// follow the scroll.
    pub fn top_offset(&self) -> usize {
        let top = self.visible_rect().y();
        self.iter_at_location(0, top)
            .map(|iter| iter.offset() as usize)
            .unwrap_or(0)
    }

    /// The caret's one-based line and column, for the status bar.
    pub fn caret_position(&self) -> (i32, i32) {
        let buffer = self.buffer();
        let caret = buffer.iter_at_mark(&buffer.get_insert());
        (caret.line() + 1, caret.line_offset() + 1)
    }

    /// The whole document, syntax included.
    pub fn text(&self) -> String {
        let buffer = self.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string()
    }

    /// The scan the view is currently showing, so the window can derive the
    /// outline and the word count without parsing a second time.
    pub fn parsed(&self) -> quill::Parsed {
        self.imp().parsed.borrow().clone()
    }

    // ---- construction ---------------------------------------------------

    fn build(&self) {
        self.set_wrap_mode(gtk::WrapMode::WordChar);
        self.set_top_margin(TOP_MARGIN);
        self.set_bottom_margin(BOTTOM_MARGIN);
        self.set_left_margin(MIN_SIDE_MARGIN);
        self.set_right_margin(MIN_SIDE_MARGIN);
        // Tab moves focus. A document is prose, and a reader who cannot leave
        // the text view with the keyboard is trapped in it.
        self.set_accepts_tab(false);
        self.add_css_class("vellum-document");
        self.update_property(&[gtk::accessible::Property::Label("Document")]);

        // A CSS provider goes on the *display*, not the widget — the per-widget
        // route was deprecated in GTK 4.10 — so a selector shared by every
        // instance would mean the last view built decided the face for all of
        // them. Two windows in different reading styles is an ordinary thing to
        // want, so each view gets a name of its own to be selected by.
        self.set_widget_name(&format!(
            "vellum-document-{}",
            VIEWS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));

        // A reading style that silently fails to load leaves the page in the
        // previous face, which looks like the style picker not working. Say so
        // instead: the stylesheet is generated, so an error here is a bug in
        // this file rather than something a user can fix.
        self.imp()
            .provider
            .connect_parsing_error(|_, section, error| {
                glib::g_warning!(
                    "vellum",
                    "reading-style stylesheet: {error} at {}",
                    section.to_str()
                );
            });
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &self.imp().provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        self.install_tags();

        let buffer = self.buffer();
        buffer.connect_changed(clone!(
            #[weak(rename_to = view)]
            self,
            move |_| view.restyle()
        ));
        // Which markers are on show depends on where the caret is, so the
        // decision is remade whenever it moves — including the moves that are
        // not edits, like clicking or arrowing.
        buffer.connect_mark_set(clone!(
            #[weak(rename_to = view)]
            self,
            move |buffer, _iter, mark| {
                if mark == &buffer.get_insert() {
                    view.refresh_markers();
                }
            }
        ));

        // Enter inside a list lays down the next bullet or number. A list you
        // have to retype the marker for is a list you stop using.
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(clone!(
            #[weak(rename_to = view)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, state| {
                let enter = matches!(key, gdk::Key::Return | gdk::Key::KP_Enter);
                let plain = !state
                    .intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK);
                if enter && plain {
                    view.continue_list()
                } else {
                    glib::Propagation::Proceed
                }
            }
        ));
        self.add_controller(keys);

        self.set_reading_style(
            crate::model::style::from_id(crate::model::style::DEFAULT_ID),
            Mode::Light,
        );
        self.restyle();
        self.apply_view_mode();
    }

    /// Create every tag once. Their values are set by [`configure_tags`], which
    /// runs again on each style change.
    ///
    /// [`configure_tags`]: Self::configure_tags
    fn install_tags(&self) {
        let table = self.buffer().tag_table();
        for name in tag_names() {
            table.add(&gtk::TextTag::new(Some(&name)));
        }
    }

    fn configure_tags(&self) {
        let imp = self.imp();
        let style = imp.style.get();
        let typography = &style.typography;
        let palette = style.palette(imp.mode.get());
        let table = self.buffer().tag_table();

        let tag = |name: &str| table.lookup(name).expect("tag installed at construction");

        for level in 1..=6u8 {
            let heading = tag(&format!("md-h{level}"));
            let scale = typography.heading_scale(level);
            heading.set_scale(scale as f64);
            heading.set_weight(typography.heading_weight);
            heading.set_family(Some(typography.heading_font));
            heading.set_foreground_rgba(Some(&rgba(palette.title)));
            // Pango measures letter spacing in absolute units, so the tracking
            // a style names in ems has to be resolved against the size this
            // heading actually comes out at.
            //
            // Clamped at zero: `GtkTextTag:letter-spacing` is declared with a
            // minimum of 0, and setting it negative aborts the process rather
            // than failing the call. That loses the slight tightening four
            // styles ask for at display sizes, and keeps the letterspacing the
            // other four are *built* on — Book's `0.16em` small caps and
            // Newsprint's `0.09em` section rules are the shape of those styles,
            // where `-0.02em` on a heading is a refinement nobody would name.
            heading.set_letter_spacing(
                (typography.heading_tracking(level)
                    * typography.body_size
                    * scale
                    * pango::SCALE as f32)
                    .round()
                    .max(0.0) as i32,
            );
            heading.set_text_transform(if typography.heading_uppercase(level) {
                pango::TextTransform::Uppercase
            } else {
                pango::TextTransform::None
            });
            heading.set_pixels_above_lines((typography.body_size * 1.3).round() as i32);
            heading.set_pixels_below_lines((typography.body_size * 0.35).round() as i32);
            heading.set_line_height(1.15);
            // Only the first level is a title, and only some styles centre it.
            // Everything else is left, including under a justified style: a
            // justified heading stretches two words across the whole measure.
            heading.set_justification(if level == 1 && typography.title_align == Align::Center {
                gtk::Justification::Center
            } else {
                gtk::Justification::Left
            });
        }

        let bold = tag("md-bold");
        bold.set_weight(700);
        let italic = tag("md-italic");
        italic.set_style(pango::Style::Italic);
        let strike = tag("md-strike");
        strike.set_strikethrough(true);
        strike.set_foreground_rgba(Some(&rgba(palette.dim)));

        // A text tag draws its background straight onto the page rather than
        // compositing it, so the translucent token has to be flattened first.
        let code = tag("md-code");
        code.set_family(Some("monospace"));
        code.set_scale(0.92);
        code.set_background_rgba(Some(&rgba(palette.code_background.over(palette.page))));
        code.set_foreground_rgba(Some(&rgba(palette.code_keyword)));

        let block = tag("md-codeblock");
        block.set_family(Some("monospace"));
        block.set_scale(0.92);
        block.set_paragraph_background_rgba(Some(&rgba(
            palette.block_background.over(palette.page),
        )));
        block.set_foreground_rgba(Some(&rgba(palette.code_string)));
        block.set_justification(gtk::Justification::Left);

        let quote = tag("md-quote");
        quote.set_style(if typography.quote_italic {
            pango::Style::Italic
        } else {
            pango::Style::Normal
        });
        quote.set_foreground_rgba(Some(&rgba(palette.quote)));

        for level in 0..=quill::MAX_LIST_DEPTH {
            let list = tag(&format!("md-list{level}"));
            // Hanging indent: the bullet sits in the margin and the wrapped
            // lines line up under the item's text, not under its bullet.
            list.set_indent(-16);
            list.set_justification(gtk::Justification::Left);
        }

        for name in ["md-link", "md-wikilink"] {
            let link = tag(name);
            link.set_underline(pango::Underline::Single);
            link.set_foreground_rgba(Some(&rgba(palette.accent)));
        }
        let hashtag = tag("md-tag");
        hashtag.set_foreground_rgba(Some(&rgba(palette.accent)));

        let task = tag("md-task");
        task.set_family(Some("monospace"));
        task.set_foreground_rgba(Some(&rgba(palette.accent)));

        // A table is its own pipes: a text view cannot draw a column rule, so
        // the columns only line up if the face is monospaced.
        for name in ["md-table", "md-table-delimiter"] {
            let table_tag = tag(name);
            table_tag.set_family(Some(if typography.table_monospace {
                "monospace"
            } else {
                typography.body_font
            }));
            table_tag.set_scale(0.94);
            table_tag.set_justification(gtk::Justification::Left);
        }
        tag("md-table-delimiter").set_foreground_rgba(Some(&rgba(palette.dim)));

        let frontmatter = tag("md-frontmatter");
        frontmatter.set_family(Some("monospace"));
        frontmatter.set_scale(0.85);
        frontmatter.set_foreground_rgba(Some(&rgba(palette.dim)));

        tag("md-hidden").set_invisible(true);
        let shown = tag("md-shown");
        shown.set_invisible(false);
        shown.set_foreground_rgba(Some(&rgba(palette.marker)));

        self.apply_margins();
    }

    /// Put the text column on the measure, and every indented block inside it.
    ///
    /// A tag's `left-margin` **replaces** the view's rather than adding to it,
    /// so a quote tagged with a bare 26-pixel indent does not sit 26 pixels
    /// inside the column — it sits 26 pixels from the window edge, with the
    /// prose above it stranded in the middle of the page. Every block tag
    /// therefore carries the column's own margin plus its indent, and all of
    /// them are recomputed whenever the window is resized.
    fn apply_margins(&self) {
        let side = self.imp().side_margin.get();
        self.set_left_margin(side);
        self.set_right_margin(side);

        let table = self.buffer().tag_table();
        let set = |name: &str, left: i32, right: i32| {
            if let Some(tag) = table.lookup(name) {
                tag.set_left_margin(left);
                tag.set_right_margin(right);
            }
        };

        set("md-quote", side + QUOTE_INDENT, side);
        set("md-codeblock", side + QUOTE_INDENT, side + QUOTE_INDENT);
        for level in 0..=quill::MAX_LIST_DEPTH {
            set(
                &format!("md-list{level}"),
                side + LIST_INDENT + i32::from(level) * LIST_STEP,
                side,
            );
        }
    }

    /// Re-scan the document and re-apply every tag.
    ///
    /// A whole-document scan on each keystroke, which is what the scanner is
    /// built to survive: one pass, no allocation per character. If a document
    /// ever arrives long enough for that to show, `quill::scan_line` re-scans a
    /// single line — but a measured problem should come before that machinery.
    fn restyle(&self) {
        let buffer = self.buffer();
        // Hidden characters included, or every offset the scanner reports would
        // be measured against a shorter string than the buffer holds and the
        // tags would land on the wrong characters.
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        let parsed = quill::parse(&text);

        buffer.remove_all_tags(&buffer.start_iter(), &buffer.end_iter());
        let table = buffer.tag_table();
        let apply = |name: &str, start: usize, end: usize| {
            let Some(tag) = table.lookup(name) else {
                return;
            };
            let from = buffer.iter_at_offset(start as i32);
            let to = buffer.iter_at_offset(end as i32);
            buffer.apply_tag(&tag, &from, &to);
        };

        let mut pseudo = Vec::new();
        for span in &parsed.spans {
            use quill::Style;

            // A text view takes a paragraph's attributes — margins, indent,
            // justification — from the tags covering the *start of the line*.
            // Block styles are scanned as content only, after the `> ` or the
            // `- `, so they are extended back to the line start or their indents
            // silently do nothing.
            let start = match span.style {
                Style::Heading(_)
                | Style::Quote
                | Style::ListItem(_)
                | Style::CodeBlock
                | Style::Task(_)
                | Style::TableRow => {
                    let mut iter = buffer.iter_at_offset(span.start as i32);
                    iter.set_line_offset(0);
                    iter.offset() as usize
                }
                _ => span.start,
            };

            let name = match span.style {
                Style::Heading(level) => format!("md-h{}", level.clamp(1, 6)),
                Style::Bold => "md-bold".into(),
                Style::Italic => "md-italic".into(),
                Style::Strikethrough => "md-strike".into(),
                Style::Code => "md-code".into(),
                Style::CodeBlock => "md-codeblock".into(),
                Style::Quote => "md-quote".into(),
                Style::ListItem(level) => format!("md-list{level}"),
                Style::Link => "md-link".into(),
                Style::WikiLink | Style::Embed => "md-wikilink".into(),
                Style::Tag => "md-tag".into(),
                Style::Task(_) => "md-task".into(),
                Style::TableRow => "md-table".into(),
                Style::TableDelimiter => "md-table-delimiter".into(),
                Style::Frontmatter => "md-frontmatter".into(),
                // A thematic break is scanned as a style rather than a marker,
                // because whether `---` should be read or drawn is the view's
                // decision. Here it is drawn, so the characters are hidden on
                // the same terms as any other syntax.
                Style::Rule => {
                    pseudo.push(quill::Marker {
                        start: span.start,
                        end: span.end,
                        reveal: (span.start, span.end),
                    });
                    continue;
                }
            };
            apply(&name, start, span.end);
        }

        self.imp().decorations.replace(decorations(&text, &parsed));
        self.imp().pseudo_markers.replace(pseudo);
        self.imp().parsed.replace(parsed);
        self.refresh_markers();
        self.queue_draw();
    }

    /// Decide which syntax characters are on show, and tag them accordingly.
    ///
    /// Runs on every caret move as well as every edit, which is what makes the
    /// live mode live.
    fn refresh_markers(&self) {
        let imp = self.imp();
        let buffer = self.buffer();
        let table = buffer.tag_table();
        let (Some(hidden), Some(shown)) = (table.lookup("md-hidden"), table.lookup("md-shown"))
        else {
            return;
        };

        buffer.remove_tag(&hidden, &buffer.start_iter(), &buffer.end_iter());
        buffer.remove_tag(&shown, &buffer.start_iter(), &buffer.end_iter());

        let caret = buffer.iter_at_mark(&buffer.get_insert()).offset() as usize;
        let mode = imp.view_mode.get();
        let parsed = imp.parsed.borrow();
        let pseudo = imp.pseudo_markers.borrow();

        for marker in parsed.markers.iter().chain(pseudo.iter()) {
            let reveal = match mode {
                ViewMode::Source => true,
                ViewMode::Reading => false,
                ViewMode::Live => marker.revealed_by(caret),
            };
            let tag = if reveal { &shown } else { &hidden };
            let from = buffer.iter_at_offset(marker.start as i32);
            let to = buffer.iter_at_offset(marker.end as i32);
            buffer.apply_tag(tag, &from, &to);
        }
    }

    fn continue_list(&self) -> glib::Propagation {
        if !self.is_editable() {
            return glib::Propagation::Proceed;
        }
        let buffer = self.buffer();
        let caret = buffer.iter_at_mark(&buffer.get_insert());

        let mut start = caret;
        start.set_line_offset(0);
        let mut end = start;
        if !end.ends_line() {
            end.forward_to_line_end();
        }
        let line = buffer.text(&start, &end, true);

        let Some(action) = quill::list_enter(&line) else {
            return glib::Propagation::Proceed;
        };

        // One undo step, so Ctrl+Z takes back the whole line rather than the
        // newline and the bullet separately.
        buffer.begin_user_action();
        buffer.delete_selection(true, true);
        match action {
            quill::ListEnter::Continue(prefix) => {
                buffer.insert_at_cursor(&format!("\n{prefix}"));
                renumber(&buffer);
            }
            // The list is over. Leave the caret on the now-blank line; the next
            // Enter is an ordinary one.
            quill::ListEnter::EndList => {
                let mut from = buffer.iter_at_mark(&buffer.get_insert());
                from.set_line_offset(0);
                let mut to = from;
                if !to.ends_line() {
                    to.forward_to_line_end();
                }
                buffer.delete(&mut from, &mut to);
            }
        }
        buffer.end_user_action();

        self.scroll_mark_onscreen(&buffer.get_insert());
        glib::Propagation::Stop
    }

    // ---- painting -------------------------------------------------------

    /// The fill behind a fenced block, under the text.
    fn draw_block_fills(&self, snapshot: &gtk::Snapshot, palette: &Palette) {
        let radius = self.imp().style.get().typography.code_frame.radius;
        let fill = rgba(palette.block_background.over(palette.page));

        for decoration in self.imp().decorations.borrow().iter() {
            if let Decoration::CodeFrame {
                first_line,
                last_line,
            } = decoration
            {
                let Some(bounds) = self.block_bounds(*first_line, *last_line) else {
                    continue;
                };
                let rounded = gsk::RoundedRect::from_rect(bounds, radius);
                snapshot.push_rounded_clip(&rounded);
                snapshot.append_color(&fill, &bounds);
                snapshot.pop();
            }
        }
    }

    /// Everything a reading style asks to be drawn as a line, over the text.
    fn draw_rules(&self, snapshot: &gtk::Snapshot, palette: &Palette) {
        let typography = &self.imp().style.get().typography;
        let column = self.column_bounds();

        for decoration in self.imp().decorations.borrow().iter() {
            match decoration {
                Decoration::QuoteBar {
                    first_line,
                    last_line,
                } => {
                    if typography.quote_border <= 0.0 {
                        continue;
                    }
                    let Some(bounds) = self.line_span(*first_line, *last_line) else {
                        continue;
                    };
                    snapshot.append_color(
                        &rgba(palette.quote_border),
                        &graphene::Rect::new(
                            column.0 + QUOTE_BAR_INSET,
                            bounds.0,
                            typography.quote_border,
                            bounds.1 - bounds.0,
                        ),
                    );
                }
                Decoration::HeadingRule { line } => {
                    if !typography.h2_rule {
                        continue;
                    }
                    let Some(bounds) = self.line_span(*line, *line) else {
                        continue;
                    };
                    // Under the heading, in the space its own bottom padding
                    // leaves — a rule touching the letters would read as an
                    // underline.
                    let y = bounds.1 - (typography.body_size * 0.22).round();
                    snapshot.append_color(
                        &rgba(palette.rule_strong),
                        &graphene::Rect::new(column.0, y, column.1 - column.0, 1.0),
                    );
                }
                Decoration::ThematicBreak { line } => {
                    let Some(bounds) = self.line_span(*line, *line) else {
                        continue;
                    };
                    snapshot.append_color(
                        &rgba(palette.rule_strong),
                        &graphene::Rect::new(
                            column.0,
                            ((bounds.0 + bounds.1) / 2.0).round(),
                            column.1 - column.0,
                            1.0,
                        ),
                    );
                }
                Decoration::CodeFrame {
                    first_line,
                    last_line,
                } => {
                    let frame = typography.code_frame;
                    if frame.width <= 0.0 {
                        continue;
                    }
                    let Some(bounds) = self.block_bounds(*first_line, *last_line) else {
                        continue;
                    };
                    let colour = rgba(palette.block_border);
                    if frame.dashed {
                        draw_dashed_frame(snapshot, &bounds, frame.width, &colour);
                    } else {
                        snapshot.append_border(
                            &gsk::RoundedRect::from_rect(bounds, frame.radius),
                            &[frame.width; 4],
                            &[colour; 4],
                        );
                    }
                }
            }
        }
    }

    /// Left and right edges of the text column, in widget coordinates.
    fn column_bounds(&self) -> (f32, f32) {
        (
            self.left_margin() as f32,
            (self.width() - self.right_margin()) as f32,
        )
    }

    /// Top and bottom of a range of lines, in widget coordinates, or `None`
    /// when the range is entirely off screen.
    fn line_span(&self, first: usize, last: usize) -> Option<(f32, f32)> {
        let buffer = self.buffer();
        let lines = buffer.line_count();
        if first as i32 >= lines {
            return None;
        }
        let top = buffer.iter_at_line(first as i32)?;
        let bottom = buffer.iter_at_line((last as i32).min(lines - 1))?;

        let (top_y, _) = self.line_yrange(&top);
        let (bottom_y, bottom_height) = self.line_yrange(&bottom);

        let (_, start) = self.buffer_to_window_coords(gtk::TextWindowType::Widget, 0, top_y);
        let (_, end) =
            self.buffer_to_window_coords(gtk::TextWindowType::Widget, 0, bottom_y + bottom_height);

        // Entirely above or below the viewport: nothing to draw, and the
        // coordinates would be far outside the widget.
        if end < 0 || start > self.height() {
            return None;
        }
        Some((start as f32, end as f32))
    }

    /// The rectangle a fenced block occupies: its lines, inset a little from
    /// the measure so the frame is not flush with the prose above it.
    fn block_bounds(&self, first: usize, last: usize) -> Option<graphene::Rect> {
        let (left, right) = self.column_bounds();
        let (top, bottom) = self.line_span(first, last)?;
        let inset = (self.imp().style.get().typography.body_size * 0.25).round();
        Some(graphene::Rect::new(
            left,
            top - inset,
            right - left,
            (bottom - top) + inset * 2.0,
        ))
    }

    /// The CSS this view's reading style contributes.
    ///
    /// Font and ink only. The page colour is painted in `snapshot` instead, so
    /// that a code block's fill can be drawn beneath the text rather than over
    /// it, and both nodes are told to stay transparent so nothing paints over
    /// what was drawn there.
    fn stylesheet(&self) -> String {
        let imp = self.imp();
        let style = imp.style.get();
        let typography = &style.typography;
        let palette = style.palette(imp.mode.get());

        // The face is set on the `text` node as well as the widget node, not
        // just on the widget and left to inherit: the theme names a font on the
        // text node, and an inherited value loses to a named one however
        // specific the outer selector is. Setting only the outer node leaves
        // every reading style rendering in the platform's monospace default —
        // colours change, the type does not, and the styles all look the same.
        let name = self.widget_name();
        format!(
            "textview#{name}, textview#{name} > text {{\n  \
                 font-family: {family};\n  \
                 font-size: {size}px;\n  \
                 color: {text};\n  \
                 background-color: transparent;\n  \
                 background-image: none;\n  \
                 caret-color: {accent};\n\
             }}\n\
             textview#{name} > text > selection {{\n  \
                 background-color: {selection};\n  color: {text};\n\
             }}\n",
            family = css_families(typography.body_font),
            size = typography.body_size,
            text = css_colour(palette.text),
            accent = if imp.view_mode.get().editable() {
                css_colour(palette.accent)
            } else {
                "transparent".to_string()
            },
            // The style's own accent, kept light enough that the text on top of
            // it stays the text rather than being inverted.
            selection = css_colour_at(palette.accent, 0.28),
        )
    }
}

/// Put ordered-list numbers back in sequence after an insertion.
fn renumber(buffer: &gtk::TextBuffer) {
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    // Back to front, so an earlier edit cannot shift a later offset.
    for edit in quill::renumber(&text).iter().rev() {
        let mut start = buffer.iter_at_offset(edit.start as i32);
        let mut end = buffer.iter_at_offset(edit.end as i32);
        buffer.delete(&mut start, &mut end);
        buffer.insert(&mut start, &edit.number.to_string());
    }
}

/// A frame in dashes, for the one style that asks for one. Gsk draws solid
/// borders and nothing else, so the dashes are rectangles.
fn draw_dashed_frame(
    snapshot: &gtk::Snapshot,
    bounds: &graphene::Rect,
    width: f32,
    colour: &gdk::RGBA,
) {
    const DASH: f32 = 5.0;
    const GAP: f32 = 4.0;

    let (x, y) = (bounds.x(), bounds.y());
    let (w, h) = (bounds.width(), bounds.height());

    let mut at = 0.0;
    while at < w {
        let length = DASH.min(w - at);
        snapshot.append_color(colour, &graphene::Rect::new(x + at, y, length, width));
        snapshot.append_color(
            colour,
            &graphene::Rect::new(x + at, y + h - width, length, width),
        );
        at += DASH + GAP;
    }

    let mut at = 0.0;
    while at < h {
        let length = DASH.min(h - at);
        snapshot.append_color(colour, &graphene::Rect::new(x, y + at, width, length));
        snapshot.append_color(
            colour,
            &graphene::Rect::new(x + w - width, y + at, width, length),
        );
        at += DASH + GAP;
    }
}

fn tag_names() -> Vec<String> {
    let mut names: Vec<String> = vec![
        "md-bold",
        "md-italic",
        "md-strike",
        "md-code",
        "md-codeblock",
        "md-quote",
        "md-link",
        "md-wikilink",
        "md-tag",
        "md-task",
        "md-table",
        "md-table-delimiter",
        "md-frontmatter",
        "md-hidden",
        "md-shown",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    names.extend((1..=6).map(|level| format!("md-h{level}")));
    names.extend((0..=quill::MAX_LIST_DEPTH).map(|level| format!("md-list{level}")));
    names
}

fn rgba(colour: Rgba) -> gdk::RGBA {
    gdk::RGBA::new(colour.red(), colour.green(), colour.blue(), colour.alpha())
}

fn css_colour(colour: Rgba) -> String {
    css_colour_at(colour, colour.alpha())
}

fn css_colour_at(colour: Rgba, alpha: f32) -> String {
    format!(
        "rgba({},{},{},{alpha:.3})",
        (colour.red() * 255.0).round(),
        (colour.green() * 255.0).round(),
        (colour.blue() * 255.0).round(),
    )
}

/// Quote the family names that need it, and leave the generic keywords bare.
fn css_families(families: &str) -> String {
    families
        .split(',')
        .map(|family| {
            let family = family.trim();
            if matches!(family, "serif" | "sans-serif" | "monospace" | "cursive") {
                family.to_string()
            } else {
                format!("\"{family}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_round_trip_through_their_ids() {
        for mode in [ViewMode::Live, ViewMode::Source, ViewMode::Reading] {
            assert_eq!(ViewMode::from_id(mode.id()), mode);
        }
        // An unknown value from settings opens the document rendered, which is
        // the safe answer: it is the one mode that cannot be typed into by
        // accident.
        assert_eq!(ViewMode::from_id("nonsense"), ViewMode::Reading);
        assert_eq!(ViewMode::default(), ViewMode::Reading);
    }

    /// Reading is a reader. Typing into a document whose syntax is hidden
    /// everywhere would let a keystroke delete a marker nobody can see.
    #[test]
    fn only_reading_mode_is_read_only() {
        assert!(ViewMode::Live.editable());
        assert!(ViewMode::Source.editable());
        assert!(!ViewMode::Reading.editable());
    }

    #[test]
    fn family_lists_quote_real_faces_and_not_keywords() {
        assert_eq!(
            css_families("PT Serif, Georgia, serif"),
            "\"PT Serif\", \"Georgia\", serif"
        );
        assert_eq!(css_families("monospace"), "monospace");
    }

    #[test]
    fn colours_are_written_as_css_understands_them() {
        assert_eq!(css_colour(Rgba::hex(0xff8000)), "rgba(255,128,0,1.000)");
        assert_eq!(css_colour(Rgba::tint(0x000000, 128)), "rgba(0,0,0,0.502)");
    }
}
