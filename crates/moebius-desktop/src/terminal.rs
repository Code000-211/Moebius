//! The shell in a run's worktree: the pane of the live view that shows a terminal running on the server. The bytes
//! the server sends are parsed by `alacritty_terminal` into a grid of coloured cells, which is drawn here as rows of
//! JetBrains Mono on the dark background of the code blocks; what is typed goes back as bytes (`key_bytes`), and the
//! pane's size in cells goes back as a resize. Selecting, copying and pasting are not built, and nor is scrolling
//! back through what has gone off the top of the screen.

use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color as Ansi, Processor};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Widget, layout, mouse, overlay, renderer};
use iced::futures::channel::mpsc;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::widget::text::Wrapping;
use iced::widget::{column, container, rich_text, row, span, text};
use iced::{Border, Color, Element, Event, Length, Padding, Rectangle, Size, Vector, window};

use crate::client::{TerminalEvent, TerminalInput};
use crate::theme;

/// The height of the pane, which holds a dozen or so rows.
const HEIGHT: f32 = 250.0;
/// The padding around the grid.
const PADDING: f32 = 10.0;

/// Collects what the terminal asks the shell to be written, such as the answer to a cursor position request.
#[derive(Clone, Default)]
struct Replies(Arc<Mutex<Vec<u8>>>);

impl EventListener for Replies {
    fn send_event(&self, event: TermEvent) {
        if let TermEvent::PtyWrite(text) = event {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend(text.into_bytes());
        }
    }
}

struct Cells {
    cols: usize,
    rows: usize,
}

impl Dimensions for Cells {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// The screen of a terminal: what the shell has printed, as cells with colours, and where the cursor is.
pub struct Terminal {
    term: Term<Replies>,
    parser: Processor,
    replies: Replies,
}

impl Terminal {
    pub fn new(cols: u16, rows: u16) -> Self {
        let replies = Replies::default();
        let size = Cells {
            cols: cols.into(),
            rows: rows.into(),
        };
        Terminal {
            term: Term::new(Config::default(), &size, replies.clone()),
            parser: Processor::new(),
            replies,
        }
    }

    /// Takes bytes the shell printed. Returns what the terminal answers to them, to be sent back to the shell.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.parser.advance(&mut self.term, bytes);
        std::mem::take(&mut *self.replies.0.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(Cells {
            cols: cols.into(),
            rows: rows.into(),
        });
    }

    /// The size in columns and rows.
    pub fn size(&self) -> (u16, u16) {
        (self.term.columns() as u16, self.term.screen_lines() as u16)
    }

    /// The text of each row of the screen, without the spaces at the end.
    pub fn lines(&self) -> Vec<String> {
        (0..self.term.screen_lines())
            .map(|line| {
                let row = &self.term.grid()[Line(line as i32)];
                let text: String = (0..self.term.columns())
                    .map(|column| &row[Column(column)])
                    .filter(|cell| !cell.flags.contains(Flags::WIDE_CHAR_SPACER))
                    .map(|cell| cell.c)
                    .collect();
                text.trim_end().to_string()
            })
            .collect()
    }

    /// The rows as spans of one colour each, with the cursor drawn as a block when `cursor` says it is shown.
    fn rows<'a, M: 'a>(&self, cursor: bool) -> Vec<Element<'a, M>> {
        let at = self.term.grid().cursor.point;
        let cursor = cursor && self.term.mode().contains(TermMode::SHOW_CURSOR);
        (0..self.term.screen_lines())
            .map(|line| {
                let row = &self.term.grid()[Line(line as i32)];
                let mut runs: Vec<(String, Color, Option<Color>, bool)> = Vec::new();
                for column in 0..self.term.columns() {
                    let cell = &row[Column(column)];
                    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                        continue;
                    }
                    let (mut fg, mut bg) = (colour(cell.fg), colour(cell.bg));
                    let under_cursor = cursor && at.line.0 == line as i32 && at.column.0 == column;
                    if cell.flags.contains(Flags::INVERSE) != under_cursor {
                        std::mem::swap(&mut fg, &mut bg);
                    }
                    let bg = (bg != theme::TERMINAL).then_some(bg);
                    let underline = cell.flags.intersects(Flags::ALL_UNDERLINES);
                    let c = if cell.flags.contains(Flags::HIDDEN) {
                        ' '
                    } else {
                        cell.c
                    };
                    match runs.last_mut() {
                        Some((text, f, b, u)) if (*f, *b, *u) == (fg, bg, underline) => {
                            text.push(c)
                        }
                        _ => runs.push((c.to_string(), fg, bg, underline)),
                    }
                }
                let spans: Vec<_> = runs
                    .into_iter()
                    .map(|(text, fg, bg, underline)| {
                        let mut s = span(text).color(fg).underline(underline);
                        if let Some(bg) = bg {
                            s = s.background(bg);
                        }
                        s
                    })
                    .collect();
                rich_text::<(), M, _, _>(spans)
                    .font(theme::MONO)
                    .size(theme::SHELL)
                    .line_height(iced::Pixels(theme::SHELL_CELL[1]))
                    .wrapping(Wrapping::None)
                    .into()
            })
            .collect()
    }
}

