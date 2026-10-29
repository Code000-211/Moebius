//! The Ports page: the ports that runs and terminals listen on, the ones forwarded by hand, the buttons of a row, the
//! page narrowed to what it was opened from, and the offer to open a port that appears in what is watched.

use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{ForwardEvent, Forwarding, ServerEvent, StreamEvent};
use moebius_desktop::screens::ports::{self, Scope};
use serde_json::json;

use crate::fixtures::{clicked, clicked_id, draw, overview, shows, update_all};

/// What the server lists: a dev server of a run, a terminal's, a chat's and one of the server's own terminal.
fn listed() -> Message {
    let ports = serde_json::from_value(json!([
        {"port": 3000, "pid": 4120, "process": "node", "kind": "run", "run": 9},
        {"port": 5173, "pid": 4388, "process": "vite", "kind": "terminal", "run": 9},
        {"port": 8000, "pid": 5021, "process": "python3", "kind": "chat", "run": 7},
        {"port": 9229, "pid": 777, "process": "node", "kind": "terminal", "run": null}
    ]))
    .unwrap();
    Message::Ports(ports::Message::Listed(Ok(ports)))
}

fn forwarded(remote: u16, local: u16) -> Message {
    Message::Ports(ports::Message::Forward(ForwardEvent {
        remote,
        what: Forwarding::Listening(local),
    }))
}

/// The app on the Ports page: four ports are open and forwarded to the same numbers here, but 5173, which is on 5174;
/// 8080 is forwarded by hand; and 9229 cannot be listened on.
fn ports_page() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Ports));
    update_all(&mut app, vec![listed()]);
    for message in [
        ports::Message::OpenForm,
        ports::Message::FormEdited("8080".into()),
        ports::Message::FormSubmit,
    ] {
        let _ = app.update(Message::Ports(message));
    }
    update_all(
        &mut app,
        vec![
            forwarded(3000, 3000),
            forwarded(5173, 5174),
            forwarded(8000, 8000),
            forwarded(8080, 8080),
            Message::Ports(ports::Message::Forward(ForwardEvent {
                remote: 9229,
                what: Forwarding::Failed(
                    "cannot listen on 127.0.0.1:9229: address in use".to_string(),
                ),
            })),
        ],
    );
    app
}

fn event(kind: &str, data: serde_json::Value) -> Message {
    Message::Stream(StreamEvent::Event(ServerEvent {
        kind: kind.into(),
        data,
    }))
}

/// The page lists each port with where it comes from and its local address, and a row has its four buttons; the
/// sidebar lists the page under Setup.
#[test]
fn the_ports_page_lists_open_and_forwarded_ports() {
    let app = ports_page();
    draw(
        &app,
        "ports",
        &[
            "Ports",
            "Setup",
            "3000",
            "node \u{b7} run 9",
            "vite \u{b7} terminal of run 9",
            "python3 \u{b7} chat 7",
            "node \u{b7} server terminal",
            "by hand",
            "127.0.0.1:5174",
            "127.0.0.1:8080",
            "Open in browser",
            "Copy address",
            "Change local port",
            "Stop forwarding",
            "Forward a port\u{2026}",
            "Restore forwarded ports when reconnecting",
        ],
    );
}

/// "Forward a port…" opens a field that takes a port of the server, and "Change local port" one for the port here.
#[test]
fn a_port_is_forwarded_by_hand_and_its_local_port_changed() {
    let mut app = ports_page();
    let _ = app.update(Message::Ports(ports::Message::OpenForm));
    let _ = app.update(Message::Ports(ports::Message::FormEdited("5432".into())));
    let _ = app.update(Message::Ports(ports::Message::Change(3000)));
    let _ = app.update(Message::Ports(ports::Message::LocalEdited("3100".into())));
    draw(
        &app,
        "ports-form",
        &["Forward", "Cancel", "Save", "127.0.0.1:"],
    );
    // Saving sends the change; a port the field cannot take is refused with a line.
    assert!(!clicked(&app, "Save").is_empty());
    let _ = app.update(Message::Ports(ports::Message::LocalEdited("web".into())));
    let _ = app.update(Message::Ports(ports::Message::LocalSubmit));
    assert!(shows(&app, "Enter a port from 1 to 65535."));
}

/// Each button of a row sends its message, and stopping a forward leaves the port in the list to be forwarded again.
#[test]
fn the_buttons_of_a_row_send_what_they_say() {
    let mut app = ports_page();
    let sent = format!("{:?}", clicked(&app, "Copy address"));
    assert!(sent.contains("Copy("), "{sent}");
    let sent = format!("{:?}", clicked(&app, "Open in browser"));
    assert!(sent.contains("Open("), "{sent}");
    let _ = app.update(Message::Ports(ports::Message::Stop(3000)));
    assert!(shows(&app, "Not forwarded"));
    assert!(shows(&app, "Forward"));
    let sent = format!("{:?}", clicked(&app, "Forward"));
    assert!(sent.contains("Start(3000)"), "{sent}");
}

/// The button of the Terminal page opens the Ports page on the ports of that terminal, with a way to show all of them.
#[test]
fn the_terminal_page_opens_the_ports_of_its_own_shell() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Terminal));
    update_all(&mut app, vec![listed()]);
    let opened = clicked_id(&app, ports::OPEN_PORTS);
    assert!(
        opened
            .iter()
            .any(|message| matches!(message, Message::OpenPorts(Some(Scope::Server)))),
        "{opened:?}"
    );
    update_all(&mut app, opened);
    assert!(shows(&app, "Ports of the server's terminal"));
    assert!(shows(&app, "9229"));
    assert!(!shows(&app, "3000"));
    assert!(shows(&app, "\u{2190} Back to Terminal"));
    click_show_all(&mut app);
    assert!(shows(&app, "3000"));
}

fn click_show_all(app: &mut App) {
    let messages = clicked(app, "Show all");
    update_all(app, messages);
}

/// A port that opens in the terminal being watched is offered over the page; the offer is answered or dismissed.
#[test]
fn a_port_that_opens_in_the_terminal_on_show_is_offered() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Terminal));
    // A port of some run is not what the page watches.
    update_all(
        &mut app,
        vec![event(
            "port_opened",
            json!({"port": 4000, "pid": 1, "process": "node", "kind": "run", "run": 3}),
        )],
    );
    assert!(!shows(&app, "Dismiss"));
    update_all(
        &mut app,
        vec![
            event(
                "port_opened",
                json!({"port": 3000, "pid": 2, "process": "node", "kind": "terminal", "run": null}),
            ),
            forwarded(3000, 3000),
        ],
    );
    draw(
        &app,
        "ports-offer",
        &[
            "Port 3000 opened in server terminal",
            "127.0.0.1:3000",
            "Open in browser",
            "Dismiss",
        ],
    );
    let sent = format!("{:?}", clicked(&app, "Open in browser"));
    assert!(sent.contains("OpenOffer"), "{sent}");
    let sent = format!("{:?}", clicked(&app, "Show ports"));
    assert!(sent.contains("Visit(Ports)"), "{sent}");
    let dismissed = clicked(&app, "Dismiss");
    update_all(&mut app, dismissed);
    assert!(!shows(&app, "Dismiss"));

    // The port closes: it is gone from the page.
    update_all(
        &mut app,
        vec![
            event("port_closed", json!({"port": 3000})),
            event("port_closed", json!({"port": 4000})),
        ],
    );
    let _ = app.update(Message::Select(Page::Ports));
    assert!(shows(&app, "No ports are open."));
}
