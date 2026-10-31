//! Every colour, size, radius and spacing of `docs/design.md` (and of the agreed mockups in `docs/design/`) as named
//! constants, and the fonts. No other module has a colour literal.

use std::borrow::Cow;

use iced::font::{Family, Weight};
use iced::theme::Palette;
use iced::{Color, Font, Theme};

const fn hex(rgb: u32) -> Color {
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// The C3 Deep Velvet base: ground, islands, wells, cards, lines and text.
pub const GROUND: Color = hex(0x0F0D14);
pub const SURFACE: Color = hex(0x181520);
pub const LANE: Color = hex(0x120F18);
pub const CARD: Color = hex(0x211C2B);
pub const CARD_HOVER: Color = hex(0x2A2437);
pub const RECESSED: Color = hex(0x0B0910);
pub const LINE: Color = hex(0x2A2438);
pub const LINE_STRONG: Color = hex(0x3B334D);
pub const TEXT: Color = hex(0xF7F4FC);
pub const TEXT_SECONDARY: Color = hex(0xAAA0BA);
pub const TEXT_MUTED: Color = hex(0x685E78);

/// Primary action and selection accent: Coral (`#F27663`).
pub const ACCENT: Color = hex(0xF27663);
pub const ACCENT_FILL: Color = hex(0x2B1A20);
pub const ACCENT_BORDER: Color = hex(0x7A3B36);

/// Active live runs and working progress: Lavender (`#C084FC`).
pub const LIVE: Color = hex(0xC084FC);
pub const LIVE_FILL: Color = hex(0x241934);
pub const LIVE_BORDER: Color = hex(0x5B3E7A);

/// Runs on its own / passing checks: Mint (`#6EE7B7`).
pub const TEAL: Color = hex(0x6EE7B7);
pub const TEAL_FILL: Color = hex(0x132624);

/// Needs you: ready to merge, the inbox count, warnings.
pub const AMBER: Color = hex(0xFBBF24);
pub const AMBER_FILL: Color = hex(0x292014);

/// Blocked: a broken rule, a stuck or blocked card; only these.
pub const RED: Color = hex(0xFB7185);
pub const RED_TEXT: Color = hex(0xFB7185);
pub const RED_FILL: Color = hex(0x2A1520);

/// The colours of the tags of a card: border, fill and text, one set for each of the roles.
pub const TEAL_BORDER: Color = hex(0x2B594B);
pub const AMBER_BORDER: Color = hex(0x664D14);
pub const AMBER_TEXT: Color = hex(0xFBBF24);
pub const RED_BORDER: Color = hex(0x6E2837);
pub const RED_TAG_FILL: Color = hex(0x2A1520);

/// The part of a progress bar that is not done.
pub const TRACK: Color = hex(0x262033);

/// The validated categorical order of the charts (Coral, Mint, Lavender, Amber).
pub const CHART: [Color; 4] = [hex(0xF27663), hex(0x6EE7B7), hex(0xC084FC), hex(0xFBBF24)];

const MODEL_COLOURS: [Color; 6] = [
    hex(0xF27663),
    hex(0x6EE7B7),
    hex(0xC084FC),
    hex(0xFBBF24),
    hex(0x38BDF8),
    hex(0xF472B6),
];
const SHADE_WEIGHT: [f32; 4] = [0.32, 0.54, 0.78, 1.0];
/// A heatmap cell without usage.
pub const HEAT_EMPTY: Color = hex(0x211C2B);

/// The colour of the model that came `index`-th in the order of appearance.
pub fn model_colour(index: usize) -> Color {
    MODEL_COLOURS[index % MODEL_COLOURS.len()]
}

/// A shade of `color` for a heatmap cell against `HEAT_EMPTY`, `level` 1 (dimmest) to 4 (the colour itself).
pub fn shade(color: Color, level: usize) -> Color {
    let w = SHADE_WEIGHT[level.clamp(1, 4) - 1];
    Color::from_rgb(
        HEAT_EMPTY.r + (color.r - HEAT_EMPTY.r) * w,
        HEAT_EMPTY.g + (color.g - HEAT_EMPTY.g) * w,
        HEAT_EMPTY.b + (color.b - HEAT_EMPTY.b) * w,
    )
}

/// The pill of a tool's name, and the fills of a diff, with the marks of its rows.
pub const HEAD_PILL: Color = hex(0x262033);
pub const DIFF_REMOVED: Color = hex(0x2A1520);
pub const DIFF_ADDED: Color = hex(0x132624);
pub const DIFF_REMOVED_MARK: Color = hex(0xFB7185);
pub const DIFF_ADDED_MARK: Color = hex(0x6EE7B7);

/// The box of an agent's steps: fill, border and the ink of its header.
pub const STEPS_FILL: Color = hex(0x120F18);
pub const STEPS_BORDER: Color = hex(0x2A2438);
pub const STEPS_INK: Color = hex(0xF7F4FC);

/// The dark block that shows what a tool printed.
pub const TERMINAL: Color = hex(0x07060A);
pub const TERMINAL_TEXT: Color = hex(0xEAE4F2);
pub const TERMINAL_DIM: Color = hex(0x685E78);
/// The wash over what is selected in a terminal.
pub const TERMINAL_SELECTION: Color = Color {
    a: 0.25,
    ..TERMINAL_TEXT
};

/// The sixteen colours of a shell: black to white, then the bright ones.
pub const TERMINAL_ANSI: [Color; 16] = [
    hex(0x2A2438),
    hex(0xFB7185),
    hex(0x6EE7B7),
    hex(0xFBBF24),
    hex(0x60A5FA),
    hex(0xC084FC),
    hex(0x22D3EE),
    hex(0xEAE4F2),
    hex(0x685E78),
    hex(0xFDA4AF),
    hex(0xA7F3D0),
    hex(0xFDE047),
    hex(0x93C5FD),
    hex(0xD8B4FE),
    hex(0x67E8F9),
    hex(0xF7F4FC),
];

// The sidebar island (`#181520`).
pub const SIDEBAR: Color = hex(0x181520);
pub const SIDEBAR_LINE: Color = hex(0x2A2438);
pub const SIDEBAR_TEXT: Color = hex(0xAAA0BA);
pub const SIDEBAR_GROUP: Color = hex(0x685E78);
pub const SIDEBAR_CURRENT: Color = hex(0x2B1A20);
pub const SIDEBAR_HOVER: Color = hex(0x211C2B);
pub const ONLINE_TEXT: Color = hex(0x6EE7B7);
pub const ON_FILL: Color = hex(0x0F0D14);
/// The shadow under what floats over the page: the floating chat's button and window.
pub const FLOAT_SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.42);

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

