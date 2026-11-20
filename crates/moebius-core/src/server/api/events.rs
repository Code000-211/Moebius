//! The event stream: every [`AppEvent`] the core publishes on its bus, as it happens.
//!
//! - `GET /v1/events`: a WebSocket on which every event arrives as one JSON text message; one that a slow client
//!   missed is answered with a `lagged` event that says how many. An event about a private project the person is no
//!   member of (its record, a repository, card, change, run, inbox item or grilling of it) is not sent to them (see
//!   `app::projects::sight`).
//! - What a run says while it works (its `run_event`s: every streamed piece of an answer, every tool call) is sent only
//!   for the runs the client asked for, with `{"subscribe": [run ids]}` and `{"unsubscribe": [run ids]}` on the same
//!   socket; the rest of the events, and of a run's the ones a board or the Fleet needs (`agent_start`, `agent_settled`,
//!   `moebius_error`), go to everyone. Who may see a run is worked out once per run and connection, and again after
//!   an event that changes what a person sees; the store is read on a thread that may block, never on the runtime's.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::Extension;
use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use tokio::sync::broadcast::error::RecvError;

use super::{ApiState, privacy};
use crate::app::{App, Sight};
use crate::bus::{AppEvent, Event};
use crate::decide::Actor;

/// The route of the event stream.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/events", get(events))
}

async fn events(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let rx = state.app.bus.subscribe();
    let app = state.app.clone();
    upgrade.on_upgrade(move |socket| stream(socket, rx, app, by))
}

/// The events about a run that carry what a chat says: another person's chat that is not shared is not sent to
/// them (see [`crate::app::chat_access`]).
const OF_A_RUN: [&str; 4] = ["run_started", "run_event", "run_ended", "chat_renamed"];

/// The events that tell one person something: only that person (`data.person`, a handle) is sent them.
const OF_A_PERSON: [&str; 2] = ["notice_added", "notice_read"];

/// The events after which what a person sees of the private projects is read again.
const OF_SIGHT: [&str; 4] = [
    "project_changed",
    "repository_added",
    "repository_updated",
    "repository_removed",
];

/// Whether `sight` sees what `event` is about; an event whose thing cannot be read is not sent.
pub(crate) fn seen(app: &App, sight: &Sight, event: &AppEvent) -> bool {
    if sight.hides_nothing() {
        return true;
    }
    seen_or_err(app, sight, event).unwrap_or(false)
}

