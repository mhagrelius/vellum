//! Render the real window in every reading style, to PNG, for design review.
//!
//! Screenshotting a live GNOME Wayland session needs interactive consent, which
//! makes "does this actually look right?" awkward to answer while working. This
//! drives the real window through the real actions — same CSS, same tags, same
//! drawing code — and writes the frames to files instead.
//!
//! ```sh
//! cargo run --example preview -- /tmp/vellum-preview light
//! cargo run --example preview -- /tmp/vellum-preview dark
//! cargo run --example preview -- /tmp/vellum-preview light bottom
//! ```
//!
//! Writes `<dir>/<scheme>-<style>.png`. One scheme per run: the colour scheme
//! is a process-wide setting and every open window follows it, so rendering
//! both in one pass would capture whichever was set last.
//!
//! A third argument of `bottom` scrolls to the end of the sample first, which
//! is where the table, the fenced block, the thematic break and the task list
//! are — the constructs the view *draws* rather than tags, and the ones a
//! screenshot of the first page never shows.
//!
//! The mode is left at whatever a document opens in, which is reading — so
//! these frames are what launching the app actually looks like. Switch to live
//! by hand if you want to review how the syntax around the caret reads.

use adw::prelude::*;
use gtk::glib::prelude::ToVariant;
use gtk::{gdk, glib, gsk};
use std::path::PathBuf;

use vellum::model::style;
use vellum::ui::style_popover::StylePopover;
use vellum::ui::Window;

/// A kitchen sink: every construct the scanner styles and every decoration the
/// view draws, so one frame per style is enough to review one.
///
/// Soft-wrapped, not hard-wrapped, on purpose. The scanner does not carry
/// inline styling across a line break — that is what makes a one-line re-scan
/// per keystroke possible — so a hard-wrapped `**bold\nrun**` is honestly not
/// bold, and a sample written that way would look like a rendering bug.
const SAMPLE: &str = r#"# Typesetting Notes

A note on how this document is set, and *why* the measure matters more than the face. A line of prose is comfortable at somewhere between sixty and seventy-five characters; past that the eye loses the start of the **next line** on the way back, and reading turns into work.

> Anything that is worth saying is worth saying at the right width.
> — a compositor, probably

## Measure and rhythm

The eight styles differ in exactly the ways that matter to reading:

- measure, leading and face
- how a heading announces itself
- whether a rule sits under a section
- what a quote looks like when it is not a box

## Tables and figures

| Style     | Measure | Leading |
|-----------|---------|---------|
| Newsprint | 37rem   | 1.50    |
| Academic  | 36rem   | 1.58    |
| Book      | 34rem   | 1.80    |

## Code and math

An inline `--measure` is set per style, never in the document. The block below is what a fence looks like:

```rust
let side = ((width - measure) / 2).max(MIN_SIDE_MARGIN);
view.set_left_margin(side);
```

---

## Open items

- [x] Draw the rules the tags cannot
- [ ] Decide about hyphenation
- [ ] Ask about ~~three~~ two more styles