// Soft 14px island radii.
pub const RADIUS_BOX: f32 = 14.0;
pub const RADIUS_CARD: f32 = 10.0;
pub const RADIUS_CONTROL: f32 = 8.0;
pub const RADIUS_ITEM: f32 = 8.0;
pub const RADIUS_SMALL: f32 = 6.0;
pub const RADIUS_SEGMENT: f32 = 2.0;
pub const RADIUS_ROUND: f32 = 999.0;

// Spacing and fixed widths.
pub const SIDEBAR_WIDTH: f32 = 220.0;
pub const RAIL_WIDTH: f32 = 56.0;
pub const PAGE_PADDING: [f32; 2] = [14.0, 18.0];
pub const PAGE_GAP: f32 = 12.0;
/// The padding of a card of a list and of a small tile.
pub const CARD_PADDING: [f32; 2] = [9.0, 11.0];
pub const TILE_PADDING: [f32; 2] = [8.0, 10.0];
/// The room a scrolling page leaves on its right for the scrollbar.
pub const SCROLL_GUTTER: f32 = 12.0;
/// The cells of the usage heatmap, their gap, and the column that names the weekdays.
pub const HEAT_CELL: f32 = 14.0;
pub const HEAT_GAP: f32 = 2.0;
pub const HEAT_LABELS: f32 = 40.0;
/// The thickness of a bar of the usage page, the square before a model's name, and the columns around a bar.
pub const USAGE_BAR: f32 = 12.0;
pub const SWATCH: f32 = 10.0;
pub const ACTIVITY_NAME_WIDTH: f32 = 220.0;
pub const ACTIVITY_TIME_WIDTH: f32 = 90.0;
/// The height of a tall text editor, such as a prompt.
pub const EDITOR_TALL: f32 = 140.0;
/// The columns of a firing's line and the tag column of a setup step.
pub const FIRING_TIME_WIDTH: f32 = 90.0;
pub const FIRING_BY_WIDTH: f32 = 80.0;
pub const STEP_TAG_WIDTH: f32 = 64.0;
/// A thin bar, and the mark across it.
pub const BAR_HEIGHT: f32 = 6.0;
pub const BAR_MARK: f32 = 2.0;
/// The width of a short field, such as a number.
pub const FIELD_WIDTH: f32 = 220.0;
/// The height of a small text editor in a form.
pub const EDITOR_COMPACT: f32 = 96.0;
/// The narrow list beside the editor of a page.
pub const LIST_WIDTH_NARROW: f32 = 240.0;
pub const ICON: f32 = 16.0;
pub const ICON_SMALL: f32 = 14.0;
/// The Moebius mark.
pub const LOGO_WIDTH: f32 = 22.0;
pub const LOGO_HEIGHT: f32 = 16.0;
pub const DOT: f32 = 6.0;

