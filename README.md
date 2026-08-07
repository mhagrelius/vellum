# vellum

A Markdown reader and editor for GNOME. It shows a document as it would be
published, while it is still the file you are editing.

```sh
./install.sh
vellum notes.md
```

## One view, not two

Most Markdown apps give you a source pane and a preview pane, and ask you to
look at both. Vellum has one `GtkTextView`. The syntax characters are still in
the buffer — they are what gets saved — but they carry a tag with `invisible`
set, so the document reads as prose. Move the caret into a bold run and its
asterisks come back; move it away and they go again.

That is the whole architecture, and it is why the caret lands where you clicked
and the scroll position never jumps when you change mode.

| Mode | Shows | |
|---|---|---|
| **Reading** | none of it, and no caret | `Ctrl+3` — the default |
| **Live** | the syntax of the construct holding the caret, and nothing else | `Ctrl+1` |
| **Source** | every marker, for when the Markdown itself is the thing you are working on | `Ctrl+2` |

A document **opens rendered**. The caret starts at the top of the file, which in
live mode is inside the first heading — so opening in live mode would make the
first thing on screen that heading's `# `, which is the opposite of the point.
One click, or `Ctrl+1`, and you are editing.

The scanner underneath is [quill](https://github.com/mhagrelius/quill), shared
with Stickies and Brain. It reports *which characters are syntax* rather than
"this range is bold", which is the distinction the whole approach rests on.

## Eight reading styles

A style is a complete typographic system — face, measure, leading, heading
treatment, rule weights, code framing and a colour palette in each of light and
dark. Picking one repaints the page instantly; there is no transition, because
an instant repaint reads as more responsive than a cross-fade.

| | Face | Measure | Leading | The idea |
|---|---|---|---|---|
| Adwaita | Adwaita Sans | 42rem | 1.65 | the desktop's own |
| Newsprint | PT Serif | 37rem | 1.50 | a broadsheet: tight, with small-cap section rules |
| Academic | EB Garamond | 36rem | 1.58 | justified, centred title, oxblood accents |
| Sepia | Source Serif 4 | 38rem | 1.72 | warm paper, loose leading, italic asides |
| Book | Libre Baskerville | 34rem | 1.80 | the narrowest measure, letterspaced small caps |
| Terminal | JetBrains Mono | 44rem | 1.70 | monospace throughout, for notes that are mostly code |
| Contrast | Space Grotesk | 39rem | 1.62 | black on white, the largest headings, framed not filled |
| Typewriter | Courier | 35rem | 1.85 | a manuscript, dashed code frame, no colour at all |

The named faces are not bundled. Every style carries generic fallbacks, so a
machine without PT Serif still gets a serif at the right measure and leading —
install the faces to see a style as it was drawn.

A reading style is the one surface in the app that does **not** follow the
desktop theme or accent colour: a page whose ink turned blue because the user
changed their accent would not be Newsprint any more. Everything around it —
header bar, outline, popovers, status bar — is libadwaita's, unmodified.

## Formatting

Right-click in the document. The platform's own Cut, Copy, Paste, Select All
and Undo are still there — the formatting commands are *appended* to that menu
rather than replacing it — followed by Bold, Italic, Strikethrough, Inline Code
and Insert Link; Paragraph and Heading 1–3; and the block commands.

Every command is a toggle: pressing Bold on a bold run takes it off rather than
nesting a second pair of asterisks, and asking for Heading 2 on a line that is
already Heading 1 replaces the level instead of stacking hashes. What each one
*writes* is quill's decision, so a command can never produce syntax the view
renders as plain prose.

| | |
|---|---|
| `Ctrl+B` `Ctrl+I` `Ctrl+E` `Ctrl+K` | bold, italic, inline code, link |
| `Alt+0` … `Alt+3` | paragraph, heading 1–3 |
| `Ctrl+1` `Ctrl+2` `Ctrl+3` | live, source, reading |
| `Ctrl+Shift+T` | reading style |
| `F9` | show or hide the outline |
| `Ctrl+F` | find in document |

## Two halves

`core/` is a crate of its own — `vellum-core` — and links no UI toolkit at all.
The reading styles, the outline, the word count, the formatting commands and the
file on disk are all there, as plain functions over `&str` and character
offsets. `src/ui/` is the GNOME frontend and the only half that knows a window
exists.

The split is a crate rather than a module for one reason: a frontend on another
platform links `vellum-core` without dragging libadwaita in behind it. Nothing
in `core/` would have to move to add one — it would write its own `ui/` and
render `h1_scale: 2.15` however its toolkit prefers.

The line falls at *presentation*. Which characters are syntax is quill's answer
for everyone; **when to show them** is a mode, and modes live in the frontend.

## What it does not do

- **No hyphenation.** Two styles ask for it and Pango has no hyphenation
  dictionary to give them.
- **Negative letter-spacing is clamped to zero.** `GtkTextTag:letter-spacing` is
  declared non-negative and aborts the process if set below it, so the slight
  tightening four styles ask for at display sizes is dropped. The letterspacing
  the other four are built on — Book's small caps, Newsprint's section rules —
  is positive and comes through.
- **Inline code is a tint, not a frame.** A text tag can set a background and not
  a border. Fenced blocks keep their real frames, which are drawn rather than
  tagged.
- **Task checkboxes read as `[x]`**, not as an icon. They are Markdown, and this
  app does not put anything in the file another editor would not understand.

## Building

```sh
./test.sh              # fmt, clippy, and the whole suite
./test.sh --headless   # the same under Xvfb and a private D-Bus session
./install.sh           # release build into ~/.local
packaging/build-deb.sh
packaging/build-flatpak.sh
```

`cargo run --example preview -- /tmp/vellum-preview light` renders the real
window in all eight styles to PNG, which is how the typography gets reviewed
without a screenshot tool.

## Licence

GPL-3.0-or-later.