See [the handoff](https://example.com/handoff) for the rest.
"#;

fn main() -> glib::ExitCode {
    let out_dir: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "preview".to_string())
        .into();
    let scheme_name = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "light".to_string());
    if !matches!(scheme_name.as_str(), "light" | "dark") {
        eprintln!("unknown scheme {scheme_name:?}; expected \"light\" or \"dark\"");
        return glib::ExitCode::FAILURE;
    }
    let third = std::env::args().nth(3);
    let bottom = third.as_deref() == Some("bottom");
    let popover = third.as_deref() == Some("popover");
    let suffix = if bottom { "-bottom" } else { "" };

    let app = adw::Application::builder()
        .application_id("us.hagreli.Vellum.Preview")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_startup(|_| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(include_str!("../src/ui/style.css"));
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });

    app.connect_activate(move |app| {
        std::fs::create_dir_all(&out_dir).expect("create output directory");

        // The style picker is a popover, which is a surface of its own and so
        // never appears in a snapshot of the window it hangs off. Its contents
        // are an ordinary box, though, so they can be lifted into a window and
        // captured like anything else.
        if popover {
            adw::StyleManager::default().set_color_scheme(if scheme_name == "dark" {
                adw::ColorScheme::ForceDark
            } else {
                adw::ColorScheme::ForceLight
            });

            let picker = StylePopover::new();
            picker.set_selected("newsprint");
            picker.set_mode(if scheme_name == "dark" {
                vellum::model::Mode::Dark
            } else {
                vellum::model::Mode::Light
            });
            let contents = picker.child().expect("the picker builds its contents");
            picker.set_child(gtk::Widget::NONE);

            let window = gtk::ApplicationWindow::builder()
                .application(app)
                .default_width(320)
                .default_height(560)
                .child(&contents)
                .build();
            window.present();

            let path = out_dir.join(format!("{scheme_name}-picker.png"));
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(1500),
                glib::clone!(
                    #[weak]
                    app,
                    #[weak]
                    window,
                    move || {
                        match capture(window.upcast_ref::<gtk::Widget>()) {
                            Some(texture) => {
                                texture.save_to_png(&path).expect("write png");
                                println!("wrote {}", path.display());
                            }
                            None => eprintln!("could not capture the picker"),
                        }
                        window.destroy();
                        app.quit();
                    }
                ),
            );
            return;
        }

        // A real file, so the preview goes through the real open path and the
        // window subtitle has something to say.
        let sample = out_dir.join("typesetting-notes.md");
        std::fs::write(&sample, SAMPLE).expect("write the sample document");

        let mut pending: Vec<(Window, PathBuf)> = Vec::new();
        for style in style::ALL {
            let window = Window::new(app);
            window.set_default_size(1180, 820);
            window.open_path(&sample);
            window.present();

            // Through the actions, so the preview exercises what a click does
            // rather than a path of its own. The mode is left alone: a document
            // opens rendered, and these frames are meant to be what launching
            // the app actually looks like.
            for (action, target) in [
                ("win.appearance", scheme_name.as_str()),
                ("win.reading-style", style.id),
            ] {
                let _ = gtk::prelude::WidgetExt::activate_action(
                    &window,
                    action,
                    Some(&target.to_variant()),
                );
            }

            pending.push((
                window,
                out_dir.join(format!("{scheme_name}-{}{suffix}.png", style.id)),
            ));
        }

        // Give the compositor a couple of frames to map and paint everything
        // before asking for pixels; an unmapped widget renders empty.
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(2000),
            glib::clone!(
                #[weak]
                app,
                move || {
                    if bottom {
                        // Through the scrollbar's own adjustment rather than
                        // `scroll_to_iter`: a text view lays its lines out
                        // lazily, and asking to scroll to a line it has not
                        // measured yet does nothing at all.
                        for (window, _) in &pending {
                            if let Some(adjustment) =
                                window.view().and_then(|view| view.vadjustment())
                            {
                                adjustment.set_value(adjustment.upper());
                            }
                        }
                        while glib::MainContext::default().iteration(false) {}
                        for (window, _) in &pending {
                            if let Some(adjustment) =
                                window.view().and_then(|view| view.vadjustment())
                            {
                                adjustment.set_value(adjustment.upper() - adjustment.page_size());
                            }
                        }
                        while glib::MainContext::default().iteration(false) {}
                    }
                    for (window, path) in &pending {
                        match capture(window.upcast_ref::<gtk::Widget>()) {
                            Some(texture) => {
                                texture.save_to_png(path).expect("write png");
                                println!("wrote {}", path.display());
                            }
                            None => eprintln!("could not capture {}", path.display()),
                        }
                    }
                    for (window, _) in &pending {
                        window.destroy();
                    }
                    app.quit();
                }
            ),
        );
    });

    app.run_with_args::<&str>(&[])
}

/// Snapshot a whole window into a texture.
fn capture(widget: &gtk::Widget) -> Option<gdk::Texture> {
    let width = widget.width();
    let height = widget.height();
    if width <= 0 || height <= 0 {
        return None;
    }

    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width as f64, height as f64);
    let node = snapshot.to_node()?;

    let renderer = gsk::CairoRenderer::new();
    renderer.realize(gdk::Surface::NONE).ok()?;
    let texture = renderer.render_texture(&node, None);
    renderer.unrealize();
    Some(texture)
}