fn seen_or_err(app: &App, sight: &Sight, event: &AppEvent) -> crate::app::Result<bool> {
    let kind = event.kind.as_str();
    let data = &event.data;
    let field = |name: &str| data[name].as_i64();
    if kind == "project_changed" {
        return Ok(field("id").is_none_or(|id| sight.project_record(id)));
    }
    if kind.starts_with("repository_") {
        return Ok(field("id").is_none_or(|id| sight.repository(id)));
    }
    // A script's, a project's kinds' and an intake request's events name their project as `project`.
    if let Some(project) = field("project_id").or(field("project")) {
        return Ok(sight.project(project));
    }
    if kind.starts_with("change_")
        && let Some(card) = field("card_id")
    {
        return app.sees_card(sight, card);
    }
    if let Some(repository) = field("repository").or(field("repository_id"))
        && !sight.repository(repository)
    {
        return Ok(false);
    }
    // `forge_error` and `settings_moved` name their repository by name.
    if let Some(name) = data["repository"].as_str()
        && app
            .store
            .repositories()?
            .iter()
            .any(|r| r.name == name && !sight.repository(r.id))
    {
        return Ok(false);
    }
    // A workflow's events name a workflow run as `run`, which belongs to a repository and to no card.
    let workflow = kind.starts_with("workflow_");
    let things = [
        ("cards", field("card")),
        ("cards", field("card_id")),
        ("runs", field("run").filter(|_| !workflow)),
        (privacy::WORKFLOW_RUNS, field("run").filter(|_| workflow)),
        ("changes", field("change")),
        ("grillings", field("grilling")),
        ("inbox", field("item")),
        ("automations", field("automation")),
        (privacy::WORKFLOW_RUNS, field("workflow_run")),
        ("research", field("research")),
        // A notice names what it is about inside it.
        ("cards", data["notice"]["card_id"].as_i64()),
        ("inbox", data["notice"]["inbox_item_id"].as_i64()),
    ];
    for (what, id) in things {
        if let Some(id) = id
            && !privacy::sees(app, sight, what, "", id)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether `by` may be told of `event`.
fn visible(app: &App, by: &Actor, event: &AppEvent) -> bool {
    if OF_A_PERSON.contains(&event.kind.as_str()) {
        return matches!(by, Actor::Member { handle, .. }
            if event.data["person"].as_str().is_some_and(|p| p.eq_ignore_ascii_case(handle)));
    }
    if !OF_A_RUN.contains(&event.kind.as_str()) {
        return true;
    }
    let Some(run) = event.data["run"].as_i64() else {
        return true;
    };
    match app.store.run(run) {
        Ok(Some(run)) => crate::app::may_use(by, &run, false),
        _ => true,
    }
}

/// The kinds of `run_event` that every client is sent, for any run it may see: they say where a run is (started
/// working, settled, failed), which the list of chats and the board follow. The others are sent for the runs a client
/// subscribed to only.
const OF_ANY_CLIENT: [&str; 3] = ["agent_start", "agent_settled", "moebius_error"];

/// How many runs one connection may subscribe to.
const MAX_WATCHED: usize = 256;

/// How many decisions about runs a connection keeps.
const MAX_REMEMBERED: usize = 4096;

/// What one connection knows: who it is for, what they see, the runs it asked for and whether each run may be shown.
struct Gate {
    app: Arc<App>,
    by: Actor,
    /// `None` when the sight could not be read: nothing private is sent.
    sight: Option<Sight>,
    watched: HashSet<i64>,
    /// Whether the person may be told of a run's events, worked out once; forgotten when what they see changes.
    runs: HashMap<i64, bool>,
}

/// What a client sends on the socket.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Request {
    subscribe: Vec<i64>,
    unsubscribe: Vec<i64>,
}

impl Gate {
    fn new(app: Arc<App>, by: Actor) -> Self {
        let sight = app.sight(&by).ok();
        Self {
            app,
            by,
            sight,
            watched: HashSet::new(),
            runs: HashMap::new(),
        }
    }

    /// Takes in what the client asked for; anything that is not a request is ignored.
    fn request(&mut self, text: &str) {
        let Ok(request) = serde_json::from_str::<Request>(text) else {
            return;
        };
        for run in request.unsubscribe {
            self.watched.remove(&run);
        }
        for run in request.subscribe {
            if self.watched.len() < MAX_WATCHED {
                self.watched.insert(run);
            }
        }
    }

    /// The run `event` is about, when it is one of the events that carry a run's words.
    fn run_of(event: &AppEvent) -> Option<i64> {
        OF_A_RUN
            .contains(&event.kind.as_str())
            .then(|| event.data["run"].as_i64())
            .flatten()
    }

    /// The answer when it needs no read of the store: an event the client did not ask for, one for a person, a run
    /// already decided. `None` when [`Gate::decide`] has to read.
    fn quick(&self, event: &AppEvent) -> Option<bool> {
        let kind = event.kind.as_str();
        if kind == "run_event"
            && let Some(run) = event.data["run"].as_i64()
            && !self.watched.contains(&run)
            && !event.data["kind"]
                .as_str()
                .is_some_and(|k| OF_ANY_CLIENT.contains(&k))
        {
            return Some(false);
        }
        if OF_A_PERSON.contains(&kind) {
            return Some(visible(&self.app, &self.by, event));
        }
        if kind == "lagged" {
            return Some(true);
        }
        Self::run_of(event).and_then(|run| self.runs.get(&run).copied())
    }

    /// Whether the client is sent `event`; reads the store, so it runs where that may block.
    fn decide(&mut self, event: &AppEvent) -> bool {
        let kind = event.kind.as_str();
        // What a person may read of a run depends on these.
        if OF_SIGHT.contains(&kind) || kind == "chat_shared" || kind == "chat_deleted" {
            self.runs.clear();
        }
        if OF_SIGHT.contains(&kind) {
            self.sight = self.app.sight(&self.by).ok();
        }
        let shown = match &self.sight {
            Some(sight) => seen(&self.app, sight, event),
            None => false,
        } && visible(&self.app, &self.by, event);
        if let Some(run) = Self::run_of(event) {
            if self.runs.len() >= MAX_REMEMBERED {
                self.runs.clear();
            }
            self.runs.insert(run, shown);
        }
        shown
    }
}

async fn stream(
    mut socket: WebSocket,
    mut rx: tokio::sync::broadcast::Receiver<AppEvent>,
    app: Arc<App>,
    by: Actor,
) {
    let gate = match tokio::task::spawn_blocking(move || Gate::new(app, by)).await {
        Ok(gate) => Arc::new(Mutex::new(gate)),
        Err(_) => return,
    };
    loop {
        tokio::select! {
            event = rx.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(RecvError::Lagged(missed)) => AppEvent::from(Event::Lagged { missed }),
                    Err(RecvError::Closed) => return,
                };
                let quick = gate.lock().ok().and_then(|gate| gate.quick(&event));
                let (shown, event) = match quick {
                    Some(shown) => (shown, event),
                    None => {
                        let gate = gate.clone();
                        let decided = tokio::task::spawn_blocking(move || {
                            let shown = gate.lock().is_ok_and(|mut gate| gate.decide(&event));
                            (shown, event)
                        })
                        .await;
                        match decided {
                            Ok(decided) => decided,
                            Err(_) => return,
                        }
                    }
                };
                if !shown {
                    continue;
                }
                let Ok(text) = serde_json::to_string(&event) else { continue };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(Message::Text(text))) => {
                    if let Ok(mut gate) = gate.lock() {
                        gate.request(&text);
                    }
                }
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    use super::*;
    use crate::server::api::testing::start;

    #[tokio::test]
    async fn the_events_of_a_chat_that_is_not_shared_do_not_reach_another_person() {
        let (_dir, state, _addr) = start().await;
        let store = &state.app.store;
        let lea = store
            .add_person("lea", "Lea", crate::store::Role::Member)
            .unwrap();
        let ben = store
            .add_person("ben", "Ben", crate::store::Role::Member)
            .unwrap();
        let run = store.add_run(None, "Moebius", "fake", "chat").unwrap();
        store.set_chat_owner(run.id, lea.id).unwrap();
        let of = |kind: Event| AppEvent::from(kind);
        let said = of(Event::RunEvent {
            run: run.id,
            seq: 1,
            kind: "said".into(),
            data: serde_json::json!({"message": "secret"}),
        });
        let (lea, ben) = (Actor::person(lea.id, "lea"), Actor::person(ben.id, "ben"));
        assert!(visible(&state.app, &lea, &said));
        assert!(!visible(&state.app, &ben, &said));
        store.set_chat_shared(run.id, true).unwrap();
        assert!(visible(&state.app, &ben, &said));
        store.set_chat_shared(run.id, false).unwrap();
        // What carries no chat's words goes to everyone, so that every list is read again.
        let shared = of(Event::ChatShared {
            run: run.id,
            shared: false,
        });
        assert!(visible(&state.app, &ben, &shared));
        assert!(!visible(
            &state.app,
            &ben,
            &of(Event::ChatRenamed {
                run: run.id,
                title: "t".into()
            })
        ));
    }

    #[tokio::test]
    async fn a_notice_reaches_the_person_it_is_for_and_no_one_else() {
        let (_dir, state, _addr) = start().await;
        let store = &state.app.store;
        let lea = store
            .add_person("lea", "Lea", crate::store::Role::Member)
            .unwrap();
        let ben = store
            .add_person("ben", "Ben", crate::store::Role::Member)
            .unwrap();
        let notice = store
            .add_notice(
                lea.id,
                &crate::store::NewNotice {
                    kind: crate::store::MENTIONED,
                    by: None,
                    card_id: None,
                    inbox_item_id: None,
                    title: "t",
                    body: "secret",
                },
            )
            .unwrap();
        let told = |kind: fn(String, crate::store::Notice) -> Event| {
            AppEvent::from(kind("Lea".into(), notice.clone()))
        };
        let added = told(|person, notice| Event::NoticeAdded { person, notice });
        let read = told(|person, notice| Event::NoticeRead { person, notice });
        let agent = Actor::Agent {
            name: "Moebius".into(),
            run: 1,
        };
        for event in [&added, &read] {
            assert!(visible(&state.app, &Actor::person(lea.id, "lea"), event));
            assert!(!visible(&state.app, &Actor::person(ben.id, "ben"), event));
            assert!(!visible(&state.app, &agent, event));
        }
    }

    #[tokio::test]
    async fn events_published_by_the_core_reach_the_client() {
        let (_dir, state, addr) = start().await;
        let mut request = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", state.token).parse().unwrap(),
        );
        let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        // The subscription exists once the upgrade is done; publish until the first event arrives.
        let event = Event::CardMoved {
            card: 1,
            from: "Backlog".into(),
            to: "Not started".into(),
        };
        let received = loop {
            state.app.bus.publish(event.clone());
            if let Ok(Some(message)) =
                tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await
            {
                break message.unwrap();
            }
        };
        let back: AppEvent = serde_json::from_str(received.to_text().unwrap()).unwrap();
        assert_eq!(back, AppEvent::from(event));

        let mut wrong = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        wrong
            .headers_mut()
            .insert("authorization", "Bearer nope".parse().unwrap());
        assert!(tokio_tungstenite::connect_async(wrong).await.is_err());
    }

    fn delta(run: i64, seq: i64) -> Event {
        Event::RunEvent {
            run,
            seq,
            kind: "message_update".into(),
            data: serde_json::json!({"type": "message_update", "text": "x".repeat(300)}),
        }
    }

    fn settled(run: i64, seq: i64) -> Event {
        Event::RunEvent {
            run,
            seq,
            kind: "agent_settled".into(),
            data: serde_json::json!({"aborted": false}),
        }
    }

    async fn connect(state: &ApiState, addr: &str) -> Client {
        let mut request = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", state.token).parse().unwrap(),
        );
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }

    type Client = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    /// The next event of the stream.
    async fn next(ws: &mut Client) -> AppEvent {
        let message = ws.next().await.unwrap().unwrap();
        serde_json::from_str(message.to_text().unwrap()).unwrap()
    }

    /// Asks for `run` and publishes pieces of it until one arrives: the server has read the request, and, as it reads
    /// them in the order they were sent, every request before it.
    async fn ask_for(ws: &mut Client, state: &ApiState, run: i64) {
        use futures_util::SinkExt;
        ws.send(tokio_tungstenite::tungstenite::Message::text(format!(
            r#"{{"subscribe": [{run}]}}"#
        )))
        .await
        .unwrap();
        loop {
            state.app.bus.publish(delta(run, 0));
            if let Ok(Some(Ok(message))) =
                tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await
                && message.to_text().unwrap().contains("message_update")
            {
                return;
            }
        }
    }

    /// The events up to and with the first one of the kind `last`, as (kind of the event or of its run event, `seq`).
    async fn until(ws: &mut Client, last: &str) -> Vec<(String, i64)> {
        let mut seen = Vec::new();
        loop {
            let event = next(ws).await;
            let kind = match event.kind.as_str() {
                "run_event" => event.data["kind"].as_str().unwrap().to_string(),
                other => other.to_string(),
            };
            seen.push((kind.clone(), event.data["seq"].as_i64().unwrap_or(0)));
            if kind == last {
                return seen;
            }
        }
    }

    #[tokio::test]
    async fn the_pieces_of_a_run_go_only_to_a_client_that_asked_for_the_run() {
        let (_dir, state, addr) = start().await;
        let mut quiet = connect(&state, &addr).await;
        let mut asking = connect(&state, &addr).await;
        ask_for(&mut asking, &state, 1).await;
        // Not asked for by the quiet one; a run's settling and ending go to everyone.
        state.app.bus.publish(delta(1, 41));
        state.app.bus.publish(delta(2, 42));
        state.app.bus.publish(settled(2, 43));
        state.app.bus.publish(Event::RunEnded {
            run: 2,
            status: "succeeded".into(),
        });
        let saw = until(&mut quiet, "run_ended").await;
        assert!(saw.contains(&("agent_settled".into(), 43)), "{saw:?}");
        assert!(
            !saw.iter()
                .any(|(kind, seq)| kind == "message_update" && *seq >= 41),
            "{saw:?}"
        );
        let saw = until(&mut asking, "run_ended").await;
        assert!(saw.contains(&("message_update".into(), 41)), "{saw:?}");
        assert!(!saw.contains(&("message_update".into(), 42)), "{saw:?}");
        assert!(saw.contains(&("agent_settled".into(), 43)), "{saw:?}");
    }

    #[tokio::test]
    async fn a_client_that_stopped_asking_for_a_run_is_sent_its_pieces_no_more() {
        use futures_util::SinkExt;
        let (_dir, state, addr) = start().await;
        let mut ws = connect(&state, &addr).await;
        ask_for(&mut ws, &state, 1).await;
        ws.send(tokio_tungstenite::tungstenite::Message::text(
            r#"{"unsubscribe": [1]}"#,
        ))
        .await
        .unwrap();
        // Run 2's pieces arriving shows the request about run 1 before it was read too.
        ask_for(&mut ws, &state, 2).await;
        state.app.bus.publish(delta(1, 77));
        state.app.bus.publish(delta(2, 78));
        loop {
            let event = next(&mut ws).await;
            assert_ne!(event.data["seq"], 77, "run 1 was not asked for any more");
            if event.data["seq"] == 78 {
                return;
            }
        }
    }

    #[tokio::test]
    async fn what_a_person_may_read_of_a_run_is_worked_out_once_and_again_after_it_changes() {
        let (_dir, state, _addr) = start().await;
        let store = &state.app.store;
        let lea = store
            .add_person("lea", "Lea", crate::store::Role::Member)
            .unwrap();
        let ben = store
            .add_person("ben", "Ben", crate::store::Role::Member)
            .unwrap();
        let run = store.add_run(None, "Moebius", "fake", "chat").unwrap();
        store.set_chat_owner(run.id, lea.id).unwrap();
        let mut gate = Gate::new(state.app.clone(), Actor::person(ben.id, "ben"));
        gate.request(&format!(r#"{{"subscribe": [{}]}}"#, run.id));
        let piece = AppEvent::from(delta(run.id, 1));
        assert_eq!(gate.quick(&piece), None, "not decided yet");
        assert!(!gate.decide(&piece));
        // Decided: no further read of the store, even for what the store would now answer differently.
        store.set_chat_shared(run.id, true).unwrap();
        assert_eq!(gate.quick(&piece), Some(false));
        // Told that it was shared, the decision is made again.
        let shared = AppEvent::from(Event::ChatShared {
            run: run.id,
            shared: true,
        });
        assert!(gate.decide(&shared));
        assert_eq!(gate.quick(&piece), None);
        assert!(gate.decide(&piece));
        assert_eq!(gate.quick(&piece), Some(true));
        // A run that was not asked for sends only what everyone needs.
        gate.request(&format!(r#"{{"unsubscribe": [{}]}}"#, run.id));
        assert_eq!(gate.quick(&piece), Some(false));
        assert_eq!(gate.quick(&AppEvent::from(settled(run.id, 2))), Some(true));
    }

    #[tokio::test]
    async fn a_client_on_the_board_is_sent_little_while_runs_stream() {
        let (_dir, state, addr) = start().await;
        let mut ws = connect(&state, &addr).await;
        // The connection is live once an event reaches it.
        loop {
            state
                .app
                .bus
                .publish(Event::RunStarted { run: 1, card: None });
            if let Ok(Some(_)) =
                tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await
            {
                break;
            }
        }
        // Three runs stream 1,000 pieces of about 340 bytes each, and settle.
        for seq in 1..=1_000 {
            for run in 1..=3 {
                state.app.bus.publish(delta(run, seq));
            }
        }
        for run in 1..=3 {
            state.app.bus.publish(settled(run, 1_001));
        }
        state.app.bus.publish(Event::RunEnded {
            run: 3,
            status: "succeeded".into(),
        });
        let mut bytes = 0;
        loop {
            let message = ws.next().await.unwrap().unwrap();
            let text = message.to_text().unwrap();
            bytes += text.len();
            if text.contains("run_ended") {
                break;
            }
        }
        println!("bytes sent to a client on the board: {bytes}");
        assert!(bytes < 2_000, "{bytes} bytes for 3,000 pieces");
    }
}
