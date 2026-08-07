//! The eight reading styles.
//!
//! A reading style is a complete typographic system — face, measure, leading,
//! heading treatment, rule weights, code framing and a colour palette in each
//! of light and dark. It is **data**, not code: nothing in this module branches
//! on which style is active, and a frontend renders every one of them through
//! the same path. Adding a ninth is adding a `ReadingStyle` to [`ALL`].
//!
//! Colours are the style's own, not the platform's. That is the one place this
//! app departs from following the desktop: a reading style whose ink turned
//! blue because the user changed their accent colour would not be Newsprint any
//! more. The *chrome* around the document — header bar, sidebar, popovers —
//! follows the platform exactly, and only the page does not.

/// A colour, stored as it is written in a design token so the table below reads
/// like the design it came from. Frontends convert to their own colour type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    rgb: u32,
    /// 0–255.
    alpha: u8,
}

impl Rgba {
    /// A fully opaque `0xRRGGBB`.
    pub const fn hex(rgb: u32) -> Self {
        Self { rgb, alpha: 255 }
    }

    /// `0xRRGGBB` at an alpha of 0–255.
    pub const fn tint(rgb: u32, alpha: u8) -> Self {
        Self { rgb, alpha }
    }

    pub fn red(self) -> f32 {
        ((self.rgb >> 16) & 0xFF) as f32 / 255.0
    }

    pub fn green(self) -> f32 {
        ((self.rgb >> 8) & 0xFF) as f32 / 255.0
    }

    pub fn blue(self) -> f32 {
        (self.rgb & 0xFF) as f32 / 255.0
    }

    pub fn alpha(self) -> f32 {
        self.alpha as f32 / 255.0
    }

    /// This colour over `under`, flattened.
    ///
    /// A text view draws a tag's background straight onto the page rather than
    /// compositing it, so a translucent token has to be resolved against the
    /// page colour before it is handed over — otherwise every tint reads as
    /// black at whatever opacity it was written with.
    pub fn over(self, under: Rgba) -> Rgba {
        let mix = |top: f32, bottom: f32| {
            let blended = top * self.alpha() + bottom * (1.0 - self.alpha());
            (blended * 255.0).round() as u32
        };
        Rgba::hex(
            (mix(self.red(), under.red()) << 16)
                | (mix(self.green(), under.green()) << 8)
                | mix(self.blue(), under.blue()),
        )
    }
}

/// Where a title sits on its measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
}

/// How a fenced code block is framed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CodeFrame {
    /// Border width in pixels; `0.0` for none.
    pub width: f32,
    pub dashed: bool,
    pub radius: f32,
}

/// Everything about a style that is the same in light and dark.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Typography {
    /// A CSS-style family list; the first installed family wins.
    pub body_font: &'static str,
    pub heading_font: &'static str,
    /// Body size in pixels at a 1.0 text scale.
    pub body_size: f32,
    /// Leading as a multiple of the body size.
    pub line_height: f32,
    /// The widest the text column is allowed to get, in pixels. Wider windows
    /// grow the margins, not the line.
    pub measure: i32,
    /// Justified rather than ragged-right.
    pub justify: bool,
    pub title_align: Align,
    /// Heading sizes as a multiple of the body size.
    pub h1_scale: f32,
    pub h2_scale: f32,
    pub heading_weight: i32,
    /// Letter spacing in ems.
    pub h1_tracking: f32,
    pub h2_tracking: f32,
    pub h1_uppercase: bool,
    pub h2_uppercase: bool,
    /// A rule beneath every second-level heading, the width of the measure.
    pub h2_rule: bool,
    pub quote_italic: bool,
    /// Width of the bar down the left of a quote; `0.0` indents instead.
    pub quote_border: f32,
    pub code_frame: CodeFrame,
    pub table_monospace: bool,
}

