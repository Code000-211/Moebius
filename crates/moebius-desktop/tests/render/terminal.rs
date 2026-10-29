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
