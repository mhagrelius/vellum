# vellum

A Markdown reader and editor for GNOME. Read `README.md` first — it is current,
and it explains the one-view architecture that everything else follows from.

## Stack

GTK 4.22 + libadwaita 1.9 via gtk4-rs 0.11 / libadwaita-rs 0.9, Rust edition
2021 (MSRV 1.80). `gio` is a direct dependency purely to raise the API level to
v2_80 — leave it.

A cargo workspace, not one crate: `core/` is `vellum-core` and links no toolkit,
so a frontend on another platform can have it without libadwaita. The root
package is the GNOME app and re-exports the core as `vellum::model`. Same shape
as `planner`.

## Commands

- `./test.sh` — fmt check, clippy with `-D warnings`, then `cargo test
  --workspace --all-targets`. Add `--headless` for Xvfb + a private D-Bus
  session. This is the gate; run it, not bare `cargo test`.
- **Never run `dbus-run-session` or `xvfb-run -a dbus-run-session` directly** —
  use `isolated-bus [--headless] -- CMD`. A private bus activates its own
  `xdg-document-portal`, which mounts over `/run/user/$UID/doc` and takes the
  login session's portal down with it when the bus exits; every flatpak on the
  machine then fails to launch until it is restarted. `test.sh --headless`
  guards against this internally, but one-off runs of a single test bypass it.
- `cargo run --example preview -- DIR light|dark [bottom]` — renders the real
  window in all eight reading styles to PNG. **Use this to review anything
  visual.** Screenshotting a live Wayland session needs interactive consent, and
  compiling clean draws zero frames.
- `./install.sh` / `./uninstall.sh`, `packaging/build-deb.sh`,
  `packaging/build-flatpak.sh`.

`test.sh` compiles the GSettings schema into a throwaway directory and points
`GSETTINGS_SCHEMA_DIR` at it. That is also the only check that the schema is
valid and carries the keys the code reads — keep new keys in
`data/us.hagreli.Vellum.gschema.xml` and the app's fallback path working, since
running from the build tree with no schema installed is normal.

## Layout

- `core/src/style.rs` — the eight reading styles, as data. Adding a ninth is
  adding a `ReadingStyle` to `ALL`; no rendering code is touched.
- `core/src/format.rs` — what Bold does to a selection, including undoing
  itself. Pure functions of text and two character offsets.
- `core/src/decoration.rs` — the line ranges the view *draws* rather than tags.
- `src/ui/document_view.rs` — the page. Tags, live markers, and the `snapshot`
  override that paints the page colour, the block fills and every rule.
- `src/ui/window.rs` — chrome, actions, settings.

## Things that bite

- **A tag's `left-margin` replaces the view's, it does not add to it.** Every
  block tag therefore carries the column's own side margin plus its indent, and
  they are all recomputed in `apply_margins` on resize. Getting this wrong
  strands quotes and lists at the window edge while the prose sits centred.
- **A CSS provider goes on the display, not the widget** (the per-widget route
  was deprecated in GTK 4.10), so each `DocumentView` has a CSS *name* of its
  own and its stylesheet selects `textview#vellum-document-N`. A shared class
  meant the last view built decided the face for all of them.
- **The document font must be set on the `text` node as well as the widget
  node.** The theme names a font on the text node, and an inherited value loses
  to a named one however specific the outer selector is.
- **`GtkTextTag:letter-spacing` is non-negative and aborts the process** if set
  below zero, rather than failing the call.
- **In reading mode a caret is painted at the insert mark even though
  `cursor-visible` reads back as false.** It honours `caret-color`, so the
  stylesheet paints it `transparent` in that mode.
- **A document opens in reading mode**, not live. The caret starts at offset 0,
  which is inside the first heading, so live mode would greet every document
  with its own `# `. `DocumentView::apply_view_mode` exists because the initial
  mode has to take effect too — `set_view_mode` only runs on a change, and a
  view left at GTK's defaults is editable with a caret in it while claiming to
  be a reader.
- Inline styling does not cross a line break — that is what makes quill's
  one-line re-scan possible. A hard-wrapped `**bold\nrun**` is honestly not
  bold; sample documents should be soft-wrapped or they look like a rendering
  bug.

## Conventions

- Use the `developing-gtk-apps` and `designing-gnome-ui` skills for widget,
  threading, and HIG decisions rather than deriving them again.
- Edit files with the Edit tool. Do not rewrite Rust sources through
  `python3 - <<PY` heredocs or `sed -i`.
- The sibling apps (brain, familiar, planner, stickies, scribe) share this
  layout and these scripts; a pattern established in one is the pattern here.
- `restyle` re-scans the whole document on every keystroke, which is what quill
  is built to survive. `quill::scan_line` re-scans a single line if that ever
  becomes a measured problem — do not build that machinery speculatively.
