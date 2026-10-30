//! The shell in a run's worktree: the pane of the live view that shows a terminal running on the server, and the same
//! pane as big as the Terminal page. The bytes
//! the server sends are parsed by `alacritty_terminal` into a grid of coloured cells, which is drawn here as rows of
//! JetBrains Mono on the dark background of the code blocks; what is typed goes back as bytes (`key_bytes`), and the
//! pane's size in cells goes back as a resize. Scrolling back through what has gone off the top of the screen is not
//! built.
//!
//! `keys` holds the bytes of the keys, `pane` the pane around the screen, which selects, copies and pastes.

use std::sync::{Arc, Mutex};

mod keys;
mod pane;

pub use keys::key_bytes;
use pane::Pane;
pub use pane::PaneEvent;

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color as Ansi, Processor};
use iced::futures::channel::mpsc;
use iced::keyboard::{Key, Modifiers};
use iced::widget::text::Wrapping;
use iced::widget::{column, container, rich_text, row, span, text};
use iced::{Border, Color, Element, Length, Padding, Size};

use crate::client::{TerminalEvent, TerminalInput};
use crate::theme;

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

/// The shell of a live view: the screen, the way to the server, and how it ended.
pub struct Shell {
    /// Tells this shell's connection from the one of an earlier shell, so that a new one reconnects.
    pub generation: u32,
    terminal: Terminal,
    input: Option<mpsc::Sender<TerminalInput>>,
    ended: Option<String>,
    /// The size the pane was last laid out at.
    pane: Size,
    /// Whether keys go to the shell: from when it is opened until a click lands outside the pane.
    focused: bool,
    /// What the pane's header says, and whether the pane fills the height it is given instead of having its own.
    title: &'static str,
    fills: bool,
}

impl Shell {
    /// A shell in a run's worktree, in the pane under the conversation.
    pub fn new(generation: u32) -> Self {
        Self::pane("Shell in the worktree", false, generation)
    }

    /// A shell on the server, in a pane as big as its page.
    pub fn on_server(generation: u32) -> Self {
        Self::pane("Shell on the server", true, generation)
    }

    /// A shell in a repository's clone on the server, in a pane as big as a project's tab.
    pub fn in_clone(generation: u32) -> Self {
        Self::pane("Shell in the clone", true, generation)
    }

    /// Pi-Bolt in a chat's worktree, in a pane as big as the chat's detail.
    pub fn in_chat(generation: u32) -> Self {
        Self::pane("Pi-Bolt in the chat's worktree", true, generation)
    }

    fn pane(title: &'static str, fills: bool, generation: u32) -> Self {
        let (cols, rows) = (80, 24);
        Shell {
            generation,
            terminal: Terminal::new(cols, rows),
            input: None,
            ended: None,
            pane: Size::ZERO,
            focused: true,
            title,
            fills,
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

    /// Whether keys go to the shell.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Sends a key press to the shell, if the pane has focus.
    pub fn key(&mut self, key: &Key, modifiers: Modifiers, text: Option<&str>) {
        if !self.focused {
            return;
        }
        let app_cursor = self.terminal.term.mode().contains(TermMode::APP_CURSOR);
        if let Some(bytes) = key_bytes(key, modifiers, text, app_cursor) {
            self.send(TerminalInput::Bytes(bytes));
        }
    }

    /// What the pane told: its size, a click that gave or took its focus, or text to paste.
    pub fn on_pane(&mut self, event: PaneEvent) {
        match event {
            PaneEvent::Resized(size) => self.resized(size),
            PaneEvent::Focus(focused) => self.set_focus(focused),
            PaneEvent::Paste(text) => self.paste(&text),
        }
    }

    /// Sends `text` to the shell as if typed, between the marks of a paste when the program asked for them, so that
    /// a pasted line ending does not run it.
    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        let bytes = match self
            .terminal
            .term
            .mode()
            .contains(TermMode::BRACKETED_PASTE)
        {
            true => format!("\x1b[200~{text}\x1b[201~"),
            false => text,
        };
        self.send(TerminalInput::Bytes(bytes.into_bytes()));
    }

    /// The pane is `size` big: the shell gets as many columns and rows as fit.
    pub fn resized(&mut self, size: Size) {
        self.pane = size;
        let [width, height] = theme::SHELL_CELL;
        let cols = ((size.width - 2.0 * theme::SPACE_10) / width)
            .floor()
            .max(1.0) as u16;
        let rows = ((size.height - 2.0 * theme::SPACE_10) / height)
            .floor()
            .max(1.0) as u16;
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

    /// The pane: a header line over the screen, with a light border while it has focus. `on_event` is told what the pane
    /// learns of its size and of clicks.
    pub fn view<'a, M: 'a>(&'a self, on_event: impl Fn(PaneEvent) -> M + 'a) -> Element<'a, M> {
        let state = match (&self.ended, self.input.is_some()) {
            (Some(reason), _) => reason.as_str(),
            (None, true) => "connected",
            (None, false) => "connecting",
        };
        let header = container(
            row![
                text(self.title)
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TERMINAL_TEXT),
                iced::widget::space::horizontal(),
                text(state)
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TERMINAL_DIM),
            ]
            .spacing(theme::SPACE_8),
        )
        .padding([theme::SPACE_6, theme::SPACE_10]);
        let screen = container(column(self.terminal.rows(self.is_open())))
            .padding(theme::SPACE_10)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true);
        let focused = self.focused;
        container(Pane::new(
            column![header, screen],
            self.pane,
            self.focused,
            self.terminal.lines(),
            on_event,
        ))
        .height(if self.fills {
            Length::Fill
        } else {
            Length::Fixed(theme::TERMINAL_HEIGHT)
        })
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(theme::TERMINAL.into()),
            border: Border {
                radius: theme::RADIUS_CARD.into(),
                color: if focused {
                    theme::TERMINAL_TEXT
                } else {
                    theme::TERMINAL
                },
                width: 1.0,
            },
            ..container::Style::default()
        })
        .padding(Padding::ZERO)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                cols: 104,
                rows: 14
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

        // A paste is typed as it is, its line endings as Enter, between the marks of a paste once the program asks.
        shell.on_pane(PaneEvent::Paste("ls\nmake".into()));
        assert_eq!(
            sent.try_recv().unwrap(),
            TerminalInput::Bytes(b"ls\rmake".to_vec())
        );
        shell.on_event(TerminalEvent::Output(b"\x1b[?2004h".to_vec()));
        shell.on_pane(PaneEvent::Paste("rm -rf build\n".into()));
        assert_eq!(
            sent.try_recv().unwrap(),
            TerminalInput::Bytes(b"\x1b[200~rm -rf build\r\x1b[201~".to_vec())
        );

        shell.on_event(TerminalEvent::Closed("the shell ended".into()));
        assert!(!shell.is_open());
    }
}