/// The spacing scale for gaps and paddings, named by pixels.
pub const SPACE_1: f32 = 1.0;
pub const SPACE_2: f32 = 2.0;
pub const SPACE_4: f32 = 4.0;
pub const SPACE_6: f32 = 6.0;
pub const SPACE_8: f32 = 8.0;
pub const SPACE_10: f32 = 10.0;
pub const SPACE_12: f32 = 12.0;
pub const SPACE_14: f32 = 14.0;
pub const SPACE_16: f32 = 16.0;
pub const SPACE_18: f32 = 18.0;
pub const SPACE_20: f32 = 20.0;
pub const SPACE_24: f32 = 24.0;
/// The padding of a text field, of a compact button and of a box.
pub const FIELD_PADDING: [f32; 2] = [6.0, 10.0];
pub const CONTROL_PADDING: [f32; 2] = [6.0, 12.0];
pub const BOX_PADDING: f32 = 14.0;
/// The thickness of a line and of a progress bar, and the side of a small square button.
pub const HAIRLINE: f32 = 1.0;
pub const BAR: f32 = 4.0;
pub const SQUARE_BUTTON: f32 = 26.0;

/// The widest a line of prose is set, the narrower width of a message, and the width of the side panels.
pub const PROSE_WIDTH: f32 = 760.0;
pub const MESSAGE_WIDTH: f32 = 600.0;
pub const PANEL_WIDTH: f32 = 300.0;
pub const PROSE_LINE_HEIGHT: f32 = 1.5;

/// The width of the marker column of a line of code.
pub const MARKER_COLUMN: f32 = 24.0;
/// The tallest the block of a tool's output grows before it scrolls.
pub const OUTPUT_MAX_HEIGHT: f32 = 240.0;
/// The line height of code.
pub const CODE_LINE_HEIGHT: f32 = 1.5;

/// The heights of a text editor in a form: a short one and a taller one.
pub const EDITOR_HEIGHT_SMALL: f32 = 120.0;
pub const EDITOR_HEIGHT: f32 = 160.0;

/// The width of the list of chats.
pub const LIST_WIDTH: f32 = 280.0;