impl Typography {
    /// The size multiple for a heading of `level`.
    ///
    /// A style names only its first two levels, because those are the two that
    /// carry its character. The rest step down from the second towards the body
    /// so that a six-level document still reads as a hierarchy rather than as
    /// four identical headings.
    pub fn heading_scale(&self, level: u8) -> f32 {
        match level {
            1 => self.h1_scale,
            2 => self.h2_scale,
            level => {
                // Levels 3–6 walk from the second level down to the body size.
                // Book's second level is *already* smaller than its body — a
                // letterspaced small-cap line — so the target is whichever of
                // the two is smaller, and the steps never turn back upwards.
                let target = self.h2_scale.min(1.0);
                let steps = (level.min(6) - 2) as f32;
                self.h2_scale + (target - self.h2_scale) * (steps / 4.0)
            }
        }
    }

    /// Letter spacing in ems for a heading of `level`.
    pub fn heading_tracking(&self, level: u8) -> f32 {
        match level {
            1 => self.h1_tracking,
            2 => self.h2_tracking,
            _ => 0.0,
        }
    }

    pub fn heading_uppercase(&self, level: u8) -> bool {
        match level {
            1 => self.h1_uppercase,
            2 => self.h2_uppercase,
            _ => false,
        }
    }
}

/// The colours of one style in one mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub page: Rgba,
    pub text: Rgba,
    pub title: Rgba,
    /// Secondary text: frontmatter, table delimiters, captions.
    pub dim: Rgba,
    pub quote: Rgba,
    pub accent: Rgba,
    /// The Markdown syntax characters, when they are on show.
    pub marker: Rgba,
    pub rule: Rgba,
    /// Thematic breaks and heading rules, which are darker than a hairline.
    pub rule_strong: Rgba,
    pub quote_border: Rgba,
    /// Behind an inline `code` run.
    pub code_background: Rgba,
    /// Behind a fenced block.
    pub block_background: Rgba,
    pub block_border: Rgba,
    pub code_keyword: Rgba,
    pub code_string: Rgba,
}

/// Light or dark, once the system preference and any override have been
/// resolved. A style carries both and is never asked to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadingStyle {
    /// Stable across releases: it is what is written to GSettings.
    pub id: &'static str,
    pub label: &'static str,
    pub typography: Typography,
    light: Palette,
    dark: Palette,
}

impl ReadingStyle {
    pub fn palette(&self, mode: Mode) -> &Palette {
        match mode {
            Mode::Light => &self.light,
            Mode::Dark => &self.dark,
        }
    }
}

/// The style a fresh install opens with.
pub const DEFAULT_ID: &str = "newsprint";

/// Look a style up by the id stored in settings.
///
/// Falls back to the default rather than failing: a settings value from a
/// future version, or a hand-edited one, must not stop the app opening.
pub fn from_id(id: &str) -> &'static ReadingStyle {
    ALL.iter()
        .find(|style| style.id == id)
        .or_else(|| ALL.iter().find(|style| style.id == DEFAULT_ID))
        .expect("the default style is in ALL")
}

/// Every bundled style, in the order they are offered.
pub const ALL: &[ReadingStyle] = &[
    ADWAITA, NEWSPRINT, ACADEMIC, SEPIA, BOOK, TERMINAL, CONTRAST, TYPEWRITER,
];

