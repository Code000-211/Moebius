//! Every colour, size, radius and spacing of `docs/design.md` (and of the agreed mockups in `docs/design/`) as named
//! constants, and the fonts. No other module has a colour literal.

use std::borrow::Cow;

use iced::font::{Family, Weight};
use iced::theme::Palette;
use iced::{Color, Font, Theme};

const fn hex(rgb: u32) -> Color {
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// The neutral base: ground, surfaces, lines and text.
pub const GROUND: Color = hex(0xF4F4F5);
pub const SURFACE: Color = hex(0xFFFFFF);
pub const LANE: Color = hex(0xFAFAFA);
pub const LINE: Color = hex(0xD4D4D8);
pub const TEXT: Color = hex(0x09090B);
pub const TEXT_SECONDARY: Color = hex(0x3F3F46);
pub const TEXT_MUTED: Color = hex(0x71717A);

/// Runs on its own: live runs, passing checks, progress.
pub const TEAL: Color = hex(0x0D9488);
pub const TEAL_FILL: Color = hex(0xF0FDFA);

/// Needs you: ready to merge, the inbox count, the primary button.
pub const AMBER: Color = hex(0xD97706);
pub const AMBER_FILL: Color = hex(0xFFFBEB);

/// Blocked: a broken rule, a stuck or blocked card; only these.
pub const RED: Color = hex(0xDC2626);
pub const RED_TEXT: Color = hex(0x991B1B);
pub const RED_FILL: Color = hex(0xFEF2F2);

/// The colours of the tags of a card: border, fill and text, one set for each of the three roles.
pub const TEAL_BORDER: Color = hex(0x99F6E4);
pub const AMBER_BORDER: Color = hex(0xFDE68A);
pub const AMBER_TEXT: Color = hex(0x92400E);
pub const RED_BORDER: Color = hex(0xFECACA);
pub const RED_TAG_FILL: Color = hex(0xFEF2F2);

/// The part of a progress bar that is not done.
pub const TRACK: Color = hex(0xE4E4E7);

/// The validated categorical order of the charts.
pub const CHART: [Color; 4] = [hex(0x2563EB), hex(0xEA580C), hex(0x059669), hex(0xD97706)];

const MODEL_COLOURS: [Color; 6] = [
    hex(0x2563EB),
    hex(0xEA580C),
    hex(0x059669),
    hex(0xD97706),
    hex(0x7C3AED),
    hex(0xDB2777),
];
const SHADE_WHITE: [f32; 4] = [0.65, 0.42, 0.2, 0.0];
/// A heatmap cell without usage.
pub const HEAT_EMPTY: Color = hex(0xE4E4E7);

/// The colour of the model that came `index`-th in the order of appearance.
pub fn model_colour(index: usize) -> Color {
    MODEL_COLOURS[index % MODEL_COLOURS.len()]
}

/// A shade of `color` for a heatmap cell, `level` 1 (lightest) to 4 (the colour itself).
pub fn shade(color: Color, level: usize) -> Color {
    let white = SHADE_WHITE[level.clamp(1, 4) - 1];
    Color::from_rgb(
        color.r + (1.0 - color.r) * white,
        color.g + (1.0 - color.g) * white,
        color.b + (1.0 - color.b) * white,
    )
}

/// The pill of a tool's name, and the fills of a diff, with the marks of its rows.
pub const HEAD_PILL: Color = hex(0xF4F4F5);
pub const DIFF_REMOVED: Color = hex(0xFEF2F2);
pub const DIFF_ADDED: Color = hex(0xF0FDF4);
pub const DIFF_REMOVED_MARK: Color = hex(0x991B1B);
pub const DIFF_ADDED_MARK: Color = hex(0x166534);

/// The box of an agent's steps: fill, border and the ink of its header.
pub const STEPS_FILL: Color = hex(0xF4F4F5);
pub const STEPS_BORDER: Color = hex(0xD4D4D8);
pub const STEPS_INK: Color = hex(0x18181B);

/// The dark block that shows what a tool printed.
pub const TERMINAL: Color = hex(0x09090B);
pub const TERMINAL_TEXT: Color = hex(0xE4E4E7);
pub const TERMINAL_DIM: Color = hex(0x71717A);

/// The sixteen colours of a shell: black to white, then the bright ones.
pub const TERMINAL_ANSI: [Color; 16] = [
    hex(0x3F3F46),
    hex(0xF87171),
    hex(0x4ADE80),
    hex(0xFACC15),
    hex(0x60A5FA),
    hex(0xC084FC),
    hex(0x22D3EE),
    hex(0xE4E4E7),
    hex(0x71717A),
    hex(0xFCA5A5),
    hex(0x86EFAC),
    hex(0xFDE047),
    hex(0x93C5FD),
    hex(0xD8B4FE),
    hex(0x67E8F9),
    hex(0xFAFAFA),
];

// The sidebar, a shade darker than the page.
pub const SIDEBAR: Color = hex(0xECECEE);
pub const SIDEBAR_LINE: Color = hex(0xD4D4D8);
pub const SIDEBAR_TEXT: Color = hex(0x27272A);
pub const SIDEBAR_GROUP: Color = hex(0x71717A);
pub const SIDEBAR_CURRENT: Color = hex(0xDCDCE0);
pub const SIDEBAR_HOVER: Color = hex(0xE4E4E7);
pub const ONLINE_TEXT: Color = hex(0x0F766E);
pub const ON_FILL: Color = SURFACE;
/// The shadow under what floats over the page: the floating chat's button and window.
pub const FLOAT_SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.16);

