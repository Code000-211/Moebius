//! The event bus: what the core publishes for the desktop app's event stream.

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::server::ports::Port;
use crate::store::{Card, Change, Repository};

/// One thing that happened, as the desktop app receives it. It is built from an [`Event`] and deserializes from the
/// `{kind, data}` a client is sent, so nothing outside [`Event`] can name a kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppEvent {
    /// What happened, such as `card_moved` or `run_event`.
    pub kind: String,
    pub data: serde_json::Value,
}

impl From<Event> for AppEvent {
    fn from(event: Event) -> Self {
        let serde_json::Value::Object(mut object) = serde_json::to_value(event).unwrap_or_default()
        else {
            unreachable!("an Event serializes to an object")
        };
        let kind = match object.remove("kind") {
            Some(serde_json::Value::String(kind)) => kind,
            _ => unreachable!("an Event has a kind"),
        };
        let data = object.remove("data").unwrap_or(serde_json::Value::Null);
        Self { kind, data }
    }
}

/// Every event the core sends to a client, with the payload of its kind: what it publishes, and the `lagged` notice
/// the stream answers a slow client with. It serializes to the `{kind, data}` of [`AppEvent`], so the wire format is
/// the one `docs/reference/desktop-api.md` lists.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Event {
    AttemptJudged {
        card: i64,
        run: i64,
        result: String,
    },
    AttemptsSettled {
        card: i64,
        picked: Option<i64>,
        runs: Vec<i64>,
    },
    AutomationFired {
        automation: i64,
        firing: i64,
        status: String,
        run: Option<i64>,
    },
    CardAdded(Card),
    CardUpdated(Card),
    CardMoved {
        card: i64,
        from: String,
        to: String,
    },
    CardWaiting {
        card: i64,
        reason: Option<String>,
    },
    ChatRenamed {
        run: i64,
        title: String,
    },
    ChangeUpdated(Change),
    ChangeOffered(Change),
    ChangeMerged {
        change: i64,
        commit: Option<String>,
    },
    ChangeClosed {
        change: i64,
    },
    RepositoryAdded(Repository),
    RepositoryUpdated(Repository),
    RepositoryRemoved(Repository),
    RunStarted {
        run: i64,
        card: Option<i64>,
    },
    RunEvent {
        run: i64,
        seq: i64,
        kind: String,
        data: serde_json::Value,
    },
    RunEnded {
        run: i64,
        status: String,
    },
    ScriptChanged {
        repository: i64,
        column: String,
        path: String,
    },
    FileChanged {
        repository: i64,
        path: String,
    },
    TakeoverProposed {
        takeover: i64,
        kind: String,
        agreement: f64,
    },
    TakeoverStatus {
        takeover: i64,
        status: String,
    },
    ProposalStatus {
        proposal: i64,
        status: String,
    },
    Replayed {
        judgement: i64,
        run: i64,
        outcome: String,
    },
    ResearchStatus {
        research: i64,
        status: String,
    },
    ExperimentDone {
        research: i64,
        lever: String,
        value: Option<f64>,
        kept: bool,
    },
    RetroDone {
        date: String,
        status: String,
        proposals: usize,
    },
    RetroError {
        message: String,
    },
    TrialDone {
        proposal: i64,
        card: Option<i64>,
        outcome: String,
        regression: bool,
    },
    GrillingEnded {
        grilling: i64,
        status: String,
    },
    ForgeError {
        repository: String,
        message: String,
    },
    BoardError {
        card: i64,
        message: String,
    },
    HarnessError {
        /// The agent whose files are broken; left out when the harness configuration repository failed to refresh.
        #[serde(skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
        message: String,
    },
    PackageInstalled {
        name: String,
        version: String,
        kind: String,
    },
    PackageRemoved {
        name: String,
        kind: String,
    },
    PackageUpdateAvailable {
        name: String,
        version: String,
    },
    /// The nightly check found a newer version of an installed Pi package; it is in the inbox as `extension_update`.
    ExtensionUpdateAvailable {
        name: String,
        version: String,
    },
    PackageCheckError {
        /// The registry that could not be read; empty when the check itself failed.
        registry: String,
        message: String,
    },
    PortOpened(Port),
    PortClosed(Port),
    Lagged {
        missed: u64,
    },
}