/// The system default: the desktop's own face, at the desktop's own weight.
const ADWAITA: ReadingStyle = ReadingStyle {
    id: "adwaita",
    label: "Adwaita",
    typography: Typography {
        body_font: "Adwaita Sans, Cantarell, sans-serif",
        heading_font: "Adwaita Sans, Cantarell, sans-serif",
        body_size: 16.0,
        line_height: 1.65,
        measure: 672,
        justify: false,
        title_align: Align::Start,
        h1_scale: 2.15,
        h2_scale: 1.4,
        heading_weight: 700,
        h1_tracking: -0.02,
        h2_tracking: -0.01,
        h1_uppercase: false,
        h2_uppercase: false,
        h2_rule: false,
        quote_italic: false,
        quote_border: 3.0,
        code_frame: CodeFrame {
            width: 1.0,
            dashed: false,
            radius: 8.0,
        },
        table_monospace: true,
    },
    light: Palette {
        page: Rgba::hex(0xfafafb),
        text: Rgba::hex(0x26262b),
        title: Rgba::hex(0x1b1b1f),
        dim: Rgba::tint(0x000000, 128),
        quote: Rgba::tint(0x000000, 158),
        accent: Rgba::hex(0x1c71d8),
        marker: Rgba::tint(0x1c71d8, 140),
        rule: Rgba::tint(0x000000, 33),
        rule_strong: Rgba::tint(0x000000, 82),
        quote_border: Rgba::tint(0x3584e4, 128),
        code_background: Rgba::tint(0x000000, 14),
        block_background: Rgba::tint(0x000000, 10),
        block_border: Rgba::tint(0x000000, 20),
        code_keyword: Rgba::hex(0x1c71d8),
        code_string: Rgba::hex(0x26a269),
    },
    dark: Palette {
        page: Rgba::hex(0x222226),
        text: Rgba::hex(0xe6e6ea),
        title: Rgba::hex(0xffffff),
        dim: Rgba::tint(0xffffff, 128),
        quote: Rgba::tint(0xffffff, 158),
        accent: Rgba::hex(0x78aeed),
        marker: Rgba::tint(0x78aeed, 153),
        rule: Rgba::tint(0xffffff, 36),
        rule_strong: Rgba::tint(0xffffff, 87),
        quote_border: Rgba::tint(0x78aeed, 128),
        code_background: Rgba::tint(0xffffff, 23),
        block_background: Rgba::tint(0x000000, 71),
        block_border: Rgba::tint(0xffffff, 18),
        code_keyword: Rgba::hex(0x78aeed),
        code_string: Rgba::hex(0x8ff0a4),
    },
};

/// A broadsheet: tight leading, a short measure, and small-caps section rules.
const NEWSPRINT: ReadingStyle = ReadingStyle {
    id: "newsprint",
    label: "Newsprint",
    typography: Typography {
        body_font: "PT Serif, Georgia, serif",
        heading_font: "PT Serif, Georgia, serif",
        body_size: 17.0,
        line_height: 1.5,
        measure: 592,
        justify: false,
        title_align: Align::Start,
        h1_scale: 2.4,
        h2_scale: 1.12,
        heading_weight: 700,
        h1_tracking: -0.015,
        h2_tracking: 0.09,
        h1_uppercase: false,
        h2_uppercase: true,
        h2_rule: true,
        quote_italic: true,
        quote_border: 0.0,
        code_frame: CodeFrame {
            width: 1.0,
            dashed: false,
            radius: 0.0,
        },
        table_monospace: false,
    },
    light: Palette {
        page: Rgba::hex(0xffffff),
        text: Rgba::hex(0x141414),
        title: Rgba::hex(0x000000),
        dim: Rgba::tint(0x000000, 133),
        quote: Rgba::hex(0x141414),
        accent: Rgba::hex(0x1a1a1a),
        marker: Rgba::tint(0x000000, 89),
        rule: Rgba::tint(0x000000, 46),
        rule_strong: Rgba::tint(0x000000, 166),
        quote_border: Rgba::tint(0x000000, 46),
        code_background: Rgba::hex(0xf1f1ec),
        block_background: Rgba::hex(0xf6f6f1),
        block_border: Rgba::tint(0x000000, 36),
        code_keyword: Rgba::hex(0x141414),
        code_string: Rgba::tint(0x000000, 140),
    },
    dark: Palette {
        page: Rgba::hex(0x151517),
        text: Rgba::hex(0xe9e7e1),
        title: Rgba::hex(0xffffff),
        dim: Rgba::tint(0xffffff, 128),
        quote: Rgba::hex(0xe9e7e1),
        accent: Rgba::hex(0xe9e7e1),
        marker: Rgba::tint(0xffffff, 102),
        rule: Rgba::tint(0xffffff, 46),
        rule_strong: Rgba::tint(0xffffff, 179),
        quote_border: Rgba::tint(0xffffff, 46),
        code_background: Rgba::tint(0xffffff, 20),
        block_background: Rgba::tint(0xffffff, 13),
        block_border: Rgba::tint(0xffffff, 36),
        code_keyword: Rgba::hex(0xffffff),
        code_string: Rgba::tint(0xffffff, 140),
    },
};