// The type scale, compact as in a desktop tool.
pub const PAGE_TITLE: f32 = 19.0;
pub const BOX_HEADING: f32 = 14.0;
pub const CARD_TITLE: f32 = 13.5;
pub const BODY: f32 = 13.0;
pub const CONTEXT: f32 = 12.5;
pub const BREADCRUMB: f32 = 12.0;
pub const LABEL: f32 = 11.0;
/// The big number of a tile.
pub const TILE_VALUE: f32 = 22.0;
/// The number above a card, a column's script and its count.
pub const SMALL: f32 = 11.0;
pub const COLUMN_NAME: f32 = 13.0;
pub const APP_NAME: f32 = 15.0;
pub const GROUP_LABEL: f32 = 11.0;
pub const CODE: f32 = 12.0;
pub const CODE_SMALL: f32 = 11.5;
/// The shell's text, and the width and height of one of its cells (JetBrains Mono is 0.6 em wide).
pub const SHELL: f32 = 12.5;
pub const SHELL_CELL: [f32; 2] = [SHELL * 0.6, 17.0];

// Radii: small, never pills.
pub const RADIUS_BOX: f32 = 6.0;
pub const RADIUS_CARD: f32 = 5.0;
pub const RADIUS_CONTROL: f32 = 4.0;
pub const RADIUS_ITEM: f32 = 4.0;
pub const RADIUS_SMALL: f32 = 3.0;
pub const RADIUS_SEGMENT: f32 = 2.0;
pub const RADIUS_ROUND: f32 = 4.0;

// Spacing and fixed widths.
pub const SIDEBAR_WIDTH: f32 = 220.0;
pub const RAIL_WIDTH: f32 = 56.0;
pub const PAGE_PADDING: [f32; 2] = [16.0, 20.0];
pub const PAGE_GAP: f32 = 12.0;
pub const ICON: f32 = 16.0;
pub const DOT: f32 = 6.0;

/// The floating chat: the round button, the window it grows into, and their distance from the window's corner.
pub const FLOAT_BUTTON: f32 = 40.0;
pub const FLOAT_WIDTH: f32 = 400.0;
pub const FLOAT_HEIGHT: f32 = 560.0;
pub const FLOAT_MARGIN: f32 = 16.0;

/// Manrope in one of the weights that are bundled.
pub fn manrope(weight: Weight) -> Font {
    Font {
        family: Family::Name("Manrope"),
        weight,
        ..Font::DEFAULT
    }
}

pub const MANROPE: Font = Font::with_name("Manrope");
/// JetBrains Mono, for code only.
pub const MONO: Font = Font::with_name("JetBrains Mono");

/// The font files, to be loaded when the app starts.
pub fn fonts() -> Vec<Cow<'static, [u8]>> {
    [
        &include_bytes!("../fonts/manrope/Manrope-400.ttf")[..],
        &include_bytes!("../fonts/manrope/Manrope-500.ttf")[..],
        &include_bytes!("../fonts/manrope/Manrope-600.ttf")[..],
        &include_bytes!("../fonts/manrope/Manrope-700.ttf")[..],
        &include_bytes!("../fonts/jetbrains-mono/JetBrainsMono-Regular.ttf")[..],
        &include_bytes!("../fonts/jetbrains-mono/JetBrainsMono-Medium.ttf")[..],
        &include_bytes!("../fonts/jetbrains-mono/JetBrainsMono-Italic.ttf")[..],
        &include_bytes!("../fonts/jetbrains-mono/JetBrainsMono-Bold.ttf")[..],
        &include_bytes!("../fonts/jetbrains-mono/JetBrainsMono-BoldItalic.ttf")[..],
    ]
    .into_iter()
    .map(Cow::Borrowed)
    .collect()
}

/// The iced theme: the app's colours where iced's widgets take their defaults from.
pub fn theme() -> Theme {
    Theme::custom(
        "Moebius",
        Palette {
            background: GROUND,
            text: TEXT,
            primary: TEXT,
            success: TEAL,
            warning: AMBER,
            danger: RED,
        },
    )
}
