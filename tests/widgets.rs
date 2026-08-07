//! Widget tests for the window and the page.
//!
//! These need a display:
//!
//! ```sh
//! ./test.sh              # against your own session
//! ./test.sh --headless   # under Xvfb
//! ```
//!
//! # Why this file has one `#[test]`
//!
//! GTK is thread-affine: it may be initialised from exactly one thread, and
//! every widget call must come from that thread afterwards. Rust's test harness
//! spawns a fresh thread per `#[test]`, and `--test-threads=1` only serialises
//! them — it does not make them share a thread. So every case here is a plain
//! function and a single `#[test]` runs them in sequence. The runner names each
//! case and keeps going after a failure, so one broken case does not hide the
//! ones behind it.
//!
//! Windows are never presented: these exercise construction, tagging and the
//! wiring between the actions and the buffer, all of which happen before a
//! surface exists.

use adw::prelude::*;
use gtk::glib::{self, prelude::ToVariant};

use vellum::model::style;
use vellum::ui::{DocumentView, ViewMode, Window};

/// Initialise the toolkit and hand back an application to parent the windows
/// under test. Idempotent, so every case calls it.
fn init() -> adw::Application {
    thread_local! {
        // One application for the whole run: registering a second under the
        // same ID fails, because the first still owns its D-Bus object.
        static APP: adw::Application = {
            gtk::init().expect("GTK could not initialise — no display? Try ./test.sh --headless.");
            adw::init().expect("libadwaita could not initialise");

            let app = adw::Application::builder()
                .application_id("us.hagreli.Vellum.Test")
                .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
                .build();
            app.register(gtk::gio::Cancellable::NONE)
                .expect("could not register on the session bus");
            app
        };
    }
    APP.with(Clone::clone)
}

/// Let the main loop settle so widget state reflects the calls just made.
fn drain_events() {
    let ctx = glib::MainContext::default();
    while ctx.pending() {
        ctx.iteration(false);
    }
}

/// A window showing `text`, from a real file on disk.
fn window_showing(text: &str) -> (Window, tempfile::TempDir) {
    let directory = tempfile::tempdir().expect("a temp dir");
    let path = directory.path().join("notes.md");
    std::fs::write(&path, text).expect("write the sample");

    let window = Window::new(&init());
    window.open_path(&path);
    drain_events();
    (window, directory)
}

fn view_of(window: &Window) -> DocumentView {
    window.view().expect("the window builds its page")
}

/// A window showing `text`, switched into live mode.
///
/// A document opens rendered and read-only, so anything testing an *edit* has
/// to say so — the same click a user makes before typing.
fn editing_window(text: &str) -> (Window, tempfile::TempDir) {
    let (window, directory) = window_showing(text);
    activate(&window, "win.mode", Some("live"));
    (window, directory)
}

fn body(view: &DocumentView) -> String {
    view.text()
}

/// Run an action on the window the way a menu item does.
fn activate(window: &Window, action: &str, target: Option<&str>) {
    let variant = target.map(|value| value.to_variant());
    gtk::prelude::WidgetExt::activate_action(window, action, variant.as_ref())
        .unwrap_or_else(|err| panic!("{action} is not wired up: {err}"));
    drain_events();
}

fn select(view: &DocumentView, word: &str) {
    let text = view.text();
    let byte = text.find(word).expect("the word is in the document");
    let start = text[..byte].chars().count() as i32;
    let buffer = view.buffer();
    buffer.select_range(
        &buffer.iter_at_offset(start),
        &buffer.iter_at_offset(start + word.chars().count() as i32),
    );
    drain_events();
}

// ---- cases --------------------------------------------------------------

fn opening_a_file_shows_it_and_names_the_window() {
    let (window, _dir) = window_showing("# Notes\n\nSome prose.\n");

    assert_eq!(body(&view_of(&window)), "# Notes\n\nSome prose.\n");
    assert_eq!(
        window.title().map(|title| title.to_string()).as_deref(),
        Some("notes.md")
    );
}

