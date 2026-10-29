use serde_json::json;

use super::*;
use crate::client::{ForwardEvent, Forwarding};

fn opened(ports: &mut Ports, remote: u16, run: i64, watched: bool) {
    let data = json!({"port": remote, "pid": 7, "process": "node", "kind": "run", "run": run});
    ports.on_event("port_opened", &data, |_| watched);
}

fn local_of(ports: &Ports, remote: u16) -> u16 {
    ports.address(remote).unwrap()[10..].parse().unwrap()
}

#[test]
fn a_port_that_opens_is_forwarded_at_once_and_goes_when_it_closes() {
    let mut ports = Ports::default();
    opened(&mut ports, 45001, 3, false);
    assert_eq!(ports.detected().len(), 1);
    assert!(ports.address(45001).is_some());
    assert!(ports.take_changed());
    assert!(!ports.take_changed());

    ports.on_event("port_closed", &json!({"port": 45001}), |_| false);
    assert!(ports.detected().is_empty());
    assert_eq!(ports.address(45001), None);
    assert!(ports.take_changed());
}

#[test]
fn the_local_port_is_the_same_number_when_it_is_free_and_another_when_it_is_not() {
    let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = busy.local_addr().unwrap().port();
    let mut ports = Ports::default();
    opened(&mut ports, taken, 1, false);
    assert_ne!(local_of(&ports, taken), taken);

    let spare = {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().port()
    };
    opened(&mut ports, spare, 1, false);
    assert_eq!(local_of(&ports, spare), spare);
}

#[test]
fn a_forward_the_person_asked_for_stays_when_the_port_closes() {
    let mut ports = Ports::default();
    let _ = ports.update(Message::OpenForm);
    let _ = ports.update(Message::FormEdited("45002".into()));
    let _ = ports.update(Message::FormSubmit);
    assert!(ports.address(45002).is_some());
    opened(&mut ports, 45002, 1, false);
    ports.on_event("port_closed", &json!({"port": 45002}), |_| false);
    assert!(ports.address(45002).is_some(), "it was asked for by hand");
    assert_eq!(
        ports.remembered(),
        vec![Forwarded {
            remote: 45002,
            local: local_of(&ports, 45002),
            by_hand: true
        }]
    );
}

#[test]
fn a_port_number_must_be_one_to_65535() {
    let mut ports = Ports::default();
    let _ = ports.update(Message::OpenForm);
    for typed in ["", "0", "70000", "web"] {
        let _ = ports.update(Message::FormEdited(typed.into()));
        let _ = ports.update(Message::FormSubmit);
        assert!(ports.remembered().is_empty(), "{typed}");
        assert!(ports.note.is_some());
    }
    let _ = ports.update(Message::FormEdited(" 45003 ".into()));
    let _ = ports.update(Message::FormSubmit);
    assert_eq!(ports.remembered().len(), 1);
    assert!(ports.form.is_none() && ports.note.is_none());
}

#[test]
fn a_stopped_forward_is_not_made_again_while_the_port_stays_open() {
    let mut ports = Ports::default();
    opened(&mut ports, 45004, 1, false);
    let _ = ports.update(Message::Stop(45004));
    assert_eq!(ports.address(45004), None);
    // The server lists it again, as it does after a reconnect, and it stays unforwarded until asked.
    ports.listed(vec![Port {
        port: 45004,
        pid: 7,
        process: String::new(),
        kind: "run".into(),
        run: Some(1),
    }]);
    assert_eq!(ports.address(45004), None);
    assert_eq!(ports.detected().len(), 1);
    let _ = ports.update(Message::Start(45004));
    assert!(ports.address(45004).is_some());

    // Once the port is gone, a new one on that number is forwarded again.
    let _ = ports.update(Message::Stop(45004));
    ports.on_event("port_closed", &json!({"port": 45004}), |_| false);
    opened(&mut ports, 45004, 1, false);
    assert!(ports.address(45004).is_some());
}