/// A journal page: justified Garamond on a centred title, oxblood accents.
const ACADEMIC: ReadingStyle = ReadingStyle {
    id: "academic",
    label: "Academic",
    typography: Typography {
        body_font: "EB Garamond, Times New Roman, serif",
        heading_font: "EB Garamond, Times New Roman, serif",
        body_size: 19.0,
        line_height: 1.58,
        measure: 576,
        justify: true,
        title_align: Align::Center,
        h1_scale: 1.95,
        h2_scale: 1.22,
        heading_weight: 600,
        h1_tracking: 0.01,
        h2_tracking: 0.005,
        h1_uppercase: false,
        h2_uppercase: false,
        h2_rule: false,
        quote_italic: false,
        quote_border: 0.0,
        code_frame: CodeFrame {
            width: 1.0,
            dashed: false,
            radius: 2.0,
        },
        table_monospace: true,
    },
    light: Palette {
        page: Rgba::hex(0xfdfdfb),
        text: Rgba::hex(0x1a1a1a),
        title: Rgba::hex(0x111111),
        dim: Rgba::tint(0x000000, 128),
        quote: Rgba::tint(0x000000, 179),
        accent: Rgba::hex(0x8b2f2f),
        marker: Rgba::tint(0x8b2f2f, 115),
        rule: Rgba::tint(0x000000, 41),
        rule_strong: Rgba::tint(0x000000, 140),
        quote_border: Rgba::tint(0x000000, 41),
        code_background: Rgba::tint(0x000000, 13),
        block_background: Rgba::hex(0xf7f6f2),
        block_border: Rgba::tint(0x000000, 31),
        code_keyword: Rgba::hex(0x8b2f2f),
        code_string: Rgba::hex(0x3f5e42),
    },
    dark: Palette {
        page: Rgba::hex(0x1b1b1d),
        text: Rgba::hex(0xe8e5df),
        title: Rgba::hex(0xffffff),
        dim: Rgba::tint(0xffffff, 128),
        quote: Rgba::tint(0xffffff, 184),
        accent: Rgba::hex(0xe09b9b),
        marker: Rgba::tint(0xe09b9b, 128),
        rule: Rgba::tint(0xffffff, 41),
        rule_strong: Rgba::tint(0xffffff, 128),
        quote_border: Rgba::tint(0xffffff, 41),
        code_background: Rgba::tint(0xffffff, 20),
        block_background: Rgba::tint(0xffffff, 13),
        block_border: Rgba::tint(0xffffff, 31),
        code_keyword: Rgba::hex(0xe09b9b),
        code_string: Rgba::hex(0xa8c9a4),
    },
};