/// An ANSI colour as it is drawn on the dark background.
fn colour(color: Ansi) -> Color {
    let ansi = |n: usize| theme::TERMINAL_ANSI[n];
    match color {
        Ansi::Spec(rgb) => Color::from_rgb8(rgb.r, rgb.g, rgb.b),
        Ansi::Indexed(n @ 0..16) => ansi(n.into()),
        Ansi::Indexed(n @ 16..232) => {
            let step = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            let n = n - 16;
            Color::from_rgb8(step(n / 36), step(n / 6 % 6), step(n % 6))
        }
        Ansi::Indexed(n) => {
            let grey = 8 + 10 * (n - 232);
            Color::from_rgb8(grey, grey, grey)
        }
        Ansi::Named(named) => match named as usize {
            n @ 0..16 => ansi(n),
            257 => theme::TERMINAL,
            n @ 259..=266 => Color {
                a: 0.6,
                ..ansi(n - 259)
            },
            268 => theme::TERMINAL_DIM,
            _ => theme::TERMINAL_TEXT,
        },
    }
}

/// The bytes a shell expects for a key press, or none when the key means nothing to it. `text` is what the key
/// produced as text; `app_cursor` is whether the program has asked for the arrow keys in application mode.
pub fn key_bytes(
    key: &Key,
    modifiers: Modifiers,
    text: Option<&str>,
    app_cursor: bool,
) -> Option<Vec<u8>> {
    let arrow = |letter: char| match app_cursor {
        true => format!("\x1bO{letter}"),
        false => format!("\x1b[{letter}"),
    };
    let named = match key {
        Key::Named(Named::Enter) => Some("\r".to_string()),
        Key::Named(Named::Backspace) => Some("\x7f".to_string()),
        Key::Named(Named::Tab) => Some(if modifiers.shift() { "\x1b[Z" } else { "\t" }.to_string()),
        Key::Named(Named::Escape) => Some("\x1b".to_string()),
        Key::Named(Named::Space) if !modifiers.control() => Some(" ".to_string()),
        Key::Named(Named::ArrowUp) => Some(arrow('A')),
        Key::Named(Named::ArrowDown) => Some(arrow('B')),
        Key::Named(Named::ArrowRight) => Some(arrow('C')),
        Key::Named(Named::ArrowLeft) => Some(arrow('D')),
        Key::Named(Named::Home) => Some(arrow('H')),
        Key::Named(Named::End) => Some(arrow('F')),
        Key::Named(Named::Delete) => Some("\x1b[3~".to_string()),
        Key::Named(Named::PageUp) => Some("\x1b[5~".to_string()),
        Key::Named(Named::PageDown) => Some("\x1b[6~".to_string()),
        _ => None,
    };
    if let Some(named) = named {
        return Some(named.into_bytes());
    }
    // Ctrl and a letter is the control character: Ctrl+C is 3, Ctrl+D is 4.
    if modifiers.control()
        && let Key::Character(c) = key
        && let Some(letter) = c.chars().next().filter(char::is_ascii_alphabetic)
    {
        return Some(vec![letter.to_ascii_lowercase() as u8 - b'a' + 1]);
    }
    if modifiers.control() || modifiers.logo() {
        return None;
    }
    let text = text.filter(|text| !text.is_empty() && !text.chars().any(char::is_control))?;
    // Alt sends the key with Escape before it.
    Some(
        match modifiers.alt() {
            true => format!("\x1b{text}"),
            false => text.to_string(),
        }
        .into_bytes(),
    )
}

/// The shell of a live view: the screen, the way to the server, and how it ended.
pub struct Shell {
    /// Tells this shell's connection from the one of an earlier shell, so that a new one reconnects.
    pub generation: u32,
    terminal: Terminal,
    input: Option<mpsc::Sender<TerminalInput>>,
    ended: Option<String>,
    /// The size the pane was last laid out at.
    pane: Size,
}

impl Shell {
    pub fn new(generation: u32) -> Self {
        let (cols, rows) = (80, 24);
        Shell {
            generation,
            terminal: Terminal::new(cols, rows),
            input: None,
            ended: None,
            pane: Size::ZERO,
        }
    }

    /// Whether what is typed reaches a shell.
    pub fn is_open(&self) -> bool {
        self.input.is_some()
    }