#[test]
fn the_local_port_can_be_changed_but_not_to_one_another_forward_uses() {
    let mut ports = Ports::default();
    opened(&mut ports, 45005, 1, false);
    opened(&mut ports, 45006, 1, false);
    let other = local_of(&ports, 45006);
    let _ = ports.update(Message::Change(45005));
    let _ = ports.update(Message::LocalEdited(other.to_string()));
    let _ = ports.update(Message::LocalSubmit);
    assert_ne!(local_of(&ports, 45005), other);
    assert!(ports.note.is_some() && ports.editing.is_some());

    let _ = ports.update(Message::LocalEdited("45099".into()));
    let _ = ports.update(Message::LocalSubmit);
    assert_eq!(local_of(&ports, 45005), 45099);
    assert!(ports.editing.is_none() && ports.note.is_none());
    assert!(
        ports
            .remembered()
            .iter()
            .any(|f| f.remote == 45005 && f.by_hand),
        "a local port chosen by hand is remembered as such"
    );
}

#[test]
fn what_the_forwarder_tells_shows_on_its_forward() {
    let mut ports = Ports::default();
    opened(&mut ports, 45007, 1, false);
    let tell = |what| {
        Message::Forward(ForwardEvent {
            remote: 45007,
            what,
        })
    };
    let _ = ports.update(tell(Forwarding::Listening(45107)));
    assert_eq!(ports.address(45007).as_deref(), Some("127.0.0.1:45107"));
    assert_eq!(ports.forwards[0].state, State::Listening);
    let _ = ports.update(tell(Forwarding::Refused("nothing answers".into())));
    assert_eq!(
        ports.forwards[0].refused.as_deref(),
        Some("nothing answers")
    );
    let _ = ports.update(tell(Forwarding::Failed("in use".into())));
    assert_eq!(ports.forwards[0].state, State::Failed("in use".into()));
}

#[test]
fn a_port_in_what_is_watched_is_offered_once_and_the_offer_goes_when_answered() {
    let mut ports = Ports::default();
    opened(&mut ports, 45008, 1, false);
    assert!(ports.offer().is_none());
    opened(&mut ports, 45009, 2, true);
    opened(&mut ports, 45009, 2, true);
    assert_eq!(ports.offer().map(|p| p.port), Some(45009));
    let _ = ports.update(Message::DismissOffer);
    assert!(ports.offer().is_none());

    opened(&mut ports, 45010, 2, true);
    ports.on_event("port_closed", &json!({"port": 45010}), |_| false);
    assert!(ports.offer().is_none(), "a port that closed is not offered");
}

#[test]
fn forwards_are_restored_from_what_was_remembered_and_dropped_if_the_server_has_no_such_port() {
    let mut ports = Ports::default();
    ports.connected(
        vec![
            Forwarded {
                remote: 45011,
                local: 45011,
                by_hand: false,
            },
            Forwarded {
                remote: 45012,
                local: 45112,
                by_hand: true,
            },
        ],
        true,
    );
    assert_eq!(ports.remembered().len(), 2);
    // The server says nothing is open: the forward made on its own goes, the one asked for by hand stays.
    ports.listed(Vec::new());
    let left: Vec<u16> = ports.remembered().iter().map(|f| f.remote).collect();
    assert_eq!(left, vec![45012]);
}

#[test]
fn the_page_shows_the_ports_of_a_scope_only() {
    let mut ports = Ports::default();
    opened(&mut ports, 45013, 1, false);
    opened(&mut ports, 45014, 2, false);
    assert_eq!(ports.count(Scope::Run(1)), 1);
    assert_eq!(ports.count(Scope::Server), 0);
    ports.show(Some(Scope::Run(2)));
    let shown: Vec<u16> = ports.rows().iter().map(|row| row.remote).collect();
    assert_eq!(shown, vec![45014]);
    let _ = ports.update(Message::ShowAll);
    assert_eq!(ports.rows().len(), 2);
}