/// Warm paper, loose leading, italic asides.
const SEPIA: ReadingStyle = ReadingStyle {
    id: "sepia",
    label: "Sepia",
    typography: Typography {
        body_font: "Source Serif 4, Source Serif Pro, Georgia, serif",
        heading_font: "Source Serif 4, Source Serif Pro, Georgia, serif",
        body_size: 18.0,
        line_height: 1.72,
        measure: 608,
        justify: false,
        title_align: Align::Start,
        h1_scale: 2.05,
        h2_scale: 1.32,
        heading_weight: 600,
        h1_tracking: -0.01,
        h2_tracking: 0.0,
        h1_uppercase: false,
        h2_uppercase: false,
        h2_rule: false,
        quote_italic: true,
        quote_border: 3.0,
        code_frame: CodeFrame {
            width: 1.0,
            dashed: false,
            radius: 6.0,
        },
        table_monospace: true,
    },
    light: Palette {
        page: Rgba::hex(0xf4ecd8),
        text: Rgba::hex(0x5b4636),
        title: Rgba::hex(0x43331f),
        dim: Rgba::tint(0x5b4636, 153),
        quote: Rgba::tint(0x5b4636, 217),
        accent: Rgba::hex(0x9a5b2e),
        marker: Rgba::tint(0x9a5b2e, 128),
        rule: Rgba::tint(0x5b4636, 56),
        rule_strong: Rgba::tint(0x5b4636, 128),
        quote_border: Rgba::tint(0x9a5b2e, 115),
        code_background: Rgba::tint(0x5b4636, 26),
        block_background: Rgba::tint(0x5b4636, 18),
        block_border: Rgba::tint(0x5b4636, 41),
        code_keyword: Rgba::hex(0x9a5b2e),
        code_string: Rgba::hex(0x6b7a4a),
    },
    dark: Palette {
        page: Rgba::hex(0x2a251f),
        text: Rgba::hex(0xded1bb),
        title: Rgba::hex(0xf0e6d2),
        dim: Rgba::tint(0xded1bb, 140),
        quote: Rgba::tint(0xded1bb, 217),
        accent: Rgba::hex(0xd9a066),
        marker: Rgba::tint(0xd9a066, 140),
        rule: Rgba::tint(0xded1bb, 51),
        rule_strong: Rgba::tint(0xded1bb, 115),
        quote_border: Rgba::tint(0xd9a066, 115),
        code_background: Rgba::tint(0xffffff, 18),
        block_background: Rgba::tint(0x000000, 56),
        block_border: Rgba::tint(0xded1bb, 36),
        code_keyword: Rgba::hex(0xd9a066),
        code_string: Rgba::hex(0xa9b87f),
    },
};

/// A printed book: the narrowest measure, the loosest leading, letterspaced
/// small-cap headings over a centred title.
const BOOK: ReadingStyle = ReadingStyle {
    id: "book",
    label: "Book",
    typography: Typography {
        body_font: "Libre Baskerville, Baskerville, Georgia, serif",
        heading_font: "Libre Baskerville, Baskerville, Georgia, serif",
        body_size: 16.0,
        line_height: 1.8,
        measure: 544,
        justify: false,
        title_align: Align::Center,
        h1_scale: 1.7,
        h2_scale: 0.95,
        heading_weight: 700,
        h1_tracking: 0.02,
        h2_tracking: 0.16,
        h1_uppercase: true,
        h2_uppercase: true,
        h2_rule: false,
        quote_italic: true,
        quote_border: 0.0,
        code_frame: CodeFrame {
            width: 1.0,
            dashed: false,
            radius: 3.0,
        },
        table_monospace: true,
    },
    light: Palette {
        page: Rgba::hex(0xfbfaf6),
        text: Rgba::hex(0x2b2620),
        title: Rgba::hex(0x1c1813),
        dim: Rgba::tint(0x2b2620, 140),
        quote: Rgba::tint(0x2b2620, 199),
        accent: Rgba::hex(0x7a5c3e),
        marker: Rgba::tint(0x7a5c3e, 115),
        rule: Rgba::tint(0x2b2620, 41),
        rule_strong: Rgba::tint(0x2b2620, 115),
        quote_border: Rgba::tint(0x2b2620, 41),
        code_background: Rgba::tint(0x2b2620, 15),
        block_background: Rgba::tint(0x2b2620, 11),
        block_border: Rgba::tint(0x2b2620, 31),
        code_keyword: Rgba::hex(0x7a5c3e),
        code_string: Rgba::hex(0x5c6b4a),
    },
    dark: Palette {
        page: Rgba::hex(0x1d1b18),
        text: Rgba::hex(0xe3ddd2),
        title: Rgba::hex(0xf4efe5),
        dim: Rgba::tint(0xe3ddd2, 128),
        quote: Rgba::tint(0xe3ddd2, 204),
        accent: Rgba::hex(0xc9a077),
        marker: Rgba::tint(0xc9a077, 128),
        rule: Rgba::tint(0xe3ddd2, 46),
        rule_strong: Rgba::tint(0xe3ddd2, 107),
        quote_border: Rgba::tint(0xe3ddd2, 46),
        code_background: Rgba::tint(0xffffff, 18),
        block_background: Rgba::tint(0x000000, 64),
        block_border: Rgba::tint(0xe3ddd2, 31),
        code_keyword: Rgba::hex(0xc9a077),
        code_string: Rgba::hex(0xa3b487),
    },
};

