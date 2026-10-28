//! Every colour, size, radius and spacing of `docs/design.md` (and of the agreed mockups in `docs/design/`) as named
//! constants, and the fonts. No other module has a colour literal.

use std::borrow::Cow;

use iced::font::{Family, Weight};
use iced::theme::Palette;
use iced::{Color, Font, Theme};

const fn hex(rgb: u32) -> Color {
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// Colour: the table of `docs/design.md`.
pub const GROUND: Color = hex(0xEDF2F7);
pub const SURFACE: Color = hex(0xFFFFFF);
pub const LANE: Color = hex(0xF7F9FC);
pub const LINE: Color = hex(0xCBD5E1);
pub const TEXT: Color = hex(0x0F172A);
pub const TEXT_SECONDARY: Color = hex(0x475569);
pub const TEXT_MUTED: Color = hex(0x64748B);
/// Runs on its own: live runs, passing checks, progress.
pub const TEAL: Color = hex(0x0F766E);
pub const TEAL_FILL: Color = hex(0xE6F5F3);
/// Needs you: ready to merge, the inbox count, the primary button.
pub const AMBER: Color = hex(0xB45309);
pub const AMBER_FILL: Color = hex(0xFDF3E6);
/// Blocked: a broken rule, a stuck or blocked card; only these.
pub const RED: Color = hex(0xD92D20);
pub const RED_TEXT: Color = hex(0x9B2C22);
pub const RED_FILL: Color = hex(0xFEF3F2);
/// The colours of the tags of a card: border, fill and text, one set for each of the three roles.
pub const TEAL_BORDER: Color = hex(0x99D5CD);
pub const AMBER_BORDER: Color = hex(0xF2C793);
pub const AMBER_TEXT: Color = hex(0x92400E);
pub const RED_BORDER: Color = hex(0xE5786B);
pub const RED_TAG_FILL: Color = hex(0xFCEDEB);
/// The part of a progress bar that is not done.
pub const TRACK: Color = hex(0xE2E8F0);
/// The validated categorical order of the charts.
pub const CHART: [Color; 4] = [hex(0x2A78D6), hex(0xEB6834), hex(0x1BAF7A), hex(0xEDA100)];

/// The colours of the models on the usage screen: the chart order, then two more. A model takes the colour at its
/// place in the order of appearance, wrapping round after the sixth.
const MODEL_COLOURS: [Color; 6] = [
    hex(0x2A78D6),
    hex(0xEB6834),
    hex(0x1BAF7A),
    hex(0xEDA100),
    hex(0x8B5CF6),
    hex(0xD6409F),
];
/// How far each of the four shades of a heatmap cell is mixed with white, from the fewest tokens to the most.
const SHADE_WHITE: [f32; 4] = [0.65, 0.42, 0.2, 0.0];
/// A heatmap cell without usage.
pub const HEAT_EMPTY: Color = hex(0xEEF2F7);

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
pub const HEAD_PILL: Color = hex(0xF1F5F9);
pub const DIFF_REMOVED: Color = hex(0xFCEDEB);
pub const DIFF_ADDED: Color = hex(0xE6F6EC);
pub const DIFF_REMOVED_MARK: Color = hex(0xB42318);
pub const DIFF_ADDED_MARK: Color = hex(0x15803D);
/// The dark block that shows what a tool printed.
pub const TERMINAL: Color = hex(0x1E293B);
pub const TERMINAL_TEXT: Color = hex(0xE2E8F0);
pub const TERMINAL_DIM: Color = hex(0x94A3B8);
/// The sixteen colours of a shell: black to white, then the bright ones.
pub const TERMINAL_ANSI: [Color; 16] = [
    hex(0x475569),
    hex(0xF87171),
    hex(0x4ADE80),
    hex(0xFACC15),
    hex(0x60A5FA),
    hex(0xC084FC),
    hex(0x22D3EE),
    hex(0xE2E8F0),
    hex(0x64748B),
    hex(0xFCA5A5),
    hex(0x86EFAC),
    hex(0xFDE047),
    hex(0x93C5FD),
    hex(0xD8B4FE),
    hex(0x67E8F9),
    hex(0xF8FAFC),
];

// The sidebar of the mockups.
pub const SIDEBAR: Color = hex(0xF7F8FA);
pub const SIDEBAR_LINE: Color = hex(0xE2E6EC);
pub const SIDEBAR_TEXT: Color = hex(0x3A424D);
pub const SIDEBAR_GROUP: Color = hex(0x6B7380);
pub const SIDEBAR_CURRENT: Color = hex(0xE7EBF0);
pub const SIDEBAR_HOVER: Color = hex(0xEEF1F5);
pub const ONLINE_TEXT: Color = hex(0x0F5E57);
pub const ON_FILL: Color = SURFACE;

// Type sizes.
pub const PAGE_TITLE: f32 = 24.0;
pub const BOX_HEADING: f32 = 16.0;
pub const CARD_TITLE: f32 = 15.0;
pub const BODY: f32 = 14.5;
pub const CONTEXT: f32 = 14.0;
pub const BREADCRUMB: f32 = 13.0;
pub const LABEL: f32 = 12.0;
/// The big number of a tile.
pub const TILE_VALUE: f32 = 28.0;
/// The number above a card, a column's script and its count.
pub const SMALL: f32 = 11.5;
pub const COLUMN_NAME: f32 = 14.5;
pub const APP_NAME: f32 = 16.5;
pub const GROUP_LABEL: f32 = 12.5;
pub const CODE: f32 = 12.5;
pub const CODE_SMALL: f32 = 12.0;
/// The shell's text, and the width and height of one of its cells (JetBrains Mono is 0.6 em wide).
pub const SHELL: f32 = 13.0;
pub const SHELL_CELL: [f32; 2] = [SHELL * 0.6, 18.0];

// Shape.
pub const RADIUS_BOX: f32 = 16.0;
pub const RADIUS_CARD: f32 = 12.0;
pub const RADIUS_CONTROL: f32 = 10.0;
pub const RADIUS_ITEM: f32 = 9.0;
pub const RADIUS_SMALL: f32 = 7.0;
pub const RADIUS_SEGMENT: f32 = 2.0;
pub const RADIUS_ROUND: f32 = 999.0;

// Spacing and layout.
pub const SIDEBAR_WIDTH: f32 = 248.0;
pub const RAIL_WIDTH: f32 = 68.0;
pub const PAGE_PADDING: [f32; 2] = [20.0, 24.0];
pub const PAGE_GAP: f32 = 18.0;
pub const ICON: f32 = 16.0;
pub const DOT: f32 = 8.0;

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
            primary: AMBER,
            success: TEAL,
            warning: AMBER,
            danger: RED,
        },
    )
}