/// A document opens *rendered*. The caret starts at the top, which in live mode
/// is inside the first heading — so opening in live mode makes the very first
/// thing on screen that heading's `# `, which is the opposite of the point.
fn a_freshly_opened_document_shows_no_syntax() {
    let (window, _dir) = window_showing("# Notes\n\nA **bold** claim.\n");
    let view = view_of(&window);
    let buffer = view.buffer();
    let hidden = buffer.tag_table().lookup("md-hidden").expect("the tag");

    assert_eq!(view.view_mode(), ViewMode::Reading);
    assert!(
        buffer.iter_at_offset(0).has_tag(&hidden),
        "the first heading's hashes are showing on a freshly opened document"
    );

    let bold = view.text().find("bold").expect("present") as i32;
    assert!(
        buffer.iter_at_offset(bold - 2).has_tag(&hidden),
        "a bold run's asterisks are showing on a freshly opened document"
    );

    // …and the page is a reader until it is asked to be anything else.
    assert!(!view.is_editable());
    assert!(!view.is_cursor_visible());
}

/// The syntax stays in the buffer whatever the mode is showing. Asking the
/// buffer to leave hidden characters out would make what gets saved depend on
/// where the caret happened to be.
fn hidden_syntax_is_still_in_the_document() {
    let (window, _dir) = window_showing("A **bold** claim.\n");
    let view = view_of(&window);

    activate(&window, "win.mode", Some("reading"));
    assert_eq!(body(&view), "A **bold** claim.\n");

    activate(&window, "win.mode", Some("source"));
    assert_eq!(body(&view), "A **bold** claim.\n");
}

fn every_construct_the_scanner_reports_is_tagged() {
    let (window, _dir) = window_showing(
        "# Heading\n\n> quoted\n\n- item\n\n`code`\n\n```\nfenced\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [ ] task\n\n[link](https://example.com)\n",
    );
    let view = view_of(&window);
    let buffer = view.buffer();
    let table = buffer.tag_table();

    for name in [
        "md-h1",
        "md-quote",
        "md-list0",
        "md-code",
        "md-codeblock",
        "md-table",
        "md-table-delimiter",
        "md-task",
        "md-link",
    ] {
        let tag = table
            .lookup(name)
            .unwrap_or_else(|| panic!("{name} exists"));
        let mut iter = buffer.start_iter();
        let mut found = iter.starts_tag(Some(&tag));
        while !found && iter.forward_to_tag_toggle(Some(&tag)) {
            found = true;
        }
        assert!(found, "nothing in the document was tagged {name}");
    }
}

/// Live mode reveals the construct the caret is in and nothing else; that is
/// the whole behaviour the app exists for.
fn live_mode_reveals_only_the_construct_under_the_caret() {
    let (window, _dir) = window_showing("A **bold** claim.\n\nAnother *italic* line.\n");
    let view = view_of(&window);
    let buffer = view.buffer();
    let hidden = buffer.tag_table().lookup("md-hidden").expect("the tag");

    activate(&window, "win.mode", Some("live"));

    // Caret inside the bold run.
    let text = view.text();
    let bold = text.find("bold").expect("present") as i32;
    buffer.place_cursor(&buffer.iter_at_offset(bold));
    drain_events();

    let asterisks = buffer.iter_at_offset(bold - 2);
    assert!(
        !asterisks.has_tag(&hidden),
        "the caret's own markers should be revealed"
    );

    let italic = text.find("italic").expect("present") as i32;
    let elsewhere = buffer.iter_at_offset(italic - 1);
    assert!(
        elsewhere.has_tag(&hidden),
        "markers away from the caret should stay hidden"
    );
}

fn reading_mode_hides_every_marker_and_refuses_edits() {
    let (window, _dir) = window_showing("A **bold** claim.\n");
    let view = view_of(&window);
    let buffer = view.buffer();
    let hidden = buffer.tag_table().lookup("md-hidden").expect("the tag");

    activate(&window, "win.mode", Some("reading"));

    let text = view.text();
    let bold = text.find("bold").expect("present") as i32;
    buffer.place_cursor(&buffer.iter_at_offset(bold));
    drain_events();

    assert!(
        buffer.iter_at_offset(bold - 2).has_tag(&hidden),
        "reading mode reveals nothing, wherever the caret is"
    );
    assert!(!view.is_editable());
    assert_eq!(view.view_mode(), ViewMode::Reading);
}

fn source_mode_reveals_every_marker() {
    let (window, _dir) = window_showing("A **bold** claim.\n");
    let view = view_of(&window);
    let buffer = view.buffer();
    let shown = buffer.tag_table().lookup("md-shown").expect("the tag");

    activate(&window, "win.mode", Some("source"));

    let bold = view.text().find("bold").expect("present") as i32;
    assert!(buffer.iter_at_offset(bold - 2).has_tag(&shown));
    assert!(view.is_editable(), "source mode is still editable");
}