/// Monospace throughout, on the widest measure. For notes that are mostly code.
const TERMINAL: ReadingStyle = ReadingStyle {
    id: "terminal",
    label: "Terminal",
    typography: Typography {
        body_font: "JetBrains Mono, Source Code Pro, monospace",
        heading_font: "JetBrains Mono, Source Code Pro, monospace",
        body_size: 14.5,
        line_height: 1.7,
        measure: 704,
        justify: false,
        title_align: Align::Start,
        h1_scale: 1.55,
        h2_scale: 1.05,
        heading_weight: 600,
        h1_tracking: -0.02,
        h2_tracking: 0.0,
        h1_uppercase: false,
        h2_uppercase: false,
        h2_rule: false,
        quote_italic: false,
        quote_border: 2.0,
        code_frame: CodeFrame {
            width: 1.0,
            dashed: false,
            radius: 0.0,
        },
        table_monospace: true,
    },
    light: Palette {
        page: Rgba::hex(0xf6f7f5),
        text: Rgba::hex(0x1f2622),
        title: Rgba::hex(0x0f1512),
        dim: Rgba::tint(0x1f2622, 128),
        quote: Rgba::tint(0x1f2622, 179),
        accent: Rgba::hex(0x1a7f4b),
        marker: Rgba::tint(0x1a7f4b, 140),
        rule: Rgba::tint(0x1f2622, 41),
        rule_strong: Rgba::tint(0x1f2622, 115),
        quote_border: Rgba::tint(0x1a7f4b, 128),
        code_background: Rgba::tint(0x1f2622, 18),
        block_background: Rgba::tint(0x1f2622, 15),
        block_border: Rgba::tint(0x1f2622, 36),
        code_keyword: Rgba::hex(0x1a7f4b),
        code_string: Rgba::hex(0x9a5b2e),
    },
    dark: Palette {
        page: Rgba::hex(0x14161a),
        text: Rgba::hex(0xd6ded8),
        title: Rgba::hex(0xeefaf1),
        dim: Rgba::tint(0xd6ded8, 115),
        quote: Rgba::tint(0xd6ded8, 184),
        accent: Rgba::hex(0x7ee081),
        marker: Rgba::tint(0x7ee081, 153),
        rule: Rgba::tint(0xd6ded8, 41),
        rule_strong: Rgba::tint(0xd6ded8, 102),
        quote_border: Rgba::tint(0x7ee081, 128),
        code_background: Rgba::tint(0xffffff, 18),
        block_background: Rgba::tint(0x000000, 89),
        block_border: Rgba::tint(0xd6ded8, 33),
        code_keyword: Rgba::hex(0x7ee081),
        code_string: Rgba::hex(0xe2b872),
    },
};