/// The width of the time column of a card's history.
pub const TIME_COLUMN: f32 = 90.0;

/// The width of a tile of the fleet page, the height of an agent's steps box and of the terminal block.
pub const TILE_WIDTH: f32 = 340.0;
pub const STEPS_MAX_HEIGHT: f32 = 380.0;
pub const TERMINAL_HEIGHT: f32 = 250.0;

/// How far each level of a file tree is indented.
pub const TREE_INDENT: f32 = 16.0;
/// The width of the Files sidebar on the right of a project's page.
pub const FILES_WIDTH: f32 = 250.0;
/// The width of a menu that drops down from a button.
pub const DROPDOWN_WIDTH: f32 = 200.0;

/// The width of the list that opens over the page when searching.
pub const SEARCH_WIDTH: f32 = 380.0;
/// How far under the top of the window that list starts, below the sidebar's search field.
pub const SEARCH_TOP: f32 = 100.0;

/// The shadow under what floats: soft and deep for dark islands.
pub fn shadow() -> iced::Shadow {
    iced::Shadow {
        color: FLOAT_SHADOW,
        offset: iced::Vector::new(0.0, 6.0),
        blur_radius: 20.0,
    }
}
/// The padding of a small button.
pub const SMALL_BUTTON_PADDING: [f32; 2] = [3.0, 8.0];
/// The width of the border that marks what is being edited.
pub const BORDER_FOCUS: f32 = 1.5;
/// How far an input beside labelled ones is moved down to line up with them: the label's line and its gap.
pub const FIELD_LABEL_OFFSET: f32 = 18.0;
/// The padding inside a text editor, and the heights of the short, the large and the full one.
pub const EDITOR_PADDING: f32 = 10.0;
pub const EDITOR_SHORT: u32 = 120;
pub const EDITOR_LARGE: u32 = 220;
pub const EDITOR_FULL: u32 = 360;
/// The padding around the small pencil of editing in place.
pub const PENCIL_PADDING: f32 = 2.0;
/// Fixed widths: the labels of a table of facts, the input for a file's name, and the card of the connect screen.
pub const FACT_LABEL_WIDTH: f32 = 170.0;
pub const NAME_INPUT_WIDTH: f32 = 360.0;
pub const CONNECT_WIDTH: f32 = 440.0;
pub const CONNECT_PADDING: f32 = 28.0;
/// The tallest the list of SSH hosts grows on the connect screen before it scrolls.
pub const HOST_LIST_HEIGHT: f32 = 220.0;

/// The floating chat: the round button, the window it grows into, and their distance from the window's corner.
pub const FLOAT_BUTTON: f32 = 40.0;
pub const FLOAT_WIDTH: f32 = 400.0;
pub const FLOAT_HEIGHT: f32 = 560.0;
pub const FLOAT_MARGIN: f32 = 16.0;

/// Zero-Scroll bottom inspector dock height (`ZS-1`) on the board.
pub const DOCK_HEIGHT: f32 = 148.0;
pub const DOCK_META_WIDTH: f32 = 270.0;

/// The chat's open canvas (`CH3-D`): the column its top bar, turn cards and composer share, the radius of the composer
/// island, and the width of the list of chats that drops down from the top bar.
pub const CHAT_COLUMN: f32 = 780.0;
pub const RADIUS_COMPOSER: f32 = 18.0;
pub const CHAT_LIST_WIDTH: f32 = 320.0;
pub const CHAT_LIST_HEIGHT: f32 = 440.0;

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
        "Moebius Deep Velvet",
        Palette {
            background: GROUND,
            text: TEXT,
            primary: ACCENT,
            success: TEAL,
            warning: AMBER,
            danger: RED,
        },
    )
}

/// The theme the code of editors, diffs and code blocks is highlighted with: a dark one, for the dark ground.
pub const HIGHLIGHT: iced::highlighter::Theme = iced::highlighter::Theme::Base16Ocean;