fn a_formatting_command_rewrites_the_selection() {
    let (window, _dir) = editing_window("A bold claim.\n");
    let view = view_of(&window);

    select(&view, "bold");
    activate(&window, "win.format", Some("bold"));

    assert_eq!(body(&view), "A **bold** claim.\n");

    // …and again takes it off, rather than nesting a second pair.
    activate(&window, "win.format", Some("bold"));
    assert_eq!(body(&view), "A bold claim.\n");
}

/// One menu click is one undo step. Two would mean Ctrl+Z leaves half the
/// formatting behind.
fn a_formatting_command_undoes_in_one_step() {
    let (window, _dir) = editing_window("A bold claim.\n");
    let view = view_of(&window);

    select(&view, "bold");
    activate(&window, "win.format", Some("bold"));
    assert_eq!(body(&view), "A **bold** claim.\n");

    view.buffer().undo();
    drain_events();
    assert_eq!(body(&view), "A bold claim.\n");
}

fn a_heading_command_replaces_the_level_already_there() {
    let (window, _dir) = editing_window("# Title\n");
    let view = view_of(&window);
    let buffer = view.buffer();

    buffer.place_cursor(&buffer.iter_at_offset(3));
    drain_events();
    activate(&window, "win.format", Some("heading-2"));
    assert_eq!(body(&view), "## Title\n");

    activate(&window, "win.format", Some("paragraph"));
    assert_eq!(body(&view), "Title\n");
}

/// Reading mode has nothing to write into, so the commands go insensitive
/// rather than silently doing nothing when chosen.
fn formatting_is_disabled_in_reading_mode() {
    let (window, _dir) = window_showing("A bold claim.\n");
    let view = view_of(&window);

    activate(&window, "win.mode", Some("reading"));
    select(&view, "bold");
    activate(&window, "win.format", Some("bold"));
    assert_eq!(
        body(&view),
        "A bold claim.\n",
        "a formatting command reached the buffer in reading mode"
    );

    activate(&window, "win.mode", Some("live"));
    select(&view, "bold");
    activate(&window, "win.format", Some("bold"));
    assert_eq!(body(&view), "A **bold** claim.\n");
}

fn choosing_a_reading_style_repaints_the_page() {
    let (window, _dir) = window_showing("# Notes\n");
    let view = view_of(&window);

    for chosen in style::ALL {
        activate(&window, "win.reading-style", Some(chosen.id));
        assert_eq!(
            view.reading_style().id,
            chosen.id,
            "the page did not take the {} style",
            chosen.id
        );
    }
}

/// A style id that is not one of ours — a downgrade, or a hand-edited setting —
/// must leave the app usable rather than blank.
fn an_unknown_reading_style_falls_back_rather_than_failing() {
    let (window, _dir) = window_showing("# Notes\n");
    let view = view_of(&window);

    activate(&window, "win.reading-style", Some("neon-vaporwave"));
    assert_eq!(view.reading_style().id, style::DEFAULT_ID);
}

fn the_outline_lists_the_headings_and_follows_the_document() {
    let (window, _dir) = window_showing("# One\n\nProse.\n\n## Two\n\nMore.\n\n## Three\n");
    let view = view_of(&window);

    let parsed = view.parsed();
    let headings = vellum::model::outline::outline(&view.text(), &parsed);
    let shape: Vec<(u8, &str)> = headings
        .iter()
        .map(|heading| (heading.level, heading.text.as_str()))
        .collect();
    assert_eq!(shape, [(1, "One"), (2, "Two"), (2, "Three")]);

    // Editing a heading changes the outline without anything having to ask.
    let buffer = view.buffer();
    buffer.insert(&mut buffer.end_iter(), "\n### Four\n");
    drain_events();

    let after = vellum::model::outline::outline(&view.text(), &view.parsed());
    assert_eq!(after.len(), 4);
    assert_eq!(after[3].text, "Four");
}

fn enter_carries_a_list_on_to_the_next_line() {
    let (window, _dir) = editing_window("- first\n");
    let view = view_of(&window);
    let buffer = view.buffer();

    buffer.place_cursor(&buffer.iter_at_offset(7));
    drain_events();
    send_enter(&view);

    assert_eq!(body(&view), "- first\n- \n");
}