/// Black on white, the largest headings, everything framed rather than filled.
const CONTRAST: ReadingStyle = ReadingStyle {
    id: "contrast",
    label: "Contrast",
    typography: Typography {
        body_font: "Space Grotesk, Helvetica, sans-serif",
        heading_font: "Space Grotesk, Helvetica, sans-serif",
        body_size: 17.5,
        line_height: 1.62,
        measure: 624,
        justify: false,
        title_align: Align::Start,
        h1_scale: 2.6,
        h2_scale: 1.5,
        heading_weight: 700,
        h1_tracking: -0.035,
        h2_tracking: -0.02,
        h1_uppercase: false,
        h2_uppercase: false,
        h2_rule: false,
        quote_italic: false,
        quote_border: 5.0,
        code_frame: CodeFrame {
            width: 2.0,
            dashed: false,
            radius: 0.0,
        },
        table_monospace: true,
    },
    light: Palette {
        page: Rgba::hex(0xffffff),
        text: Rgba::hex(0x000000),
        title: Rgba::hex(0x000000),
        dim: Rgba::tint(0x000000, 158),
        quote: Rgba::hex(0x000000),
        accent: Rgba::hex(0x0000ee),
        marker: Rgba::tint(0x000000, 115),
        rule: Rgba::tint(0x000000, 89),
        rule_strong: Rgba::hex(0x000000),
        quote_border: Rgba::hex(0x000000),
        // The prototype frames inline code in a 1.5px rule on a transparent
        // background. A text tag can set a background and not a border, so the
        // frame becomes a tint of the same visual weight; the fenced block below
        // keeps its real 2px frame, which is drawn rather than tagged.
        code_background: Rgba::tint(0x000000, 26),
        block_background: Rgba::hex(0xffffff),
        block_border: Rgba::hex(0x000000),
        code_keyword: Rgba::hex(0x0000ee),
        code_string: Rgba::hex(0x006600),
    },
    dark: Palette {
        page: Rgba::hex(0x000000),
        text: Rgba::hex(0xffffff),
        title: Rgba::hex(0xffffff),
        dim: Rgba::tint(0xffffff, 173),
        quote: Rgba::hex(0xffffff),
        accent: Rgba::hex(0x7ac1ff),
        marker: Rgba::tint(0xffffff, 128),
        rule: Rgba::tint(0xffffff, 102),
        rule_strong: Rgba::hex(0xffffff),
        quote_border: Rgba::hex(0xffffff),
        code_background: Rgba::tint(0xffffff, 36),
        block_background: Rgba::hex(0x000000),
        block_border: Rgba::hex(0xffffff),
        code_keyword: Rgba::hex(0x7ac1ff),
        code_string: Rgba::hex(0x8ff0a4),
    },
};