    pub fn on_event(&mut self, event: TerminalEvent) {
        match event {
            TerminalEvent::Connected(input) => {
                self.input = Some(input);
                let (cols, rows) = self.terminal.size();
                self.send(TerminalInput::Resize { cols, rows });
            }
            TerminalEvent::Output(bytes) => {
                let replies = self.terminal.feed(&bytes);
                if !replies.is_empty() {
                    self.send(TerminalInput::Bytes(replies));
                }
            }
            TerminalEvent::Closed(reason) => {
                self.input = None;
                self.ended = Some(reason);
            }
        }
    }

    /// Sends a key press to the shell.
    pub fn key(&mut self, key: &Key, modifiers: Modifiers, text: Option<&str>) {
        let app_cursor = self.terminal.term.mode().contains(TermMode::APP_CURSOR);
        if let Some(bytes) = key_bytes(key, modifiers, text, app_cursor) {
            self.send(TerminalInput::Bytes(bytes));
        }
    }

    /// The pane is `size` big: the shell gets as many columns and rows as fit.
    pub fn resized(&mut self, size: Size) {
        self.pane = size;
        let [width, height] = theme::SHELL_CELL;
        let cols = ((size.width - 2.0 * PADDING) / width).floor().max(1.0) as u16;
        let rows = ((size.height - 2.0 * PADDING) / height).floor().max(1.0) as u16;
        if (cols, rows) != self.terminal.size() {
            self.terminal.resize(cols, rows);
            self.send(TerminalInput::Resize { cols, rows });
        }
    }

    fn send(&mut self, input: TerminalInput) {
        if let Some(sender) = &mut self.input
            && sender.try_send(input).is_err()
        {
            self.input = None;
        }
    }

    /// The text of the screen, row by row.
    pub fn lines(&self) -> Vec<String> {
        self.terminal.lines()
    }

    /// The pane: a header line over the screen. `resized` is told the size the pane is laid out at, when it changes.
    pub fn view<'a, M: 'a>(&'a self, resized: impl Fn(Size) -> M + 'a) -> Element<'a, M> {
        let state = match (&self.ended, self.input.is_some()) {
            (Some(reason), _) => reason.as_str(),
            (None, true) => "connected",
            (None, false) => "connecting",
        };
        let header = container(
            row![
                text("Shell in the worktree")
                    .size(theme::LABEL)
                    .font(theme::manrope(iced::font::Weight::Bold))
                    .color(theme::TERMINAL_TEXT),
                iced::widget::space::horizontal(),
                text(state).size(theme::LABEL).color(theme::TERMINAL_DIM),
            ]
            .spacing(8),
        )
        .padding([6, 10]);
        let screen = container(column(self.terminal.rows(self.is_open())))
            .padding(PADDING)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true);
        container(Measure::new(column![header, screen], self.pane, resized))
            .height(HEIGHT)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(theme::TERMINAL.into()),
                border: Border {
                    radius: theme::RADIUS_CARD.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .padding(Padding::ZERO)
            .into()
    }
}

/// Tells what size its content is laid out at, whenever that differs from `known`: how the pane learns how many
/// columns and rows fit, which a view cannot know.
struct Measure<'a, M> {
    content: Element<'a, M>,
    known: Size,
    on_resize: Box<dyn Fn(Size) -> M + 'a>,
}

