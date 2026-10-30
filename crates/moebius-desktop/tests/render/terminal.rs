//! The Terminal page: a shell on the server as big as the page, connected while the page is open.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, TerminalEvent};
use moebius_desktop::screens::server_terminal;
use moebius_desktop::widgets::terminal::PaneEvent;

use crate::fixtures::{draw, key, overview, simulator};

/// The app on the Terminal page with its shell connected and sized, and `sent` the channel to the server.
fn terminal_page() -> (
    App,
    iced::futures::channel::mpsc::Receiver<client::TerminalInput>,
) {
    let mut app = App::with_snapshot(overview());
    let (input, sent) = iced::futures::channel::mpsc::channel(16);
    let _ = app.update(Message::Select(Page::Terminal));
    for message in [
        server_terminal::Message::Shell(TerminalEvent::Connected(input)),
        server_terminal::Message::Pane(PaneEvent::Resized(Size::new(1120.0, 700.0))),
    ] {
        let _ = app.update(Message::Terminal(message));
    }
    (app, sent)
}

/// The Terminal page, with the Setup group of the sidebar listing it and the shell filling the page.
#[test]
fn the_terminal_page_shows_a_shell_as_big_as_the_page() {
    let (mut app, _sent) = terminal_page();
    let printed = "\x1b[1;32mleon@server\x1b[0m:\x1b[1;34m~/moebius\x1b[0m$ ls\r\n\
        moebius.toml  \x1b[1;34m.moebius\x1b[0m  \x1b[1;34mworkflow\x1b[0m\r\n\
        \x1b[1;32mleon@server\x1b[0m:\x1b[1;34m~/moebius\x1b[0m$ git log --oneline -1\r\n\
        \x1b[33m64e3d24\x1b[0m Show Online once the saved server answers\r\n\
        \x1b[1;32mleon@server\x1b[0m:\x1b[1;34m~/moebius\x1b[0m$ ";
    let _ = app.update(Message::Terminal(server_terminal::Message::Shell(
        TerminalEvent::Output(printed.as_bytes().to_vec()),
    )));
    draw(
        &app,
        "terminal",
        &["Terminal", "Plugins", "Shell on the server", "connected"],
    );
}

/// The page opens a shell when it is opened and drops the connection when it is left; what is typed goes to the
/// shell while the pane has focus; the next opening starts a new shell.
#[test]
fn the_terminal_page_connects_when_opened_and_types_into_the_shell() {
    let (mut app, mut sent) = terminal_page();
    let connected = app.subscription().units();
    let _ = app.update(Message::Select(Page::Agents));
    assert!(
        app.subscription().units() < connected,
        "leaving the page keeps the connection"
    );
    let _ = app.update(Message::Select(Page::Terminal));
    assert_eq!(app.subscription().units(), connected);

    // A new shell has no connection until the new socket is open, so nothing is sent.
    let _ = sent.try_recv(); // the size the first connection started with
    let _ = sent.try_recv();
    let _ = app.update(Message::Terminal(key("a")));
    assert!(sent.try_recv().is_err());

    // Once connected, a key is bytes for the shell and the page tells the server its size.
    let (input, mut sent) = iced::futures::channel::mpsc::channel(16);
    let _ = app.update(Message::Terminal(server_terminal::Message::Shell(
        TerminalEvent::Connected(input),
    )));
    let _ = app.update(Message::Terminal(server_terminal::Message::Pane(
        PaneEvent::Resized(Size::new(1120.0, 700.0)),
    )));
    let mut told = Vec::new();
    while let Ok(input) = sent.try_recv() {
        told.push(input);
    }
    assert!(
        told.iter()
            .any(|input| matches!(input, client::TerminalInput::Resize { .. })),
        "{told:?}"
    );
    let _ = app.update(Message::Terminal(key("a")));
    assert_eq!(
        sent.try_recv().unwrap(),
        client::TerminalInput::Bytes(b"a".to_vec())
    );

    // A click outside the pane takes the focus away; the shell then ends and says so.
    let _ = app.update(Message::Terminal(server_terminal::Message::Pane(
        PaneEvent::Focus(false),
    )));
    let _ = app.update(Message::Terminal(key("a")));
    assert!(sent.try_recv().is_err());
    let _ = app.update(Message::Terminal(server_terminal::Message::Shell(
        TerminalEvent::Closed("the shell ended".into()),
    )));
    let mut ui = simulator(&app);
    assert!(ui.find("the shell ended").is_ok());
}

/// A clipboard of the test, which the pane reads and writes.
struct Clipboard(Option<String>);

impl iced::advanced::Clipboard for Clipboard {
    fn read(&self, _: iced::advanced::clipboard::Kind) -> Option<String> {
        self.0.clone()
    }

    fn write(&mut self, _: iced::advanced::clipboard::Kind, contents: String) {
        self.0 = Some(contents);
    }
}

/// Text dragged over with the mouse is copied when the button is released; a right click pastes what the clipboard
/// holds into the shell.
#[test]
fn the_terminal_copies_what_is_dragged_over_and_pastes_on_a_right_click() {
    use iced::advanced::renderer::Headless;
    use iced::{Event, Point, mouse};
    use iced_runtime::user_interface::{Cache, UserInterface};

    let (mut app, mut sent) = terminal_page();
    let _ = app.update(Message::Terminal(server_terminal::Message::Shell(
        TerminalEvent::Output(b"hello world\r\n$ ".to_vec()),
    )));
    while sent.try_recv().is_ok() {}
    let header = simulator(&app).find("Shell on the server").unwrap();
    let header = iced_test::selector::Bounded::bounds(&header);
    let mut renderer = futures_util::FutureExt::now_or_never(<iced::Renderer as Headless>::new(
        moebius_desktop::theme::MANROPE,
        16.into(),
        None,
    ))
    .unwrap()
    .unwrap();
    let mut ui = UserInterface::build(
        app.view(),
        Size::new(1440.0, 900.0),
        Cache::default(),
        &mut renderer,
    );
    // The first row of the screen is below the header and the screen's padding.
    let [width, height] = moebius_desktop::theme::SHELL_CELL;
    let row = header.y + header.height + 2.0 * moebius_desktop::theme::SPACE_10 + height / 2.0;
    let left = header.x + width / 2.0;
    let (start, end) = (Point::new(left, row), Point::new(left + 4.0 * width, row));
    let mut clipboard = Clipboard(None);
    let mut messages = Vec::new();
    for (at, event) in [
        (start, mouse::Event::CursorMoved { position: start }),
        (start, mouse::Event::ButtonPressed(mouse::Button::Left)),
        (end, mouse::Event::CursorMoved { position: end }),
        (end, mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        let _ = ui.update(
            &[Event::Mouse(event)],
            mouse::Cursor::Available(at),
            &mut renderer,
            &mut clipboard,
            &mut messages,
        );
    }
    assert_eq!(clipboard.0.as_deref(), Some("hello"));

    clipboard.0 = Some("ls -la".into());
    let _ = ui.update(
        &[Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Right,
        ))],
        mouse::Cursor::Available(end),
        &mut renderer,
        &mut clipboard,
        &mut messages,
    );
    drop(ui);
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(
        sent.try_recv().unwrap(),
        client::TerminalInput::Bytes(b"ls -la".to_vec())
    );
}