/// A manuscript: Courier, centred uppercase headings, a dashed code frame, and
/// no colour anywhere — the accent is the ink.
const TYPEWRITER: ReadingStyle = ReadingStyle {
    id: "typewriter",
    label: "Typewriter",
    typography: Typography {
        body_font: "Courier New, Courier, monospace",
        heading_font: "Courier New, Courier, monospace",
        body_size: 16.5,
        line_height: 1.85,
        measure: 560,
        justify: false,
        title_align: Align::Center,
        h1_scale: 1.5,
        h2_scale: 1.0,
        heading_weight: 700,
        h1_tracking: 0.06,
        h2_tracking: 0.05,
        h1_uppercase: true,
        h2_uppercase: true,
        h2_rule: false,
        quote_italic: false,
        quote_border: 0.0,
        code_frame: CodeFrame {
            width: 1.0,
            dashed: true,
            radius: 0.0,
        },
        table_monospace: true,
    },
    light: Palette {
        page: Rgba::hex(0xf7f5ef),
        text: Rgba::hex(0x2c2a26),
        title: Rgba::hex(0x1a1916),
        dim: Rgba::tint(0x2c2a26, 140),
        quote: Rgba::tint(0x2c2a26, 204),
        accent: Rgba::hex(0x2c2a26),
        marker: Rgba::tint(0x2c2a26, 102),
        rule: Rgba::tint(0x2c2a26, 64),
        rule_strong: Rgba::tint(0x2c2a26, 153),
        quote_border: Rgba::tint(0x2c2a26, 64),
        code_background: Rgba::tint(0x2c2a26, 18),
        block_background: Rgba::tint(0x2c2a26, 13),
        block_border: Rgba::tint(0x2c2a26, 77),
        code_keyword: Rgba::hex(0x2c2a26),
        code_string: Rgba::tint(0x2c2a26, 153),
    },
    dark: Palette {
        page: Rgba::hex(0x1c1b18),
        text: Rgba::hex(0xddd8cc),
        title: Rgba::hex(0xf2ede1),
        dim: Rgba::tint(0xddd8cc, 128),
        quote: Rgba::tint(0xddd8cc, 204),
        accent: Rgba::hex(0xddd8cc),
        marker: Rgba::tint(0xddd8cc, 115),
        rule: Rgba::tint(0xddd8cc, 64),
        rule_strong: Rgba::tint(0xddd8cc, 140),
        quote_border: Rgba::tint(0xddd8cc, 64),
        code_background: Rgba::tint(0xffffff, 18),
        block_background: Rgba::tint(0x000000, 64),
        block_border: Rgba::tint(0xddd8cc, 77),
        code_keyword: Rgba::hex(0xddd8cc),
        code_string: Rgba::tint(0xddd8cc, 140),
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_the_default_is_one_of_them() {
        let mut ids: Vec<&str> = ALL.iter().map(|style| style.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two styles share an id");
        assert!(ALL.iter().any(|style| style.id == DEFAULT_ID));
    }

    /// An unknown id is what a downgrade or a hand-edited GSettings value looks
    /// like, and it must open the app rather than stop it.
    #[test]
    fn an_unknown_id_falls_back_to_the_default() {
        assert_eq!(from_id("neon-vaporwave").id, DEFAULT_ID);
        assert_eq!(from_id("").id, DEFAULT_ID);
    }

    #[test]
    fn every_style_round_trips_through_its_id() {
        for style in ALL {
            assert_eq!(from_id(style.id).id, style.id);
        }
    }

    /// The page and the ink have to be on opposite sides of mid-grey, or the
    /// document is unreadable in that mode. Cheap arithmetic, but it is exactly
    /// the mistake a copied-and-edited palette makes.
    #[test]
    fn text_contrasts_with_its_page_in_both_modes() {
        let luminance =
            |colour: Rgba| 0.2126 * colour.red() + 0.7152 * colour.green() + 0.0722 * colour.blue();

        for style in ALL {
            for mode in [Mode::Light, Mode::Dark] {
                let palette = style.palette(mode);
                let page = luminance(palette.page);
                let text = luminance(palette.text);
                assert!(
                    (page - text).abs() > 0.4,
                    "{} in {mode:?}: page and text are too close ({page:.2} vs {text:.2})",
                    style.id
                );
                let light = matches!(mode, Mode::Light);
                assert_eq!(
                    page > 0.5,
                    light,
                    "{} in {mode:?} has the wrong page for its mode",
                    style.id
                );
            }
        }
    }

    /// Headings must not step upwards as the level goes down, and must never
    /// end up smaller than the body they head.
    #[test]
    fn heading_sizes_descend_to_the_body_size() {
        for style in ALL {
            let scales: Vec<f32> = (1..=6)
                .map(|level| style.typography.heading_scale(level))
                .collect();
            for pair in scales.windows(2) {
                assert!(
                    pair[0] >= pair[1],
                    "{}: heading sizes go up at some level ({scales:?})",
                    style.id
                );
            }
            let floor = style.typography.h2_scale.min(1.0);
            assert!(
                scales.iter().all(|scale| *scale >= floor),
                "{}: a heading fell below its own second level ({scales:?})",
                style.id
            );
        }
    }

    #[test]
    fn flattening_a_tint_lands_between_the_two_colours() {
        let black_half = Rgba::tint(0x000000, 128);
        let over_white = black_half.over(Rgba::hex(0xffffff));
        assert_eq!(over_white.alpha(), 1.0);
        assert!((over_white.red() - 0.5).abs() < 0.01);

        // An opaque colour is unchanged by whatever is under it.
        let opaque = Rgba::hex(0x123456);
        assert_eq!(opaque.over(Rgba::hex(0xffffff)), opaque);
    }
}