/// Fans events out to every listener. A listener that falls too far behind loses the oldest events and is told so.
#[derive(Debug, Clone)]
pub struct Bus {
    tx: broadcast::Sender<AppEvent>,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(1024).0,
        }
    }

    /// Publishes an event; with nobody listening it is dropped.
    pub fn publish(&self, event: Event) {
        let _ = self.tx.send(event.into());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn card() -> Card {
        Card {
            id: 7,
            repository_id: 2,
            number: 12,
            title: "Fix the bug".into(),
            body: "It fails".into(),
            author: "ada".into(),
            column: "Not started".into(),
            created_at: 1_700_000_000,
            note: None,
            retried_after: None,
            confirmed: false,
        }
    }

    fn change() -> Change {
        Change {
            id: 4,
            repository_id: Some(2),
            card_id: Some(7),
            branch: "moebius/12".into(),
            state: "open".into(),
            pull_request: Some(31),
            checks: Some("pending".into()),
            review: None,
            head: Some("abc123".into()),
            published_at: Some(1_700_000_000),
            checks_head: Some("abc123".into()),
            failed_checks: None,
            check_output: None,
            harness_repository: false,
        }
    }

    fn repository() -> Repository {
        Repository {
            id: 2,
            name: "moebius".into(),
            path: "/srv/moebius".into(),
            forge: "github".into(),
            default_branch: "main".into(),
            github: Some("owner/repo".into()),
            trusted: vec!["ada".into()],
            project: None,
        }
    }

    fn port() -> Port {
        Port {
            port: 8080,
            pid: 42,
            process: "node".into(),
            kind: "run".into(),
            run: Some(9),
        }
    }

    /// One of every [`Event`], with the `{kind, data}` `docs/reference/desktop-api.md` lists for it.
    fn fixtures() -> Vec<(Event, Value)> {
        let card = card();
        let change = change();
        let repository = repository();
        let port = port();
        vec![
            (
                Event::AttemptJudged {
                    card: 7,
                    run: 3,
                    result: "passed".into(),
                },
                json!({"kind": "attempt_judged", "data": {"card": 7, "run": 3, "result": "passed"}}),
            ),
            (
                Event::AttemptsSettled {
                    card: 7,
                    picked: Some(3),
                    runs: vec![3, 4],
                },
                json!({"kind": "attempts_settled", "data": {"card": 7, "picked": 3, "runs": [3, 4]}}),
            ),
            (
                Event::AutomationFired {
                    automation: 2,
                    firing: 5,
                    status: "started".into(),
                    run: Some(9),
                },
                json!({"kind": "automation_fired", "data": {"automation": 2, "firing": 5, "status": "started", "run": 9}}),
            ),
            (
                Event::CardAdded(card.clone()),
                json!({"kind": "card_added", "data": card.clone()}),
            ),
            (
                Event::CardUpdated(card.clone()),
                json!({"kind": "card_updated", "data": card.clone()}),
            ),
            (
                Event::CardMoved {
                    card: 7,
                    from: "Backlog".into(),
                    to: "Not started".into(),
                },
                json!({"kind": "card_moved", "data": {"card": 7, "from": "Backlog", "to": "Not started"}}),
            ),
            (
                Event::CardWaiting {
                    card: 7,
                    reason: Some("waiting for a person".into()),
                },
                json!({"kind": "card_waiting", "data": {"card": 7, "reason": "waiting for a person"}}),
            ),
            (
                Event::ChatRenamed {
                    run: 4,
                    title: "Plan".into(),
                },
                json!({"kind": "chat_renamed", "data": {"run": 4, "title": "Plan"}}),
            ),
            (
                Event::ChangeUpdated(change.clone()),
                json!({"kind": "change_updated", "data": change.clone()}),
            ),
            (
                Event::ChangeOffered(change.clone()),
                json!({"kind": "change_offered", "data": change.clone()}),
            ),
            (
                Event::ChangeMerged {
                    change: 4,
                    commit: Some("abc123".into()),
                },
                json!({"kind": "change_merged", "data": {"change": 4, "commit": "abc123"}}),
            ),
            (
                Event::ChangeClosed { change: 4 },
                json!({"kind": "change_closed", "data": {"change": 4}}),
            ),
            (
                Event::RepositoryAdded(repository.clone()),
                json!({"kind": "repository_added", "data": repository.clone()}),
            ),
            (
                Event::RepositoryUpdated(repository.clone()),
                json!({"kind": "repository_updated", "data": repository.clone()}),
            ),
            (
                Event::RepositoryRemoved(repository.clone()),
                json!({"kind": "repository_removed", "data": repository.clone()}),
            ),
            (
                Event::RunStarted {
                    run: 9,
                    card: Some(7),
                },
                json!({"kind": "run_started", "data": {"run": 9, "card": 7}}),
            ),
            (
                Event::RunStarted { run: 9, card: None },
                json!({"kind": "run_started", "data": {"run": 9, "card": null}}),
            ),
            (
                Event::RunEvent {
                    run: 9,
                    seq: 1,
                    kind: "said".into(),
                    data: json!({"message": "hello"}),
                },
                json!({"kind": "run_event", "data": {"run": 9, "seq": 1, "kind": "said", "data": {"message": "hello"}}}),
            ),
            (
                Event::RunEnded {
                    run: 9,
                    status: "succeeded".into(),
                },
                json!({"kind": "run_ended", "data": {"run": 9, "status": "succeeded"}}),
            ),
            (
                Event::ScriptChanged {
                    repository: 2,
                    column: "Not started".into(),
                    path: ".moebius/ready.luau".into(),
                },
                json!({"kind": "script_changed", "data": {"repository": 2, "column": "Not started", "path": ".moebius/ready.luau"}}),
            ),
            (
                Event::FileChanged {
                    repository: 2,
                    path: "README.md".into(),
                },
                json!({"kind": "file_changed", "data": {"repository": 2, "path": "README.md"}}),
            ),
            (
                Event::TakeoverProposed {
                    takeover: 1,
                    kind: "tool_call".into(),
                    agreement: 0.9,
                },
                json!({"kind": "takeover_proposed", "data": {"takeover": 1, "kind": "tool_call", "agreement": 0.9}}),
            ),
            (
                Event::TakeoverStatus {
                    takeover: 1,
                    status: "merged".into(),
                },
                json!({"kind": "takeover_status", "data": {"takeover": 1, "status": "merged"}}),
            ),
            (
                Event::ProposalStatus {
                    proposal: 3,
                    status: "trying".into(),
                },
                json!({"kind": "proposal_status", "data": {"proposal": 3, "status": "trying"}}),
            ),
            (
                Event::Replayed {
                    judgement: 5,
                    run: 9,
                    outcome: "accepted".into(),
                },
                json!({"kind": "replayed", "data": {"judgement": 5, "run": 9, "outcome": "accepted"}}),
            ),
            (
                Event::ResearchStatus {
                    research: 6,
                    status: "running".into(),
                },
                json!({"kind": "research_status", "data": {"research": 6, "status": "running"}}),
            ),
            (
                Event::ExperimentDone {
                    research: 6,
                    lever: "moebius/lever/a".into(),
                    value: Some(1.5),
                    kept: true,
                },
                json!({"kind": "experiment_done", "data": {"research": 6, "lever": "moebius/lever/a", "value": 1.5, "kept": true}}),
            ),
            (
                Event::RetroDone {
                    date: "2026-01-01".into(),
                    status: "proposed".into(),
                    proposals: 2,
                },
                json!({"kind": "retro_done", "data": {"date": "2026-01-01", "status": "proposed", "proposals": 2}}),
            ),
            (
                Event::RetroError {
                    message: "boom".into(),
                },
                json!({"kind": "retro_error", "data": {"message": "boom"}}),
            ),
            (
                Event::TrialDone {
                    proposal: 3,
                    card: Some(7),
                    outcome: "positive".into(),
                    regression: false,
                },
                json!({"kind": "trial_done", "data": {"proposal": 3, "card": 7, "outcome": "positive", "regression": false}}),
            ),
            (
                Event::GrillingEnded {
                    grilling: 1,
                    status: "proposed".into(),
                },
                json!({"kind": "grilling_ended", "data": {"grilling": 1, "status": "proposed"}}),
            ),
            (
                Event::ForgeError {
                    repository: "owner/repo".into(),
                    message: "boom".into(),
                },
                json!({"kind": "forge_error", "data": {"repository": "owner/repo", "message": "boom"}}),
            ),
            (
                Event::BoardError {
                    card: 7,
                    message: "boom".into(),
                },
                json!({"kind": "board_error", "data": {"card": 7, "message": "boom"}}),
            ),
            (
                Event::HarnessError {
                    agent: Some("Coder".into()),
                    message: "boom".into(),
                },
                json!({"kind": "harness_error", "data": {"agent": "Coder", "message": "boom"}}),
            ),
            (
                Event::HarnessError {
                    agent: None,
                    message: "boom".into(),
                },
                json!({"kind": "harness_error", "data": {"message": "boom"}}),
            ),
            (
                Event::PackageInstalled {
                    name: "gate".into(),
                    version: "1.0.0".into(),
                    kind: "plugin".into(),
                },
                json!({"kind": "package_installed", "data": {"name": "gate", "version": "1.0.0", "kind": "plugin"}}),
            ),
            (
                Event::PackageRemoved {
                    name: "gate".into(),
                    kind: "plugin".into(),
                },
                json!({"kind": "package_removed", "data": {"name": "gate", "kind": "plugin"}}),
            ),
            (
                Event::PackageUpdateAvailable {
                    name: "gate".into(),
                    version: "1.1.0".into(),
                },
                json!({"kind": "package_update_available", "data": {"name": "gate", "version": "1.1.0"}}),
            ),
            (
                Event::ExtensionUpdateAvailable {
                    name: "pi-tools".into(),
                    version: "1.1.0".into(),
                },
                json!({"kind": "extension_update_available", "data": {"name": "pi-tools", "version": "1.1.0"}}),
            ),
            (
                Event::PackageCheckError {
                    registry: "moebius-plugins".into(),
                    message: "boom".into(),
                },
                json!({"kind": "package_check_error", "data": {"registry": "moebius-plugins", "message": "boom"}}),
            ),
            (
                Event::PortOpened(port.clone()),
                json!({"kind": "port_opened", "data": port.clone()}),
            ),
            (
                Event::PortClosed(port.clone()),
                json!({"kind": "port_closed", "data": port.clone()}),
            ),
            (
                Event::Lagged { missed: 3 },
                json!({"kind": "lagged", "data": {"missed": 3}}),
            ),
        ]
    }

    #[test]
    fn every_event_serializes_to_its_kind_and_data() {
        for (event, expected) in fixtures() {
            let published = AppEvent::from(event);
            assert_eq!(serde_json::to_value(&published).unwrap(), expected);
        }
    }

    #[test]
    fn every_variant_has_a_fixture() {
        let mut kinds: Vec<String> = fixtures()
            .into_iter()
            .map(|(event, _)| AppEvent::from(event).kind)
            .collect();
        kinds.sort();
        kinds.dedup();
        assert_eq!(
            kinds,
            variant_kinds(),
            "a variant has no fixture, or a fixture no variant"
        );
    }

    /// The `kind` each variant of [`Event`] serializes to, read from this file as serde's `snake_case` makes it.
    fn variant_kinds() -> Vec<String> {
        let source = include_str!("bus.rs");
        let body = source
            .split_once("pub enum Event {")
            .expect("the Event enum")
            .1
            .split_once("\n}")
            .expect("the end of the enum")
            .0;
        let mut kinds: Vec<String> = body
            .lines()
            .map(str::trim)
            .filter(|line| line.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
            .map(|line| snake_case(line.split(['{', '(', ' ']).next().unwrap_or_default()))
            .collect();
        kinds.sort();
        kinds
    }

    /// `CardAdded` as `card_added`.
    fn snake_case(name: &str) -> String {
        let mut out = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_ascii_uppercase() {
                if i > 0 {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
            } else {
                out.push(c);
            }
        }
        out
    }
}