fn typing_marks_the_document_modified_and_saving_clears_it() {
    let directory = tempfile::tempdir().expect("a temp dir");
    let path = directory.path().join("notes.md");
    std::fs::write(&path, "original\n").expect("write");

    let window = Window::new(&init());
    window.open_path(&path);
    drain_events();

    let view = view_of(&window);
    let buffer = view.buffer();
    buffer.insert(&mut buffer.end_iter(), "edited\n");
    drain_events();

    assert!(
        window
            .title()
            .map(|title| title.to_string())
            .is_some_and(|title| title.contains("notes.md")),
        "the window keeps naming the file while it is dirty"
    );

    activate(&window, "win.save", None);
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        "original\nedited\n"
    );
}

/// The outline pane starts visible. A `sync_create` bind from a default-off
/// toggle is the standard way to get this wrong, and it hides the sidebar at
/// launch on every start.
fn the_outline_starts_shown_and_the_toggle_hides_it() {
    let (window, _dir) = window_showing("# One\n");
    let split = find_split_view(window.upcast_ref::<gtk::Widget>()).expect("a split view");

    assert!(split.shows_sidebar(), "the outline should start shown");
    activate(&window, "win.toggle-outline", None);
    assert!(!split.shows_sidebar());
    activate(&window, "win.toggle-outline", None);
    assert!(split.shows_sidebar());
}

fn the_stylesheet_parses() {
    init();
    let provider = gtk::CssProvider::new();
    let failed = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    provider.connect_parsing_error({
        let failed = failed.clone();
        move |_, _, error| failed.borrow_mut().push(error.to_string())
    });
    provider.load_from_string(include_str!("../src/ui/style.css"));
    assert!(
        failed.borrow().is_empty(),
        "style.css does not parse: {:#?}",
        failed.borrow()
    );
}

// ---- helpers ------------------------------------------------------------

/// Press Enter on the page.
///
/// These windows are never presented, so there is no surface to deliver a real
/// key event to. Emitting `key-pressed` on the view's own key controller runs
/// the same handler the compositor would reach, which is the part under test.
fn send_enter(view: &DocumentView) {
    use gtk::glib::translate::IntoGlib;

    let mut controller = view.observe_controllers().into_iter().find_map(|item| {
        item.ok()
            .and_then(|item| item.downcast::<gtk::EventControllerKey>().ok())
    });
    let controller = controller
        .take()
        .expect("the page installs a key controller");
    controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Return.into_glib(),
            &36u32,
            &gtk::gdk::ModifierType::empty(),
        ],
    );
    drain_events();
}

fn find_split_view(widget: &gtk::Widget) -> Option<adw::OverlaySplitView> {
    if let Ok(split) = widget.clone().downcast::<adw::OverlaySplitView>() {
        return Some(split);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_split_view(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
fn window_suite() {
    let mut failures: Vec<String> = Vec::new();

    macro_rules! case {
        ($case:ident) => {
            // Collected rather than propagated: each case builds its own
            // widgets, so an unwind part-way through one does not leak state
            // into the next, and reporting all of them at once beats
            // rediscovering them one run at a time.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe($case)).is_err() {
                failures.push(stringify!($case).to_string());
            }
        };
    }

    case!(opening_a_file_shows_it_and_names_the_window);
    case!(a_freshly_opened_document_shows_no_syntax);
    case!(hidden_syntax_is_still_in_the_document);
    case!(every_construct_the_scanner_reports_is_tagged);
    case!(live_mode_reveals_only_the_construct_under_the_caret);
    case!(reading_mode_hides_every_marker_and_refuses_edits);
    case!(source_mode_reveals_every_marker);
    case!(a_formatting_command_rewrites_the_selection);
    case!(a_formatting_command_undoes_in_one_step);
    case!(a_heading_command_replaces_the_level_already_there);
    case!(formatting_is_disabled_in_reading_mode);
    case!(choosing_a_reading_style_repaints_the_page);
    case!(an_unknown_reading_style_falls_back_rather_than_failing);
    case!(the_outline_lists_the_headings_and_follows_the_document);
    case!(enter_carries_a_list_on_to_the_next_line);
    case!(typing_marks_the_document_modified_and_saving_clears_it);
    case!(the_outline_starts_shown_and_the_toggle_hides_it);
    case!(the_stylesheet_parses);

    assert!(
        failures.is_empty(),
        "{} widget cases failed: {:#?}\n(panic messages are printed above, in order)",
        failures.len(),
        failures
    );
}