impl<'a, M> Measure<'a, M> {
    fn new(
        content: impl Into<Element<'a, M>>,
        known: Size,
        on_resize: impl Fn(Size) -> M + 'a,
    ) -> Self {
        Measure {
            content: content.into(),
            known,
            on_resize: Box::new(on_resize),
        }
    }
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Measure<'_, M> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut iced::advanced::Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        if matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && layout.bounds().size() != self.known
        {
            shell.publish((self.on_resize)(layout.bounds().size()));
        }
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, iced::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

impl<'a, M: 'a> From<Measure<'a, M>> for Element<'a, M> {
    fn from(measure: Measure<'a, M>) -> Self {
        Element::new(measure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressed(key: Key, modifiers: Modifiers, text: Option<&str>) -> Option<Vec<u8>> {
        key_bytes(&key, modifiers, text, false)
    }

    fn character(c: &str) -> Key {
        Key::Character(c.into())
    }

    #[test]
    fn bytes_with_colours_and_a_cursor_move_make_the_grid() {
        let mut terminal = Terminal::new(20, 4);
        terminal.feed(b"hello \x1b[31mred\x1b[0m\r\nsecond");
        assert_eq!(terminal.lines(), ["hello red", "second", "", ""]);
        let cell = |column| terminal.term.grid()[Line(0)][Column(column)].clone();
        assert_eq!(colour(cell(6).fg), theme::TERMINAL_ANSI[1]);
        assert_eq!(colour(cell(0).fg), theme::TERMINAL_TEXT);

        // Back two columns and over the middle letter; then a clear of the line.
        terminal.feed(b"\x1b[2D\x1b[1;38;2;1;2;3mX");
        assert_eq!(terminal.lines()[1], "secoXd");
        let cell = terminal.term.grid()[Line(1)][Column(4)].clone();
        assert_eq!(colour(cell.fg), Color::from_rgb8(1, 2, 3));
        terminal.feed(b"\x1b[2K");
        assert_eq!(terminal.lines()[1], "");
    }

    #[test]
    fn output_scrolls_and_a_resize_keeps_what_fits() {
        let mut terminal = Terminal::new(10, 3);
        terminal.feed(b"one\r\ntwo\r\nthree\r\nfour");
        assert_eq!(terminal.lines(), ["two", "three", "four"]);
        terminal.resize(5, 2);
        assert_eq!(terminal.size(), (5, 2));
        assert_eq!(terminal.lines().len(), 2);
    }

    #[test]
    fn what_the_terminal_is_asked_it_answers_to_the_shell() {
        let mut terminal = Terminal::new(10, 3);
        let reply = terminal.feed(b"ab\x1b[6n");
        assert_eq!(reply, b"\x1b[1;3R");
    }

    #[test]
    fn key_presses_are_the_bytes_a_shell_expects() {
        let none = Modifiers::empty();
        assert_eq!(pressed(character("a"), none, Some("a")).unwrap(), b"a");
        assert_eq!(
            pressed(character("A"), Modifiers::SHIFT, Some("A")).unwrap(),
            b"A"
        );
        assert_eq!(
            pressed(Key::Named(Named::Enter), none, Some("\r")).unwrap(),
            b"\r"
        );
        assert_eq!(
            pressed(Key::Named(Named::Backspace), none, None).unwrap(),
            b"\x7f"
        );
        assert_eq!(pressed(Key::Named(Named::Tab), none, None).unwrap(), b"\t");
        assert_eq!(
            pressed(Key::Named(Named::Escape), none, None).unwrap(),
            b"\x1b"
        );
        assert_eq!(
            pressed(Key::Named(Named::Space), none, Some(" ")).unwrap(),
            b" "
        );
        assert_eq!(
            pressed(Key::Named(Named::ArrowUp), none, None).unwrap(),
            b"\x1b[A"
        );
        assert_eq!(
            pressed(Key::Named(Named::ArrowLeft), none, None).unwrap(),
            b"\x1b[D"
        );
        assert_eq!(
            pressed(character("c"), Modifiers::CTRL, Some("c")).unwrap(),
            [3]
        );
        assert_eq!(pressed(character("d"), Modifiers::CTRL, None).unwrap(), [4]);
        assert_eq!(
            pressed(character("x"), Modifiers::ALT, Some("x")).unwrap(),
            b"\x1bx"
        );
        assert_eq!(
            pressed(character("é"), none, Some("é")).unwrap(),
            "é".as_bytes()
        );
        assert_eq!(
            pressed(Key::Named(Named::Shift), Modifiers::SHIFT, None),
            None
        );
        assert_eq!(pressed(character("c"), Modifiers::LOGO, Some("c")), None);
        // A program that asked for application mode gets the arrows that way.
        let up = key_bytes(&Key::Named(Named::ArrowUp), none, None, true);
        assert_eq!(up.unwrap(), b"\x1bOA");
    }

    #[test]
    fn the_shell_sends_what_is_typed_and_the_size_that_fits_the_pane() {
        let (input, mut sent) = mpsc::channel(8);
        let mut shell = Shell::new(1);
        assert!(!shell.is_open());
        shell.on_event(TerminalEvent::Connected(input));
        assert!(shell.is_open());
        assert_eq!(
            sent.try_recv().unwrap(),
            TerminalInput::Resize { cols: 80, rows: 24 }
        );

        shell.resized(Size::new(805.0, 260.0));
        assert_eq!(
            sent.try_recv().unwrap(),
            TerminalInput::Resize {
                cols: 100,
                rows: 13
            }
        );
        shell.resized(Size::new(805.0, 260.0));
        assert!(sent.try_recv().is_err(), "the same size is not sent again");

        shell.key(&character("l"), Modifiers::empty(), Some("l"));
        assert_eq!(
            sent.try_recv().unwrap(),
            TerminalInput::Bytes(b"l".to_vec())
        );

        shell.on_event(TerminalEvent::Output(b"hi\x1b[6n".to_vec()));
        assert_eq!(shell.lines()[0], "hi");
        assert_eq!(
            sent.try_recv().unwrap(),
            TerminalInput::Bytes(b"\x1b[1;3R".to_vec())
        );

        shell.on_event(TerminalEvent::Closed("the shell ended".into()));
        assert!(!shell.is_open());
    }
}
