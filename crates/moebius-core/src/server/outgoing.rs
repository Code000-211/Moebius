//! Outgoing webhooks: posting what happens in Moebius to the URLs an owner added.
//!
//! [`Outgoing`] listens to the bus. For each event it picks the hooks that want its kind and that may see its project
//! ([`Scope`]), and posts `{id, kind, at, data}` to each, signed: the header [`SIGNATURE_HEADER`] holds
//! `sha256=<hex HMAC-SHA256 of the body under the hook's secret>`. A post that fails is tried again after each wait of
//! the [`Policy`]; when the last one fails too the hook is given up on for that event and a `webhook_failed` item is
//! put in the inbox. The secret of a hook is kept in `secrets.toml` as `outgoing-webhook-<id>`, never in the store,
//! and the event is redacted before it leaves. An event of a private project goes only to a hook that names the
//! project; an event of no project goes only to a hook that names none. The hooks are edited through
//! `api::outgoing`.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::broadcast::{Receiver, error::RecvError};
use tokio::task::JoinHandle;

use super::settings::Secrets;
use super::webhooks::hmac_sha256;
use crate::app::App;
use crate::blobs::{redact_json, remember_secret};
use crate::bus::AppEvent;
use crate::store::{OutgoingWebhook, Store, now};

/// The header that carries the signature of a delivery.
pub const SIGNATURE_HEADER: &str = "X-Moebius-Signature-256";

/// The kind of the inbox item that tells a hook was given up on.
pub const WEBHOOK_FAILED: &str = "webhook_failed";

/// The name of the secret that signs the deliveries of hook `id`.
pub fn secret_name(id: i64) -> String {
    format!("outgoing-webhook-{id}")
}

/// The signature of `body` under `secret`, as the header carries it.
pub fn sign(secret: &str, body: &[u8]) -> String {
    format!(
        "sha256={}",
        hex::encode(hmac_sha256(secret.as_bytes(), body))
    )
}

/// How a failing delivery is retried: it waits for each duration in turn and tries again, so there is one attempt more
/// than there are waits.
#[derive(Debug, Clone)]
pub struct Policy {
    pub backoff: Vec<Duration>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            backoff: vec![
                Duration::from_secs(5),
                Duration::from_secs(30),
                Duration::from_secs(5 * 60),
                Duration::from_secs(30 * 60),
            ],
        }
    }
}

/// Which project an event belongs to, which decides the hooks that may have it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// An event of the repository with this id.
    Project(i64),
    /// An event of the whole server, such as an installed package.
    Server,
    /// An event whose project cannot be found, or that is not known to be of the whole server, which goes nowhere.
    Unknown,
}

/// The project of `event`, from what its payload names.
pub fn scope(store: &Store, event: &AppEvent) -> Scope {
    let data = &event.data;
    let int = |key: &str| data.get(key).and_then(Value::as_i64);
    let found = |repository: Option<Option<i64>>| match repository {
        Some(Some(id)) => Scope::Project(id),
        Some(None) => Scope::Server,
        None => Scope::Unknown,
    };
    if let Some(id) = int("repository_id") {
        return Scope::Project(id);
    }
    if event.kind.starts_with("repository_") {
        return int("id").map_or(Scope::Unknown, Scope::Project);
    }
    if let Some(name) = data.get("repository").and_then(Value::as_str) {
        let repositories = store.repositories().unwrap_or_default();
        return found(
            repositories
                .iter()
                .find(|r| r.name == name)
                .map(|r| Some(r.id)),
        );
    }
    if let Some(id) = int("repository") {
        return Scope::Project(id);
    }
    if let Some(id) = int("card") {
        return found(
            store
                .card(id)
                .ok()
                .flatten()
                .map(|card| Some(card.repository_id)),
        );
    }
    if let Some(id) = int("change") {
        return found(
            store
                .change(id)
                .ok()
                .flatten()
                .map(|change| change.repository_id),
        );
    }
    if event.kind.starts_with("workflow_") {
        return found(
            int("run")
                .and_then(|id| store.workflow_run(id).ok().flatten())
                .map(|run| Some(run.repository_id)),
        );
    }
    if event.kind.starts_with("run_") {
        let Some(run) = int("run").and_then(|id| store.run(id).ok().flatten()) else {
            return Scope::Unknown;
        };
        return match (run.repository_id, run.card_id) {
            (Some(id), _) => Scope::Project(id),
            (None, Some(card)) => found(
                store
                    .card(card)
                    .ok()
                    .flatten()
                    .map(|card| Some(card.repository_id)),
            ),
            (None, None) => Scope::Server,
        };
    }
    if is_server_wide(&event.kind) {
        Scope::Server
    } else {
        // A kind that names no project and is not known to be of the whole server may be of a private project (an
        // inbox item's title, a chat, a proposal), so it fails closed.
        Scope::Unknown
    }
}

/// Whether an event of `kind` that names no project is of the whole server.
fn is_server_wide(kind: &str) -> bool {
    const KINDS: [&str; 6] = [
        "harness_error",
        "extension_update_available",
        "experiment_done",
        "trial_done",
        "lagged",
        "settings_moved",
    ];
    KINDS.contains(&kind)
        || ["package_", "plugin_", "port_", "retro_"]
            .iter()
            .any(|prefix| kind.starts_with(prefix))
}

/// Whether `hook` is to have an event of `kind` that is of `scope`.
pub fn wants(store: &Store, hook: &OutgoingWebhook, kind: &str, scope: Scope) -> bool {
    if !hook.kinds.iter().any(|k| k == kind) {
        return false;
    }
    match scope {
        Scope::Unknown => false,
        Scope::Server => hook.projects.is_empty(),
        Scope::Project(id) => {
            hook.projects.contains(&id)
                || (hook.projects.is_empty() && !store.repository_is_private(id).unwrap_or(true))
        }
    }
}

/// Why a delivery did not arrive.
#[derive(Debug, thiserror::Error)]
enum DeliveryError {
    #[error("the receiver answered {0}")]
    Status(u16),
    #[error("the receiver could not be reached: {0}")]
    Send(String),
    #[error("the hook has no secret to sign with")]
    NoSecret,
}

/// Posts the events of the bus to the hooks that want them.
pub struct Outgoing {
    app: Arc<App>,
    secrets: Secrets,
    policy: Policy,
    client: reqwest::Client,
    events: Receiver<AppEvent>,
}

impl Outgoing {
    /// Starts listening to the bus at once, so that no event after this call is missed.
    pub fn new(app: Arc<App>, secrets: Secrets, policy: Policy) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        let events = app.bus.subscribe();
        Self {
            app,
            secrets,
            policy,
            client,
            events,
        }
    }

    /// Delivers events until the bus is gone.
    pub async fn run(mut self) {
        loop {
            match self.events.recv().await {
                Ok(event) => {
                    self.dispatch(&event);
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            }
        }
    }

    /// Starts the delivery of `event` to every hook that wants it; each handle ends when its delivery has arrived or
    /// been given up on.
    pub fn dispatch(&self, event: &AppEvent) -> Vec<JoinHandle<()>> {
        let store = &self.app.store;
        let Ok(hooks) = store.outgoing_webhooks() else {
            return Vec::new();
        };
        let hooks: Vec<_> = hooks
            .into_iter()
            .filter(|hook| hook.kinds.contains(&event.kind))
            .collect();
        if hooks.is_empty() {
            return Vec::new();
        }
        let scope = scope(store, event);
        let mut id = [0u8; 16];
        let _ = getrandom::fill(&mut id);
        let body = json!({
            "id": hex::encode(id),
            "kind": event.kind,
            "at": now(),
            "data": redact_json(&event.data),
        })
        .to_string();
        hooks
            .into_iter()
            .filter(|hook| wants(store, hook, &event.kind, scope))
            .map(|hook| {
                let (app, secrets, policy, client) = (
                    self.app.clone(),
                    self.secrets.clone(),
                    self.policy.clone(),
                    self.client.clone(),
                );
                let (body, kind) = (body.clone(), event.kind.clone());
                tokio::spawn(async move {
                    let outcome = deliver(&client, &secrets, &policy, &hook, &kind, &body).await;
                    if let Err(error) = outcome {
                        let _ = app.store.add_inbox_item(
                            WEBHOOK_FAILED,
                            None,
                            &format!("The webhook to {} was given up on", hook.url),
                            &json!({
                                "webhook": hook.id,
                                "url": hook.url,
                                "event": kind,
                                "error": error.to_string(),
                                "attempts": policy.backoff.len() + 1,
                            }),
                        );
                    }
                })
            })
            .collect()
    }
}

/// Posts `body` to the hook, again after each wait of the policy; the last error when none got through.
async fn deliver(
    client: &reqwest::Client,
    secrets: &Secrets,
    policy: &Policy,
    hook: &OutgoingWebhook,
    kind: &str,
    body: &str,
) -> Result<(), DeliveryError> {
    let secret = match secrets.get(&secret_name(hook.id)) {
        Ok(Some(secret)) => secret,
        _ => return Err(DeliveryError::NoSecret),
    };
    remember_secret(&secret);
    let signature = sign(&secret, body.as_bytes());
    let mut waits = policy.backoff.iter();
    loop {
        let sent = client
            .post(&hook.url)
            .header("Content-Type", "application/json")
            .header("X-Moebius-Event", kind)
            .header(SIGNATURE_HEADER, &signature)
            .body(body.to_string())
            .send()
            .await;
        let error = match sent {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => DeliveryError::Status(response.status().as_u16()),
            Err(e) => DeliveryError::Send(e.without_url().to_string()),
        };
        match waits.next() {
            Some(wait) => tokio::time::sleep(*wait).await,
            None => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::body::Bytes;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use moebius_plugin_api::Host;

    use super::*;
    use crate::bus::Event;

    /// A receiver on port 0 that answers 500 to its first `failures` posts and keeps what it got.
    #[derive(Clone)]
    struct Sink {
        failures: usize,
        posts: Arc<AtomicUsize>,
        got: Arc<Mutex<Vec<(HeaderMap, Bytes)>>>,
        arrived: Arc<tokio::sync::Notify>,
    }

    async fn receive(State(r): State<Sink>, headers: HeaderMap, body: Bytes) -> StatusCode {
        let n = r.posts.fetch_add(1, Ordering::SeqCst);
        if n < r.failures {
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
        r.got.lock().unwrap().push((headers, body));
        r.arrived.notify_one();
        StatusCode::OK
    }

    async fn receiver(failures: usize) -> (String, Sink) {
        let r = Sink {
            failures,
            posts: Arc::default(),
            got: Arc::default(),
            arrived: Arc::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/hook", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/hook", post(receive))
            .with_state(r.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        (url, r)
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        app: Arc<App>,
        outgoing: Outgoing,
        secrets: Secrets,
    }

    fn fixture(waits: usize) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let app = Arc::new(App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(5))),
            dir.path(),
        ));
        let secrets = Secrets::new(dir.path());
        let policy = Policy {
            backoff: vec![Duration::from_millis(1); waits],
        };
        let outgoing = Outgoing::new(app.clone(), secrets.clone(), policy);
        Fixture {
            _dir: dir,
            app,
            outgoing,
            secrets,
        }
    }

    impl Fixture {
        fn hook(&self, url: &str, kinds: &[&str], projects: &[i64]) -> OutgoingWebhook {
            let kinds: Vec<String> = kinds.iter().map(|k| k.to_string()).collect();
            let hook = self
                .app
                .store
                .add_outgoing_webhook(url, &kinds, projects)
                .unwrap();
            self.secrets
                .set(
                    &secret_name(hook.id),
                    "whsec_0123456789abcdef0123456789abcdef",
                )
                .unwrap();
            hook
        }

        async fn send(&self, event: Event) {
            for handle in self.outgoing.dispatch(&event.into()) {
                handle.await.unwrap();
            }
        }
    }

    fn moved(card: i64) -> Event {
        Event::CardMoved {
            card,
            from: "Backlog".into(),
            to: "Coding".into(),
        }
    }

    fn card(f: &Fixture, repository: i64) -> i64 {
        f.app
            .store
            .add_card(repository, "A card", "", "leon", "Backlog")
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn a_receiver_gets_a_signed_event_it_can_verify() {
        let f = fixture(2);
        let repo = f
            .app
            .store
            .add_repository("demo", "/d", "none", "main")
            .unwrap();
        let card = card(&f, repo.id);
        let (url, r) = receiver(0).await;
        f.hook(&url, &["card_moved"], &[]);
        f.send(moved(card)).await;

        let got = r.got.lock().unwrap();
        assert_eq!(got.len(), 1);
        let (headers, body) = &got[0];
        let secret = "whsec_0123456789abcdef0123456789abcdef";
        assert_eq!(
            headers[SIGNATURE_HEADER].to_str().unwrap(),
            sign(secret, body)
        );
        assert_eq!(headers["x-moebius-event"], "card_moved");
        let sent: Value = serde_json::from_slice(body).unwrap();
        assert_eq!(sent["kind"], "card_moved");
        assert_eq!(sent["data"]["to"], "Coding");
        assert!(sent["id"].as_str().unwrap().len() == 32);
    }

    #[tokio::test]
    async fn a_hook_gets_only_the_kinds_it_wants() {
        let f = fixture(0);
        let repo = f
            .app
            .store
            .add_repository("demo", "/d", "none", "main")
            .unwrap();
        let card = card(&f, repo.id);
        let (url, r) = receiver(0).await;
        f.hook(&url, &["card_closed"], &[]);
        f.send(moved(card)).await;
        assert_eq!(r.posts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_failing_receiver_is_retried() {
        let f = fixture(3);
        let repo = f
            .app
            .store
            .add_repository("demo", "/d", "none", "main")
            .unwrap();
        let card = card(&f, repo.id);
        let (url, r) = receiver(2).await;
        f.hook(&url, &["card_moved"], &[]);
        f.send(moved(card)).await;
        assert_eq!(r.posts.load(Ordering::SeqCst), 3);
        assert_eq!(r.got.lock().unwrap().len(), 1);
        assert!(f.app.store.inbox().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_receiver_that_keeps_failing_is_given_up_on_with_a_note_in_the_inbox() {
        let f = fixture(2);
        let repo = f
            .app
            .store
            .add_repository("demo", "/d", "none", "main")
            .unwrap();
        let card = card(&f, repo.id);
        let (url, r) = receiver(usize::MAX).await;
        let hook = f.hook(&url, &["card_moved"], &[]);
        f.send(moved(card)).await;
        assert_eq!(r.posts.load(Ordering::SeqCst), 3);
        let inbox = f.app.store.inbox().unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].kind, WEBHOOK_FAILED);
        assert_eq!(inbox[0].data["webhook"], hook.id);
        assert_eq!(inbox[0].data["event"], "card_moved");
        assert_eq!(inbox[0].data["error"], "the receiver answered 500");
        assert!(!inbox[0].data.to_string().contains("whsec_"));
    }

    #[tokio::test]
    async fn an_unreachable_receiver_is_given_up_on_too() {
        let f = fixture(1);
        let repo = f
            .app
            .store
            .add_repository("demo", "/d", "none", "main")
            .unwrap();
        let card = card(&f, repo.id);
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/hook", closed.local_addr().unwrap());
        drop(closed);
        f.hook(&url, &["card_moved"], &[]);
        f.send(moved(card)).await;
        assert_eq!(f.app.store.inbox().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_hook_without_a_secret_sends_nothing() {
        let f = fixture(0);
        let repo = f
            .app
            .store
            .add_repository("demo", "/d", "none", "main")
            .unwrap();
        let card = card(&f, repo.id);
        let (url, r) = receiver(0).await;
        f.app
            .store
            .add_outgoing_webhook(&url, &["card_moved".into()], &[])
            .unwrap();
        f.send(moved(card)).await;
        assert_eq!(r.posts.load(Ordering::SeqCst), 0);
        assert_eq!(f.app.store.inbox().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_event_of_a_private_project_reaches_only_a_hook_that_names_it() {
        let f = fixture(0);
        let open = f
            .app
            .store
            .add_repository("open", "/o", "none", "main")
            .unwrap();
        let secret = f
            .app
            .store
            .add_repository("secret", "/s", "none", "main")
            .unwrap();
        let other = f
            .app
            .store
            .add_repository("other", "/t", "none", "main")
            .unwrap();
        f.app.store.set_repository_private(secret.id, true).unwrap();
        let (open_card, secret_card) = (card(&f, open.id), card(&f, secret.id));

        let (all_url, all) = receiver(0).await;
        let (named_url, named) = receiver(0).await;
        let (other_url, other_r) = receiver(0).await;
        f.hook(&all_url, &["card_moved"], &[]);
        f.hook(&named_url, &["card_moved"], &[secret.id]);
        f.hook(&other_url, &["card_moved"], &[other.id]);

        f.send(moved(secret_card)).await;
        assert_eq!(all.posts.load(Ordering::SeqCst), 0);
        assert_eq!(named.posts.load(Ordering::SeqCst), 1);
        assert_eq!(other_r.posts.load(Ordering::SeqCst), 0);

        // A public project's event goes to the hook for all and to the hook that names it, not to another's.
        f.send(moved(open_card)).await;
        assert_eq!(all.posts.load(Ordering::SeqCst), 1);
        assert_eq!(named.posts.load(Ordering::SeqCst), 1);
        assert_eq!(other_r.posts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_event_whose_project_is_gone_goes_nowhere() {
        let f = fixture(0);
        let (url, r) = receiver(0).await;
        f.hook(&url, &["card_moved"], &[]);
        f.send(moved(404)).await;
        assert_eq!(r.posts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_event_of_the_server_goes_to_a_hook_that_names_no_project() {
        let f = fixture(0);
        let repo = f
            .app
            .store
            .add_repository("demo", "/d", "none", "main")
            .unwrap();
        let (all_url, all) = receiver(0).await;
        let (named_url, named) = receiver(0).await;
        f.hook(&all_url, &["package_removed"], &[]);
        f.hook(&named_url, &["package_removed"], &[repo.id]);
        f.send(Event::PackageRemoved {
            name: "x".into(),
            kind: "plugin".into(),
        })
        .await;
        assert_eq!(all.posts.load(Ordering::SeqCst), 1);
        assert_eq!(named.posts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_event_that_names_no_project_and_is_not_of_the_server_goes_nowhere() {
        let f = fixture(0);
        let (url, r) = receiver(0).await;
        f.hook(&url, &["proposal_status", "chat_renamed"], &[]);
        f.send(Event::ProposalStatus {
            proposal: 3,
            status: "trying".into(),
        })
        .await;
        f.send(Event::ChatRenamed {
            run: 4,
            title: "Plan".into(),
        })
        .await;
        assert_eq!(r.posts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn run_listens_to_the_bus() {
        let f = fixture(0);
        let repo = f
            .app
            .store
            .add_repository("demo", "/d", "none", "main")
            .unwrap();
        let card = card(&f, repo.id);
        let (url, r) = receiver(0).await;
        f.hook(&url, &["card_moved"], &[]);
        let bus = f.app.bus.clone();
        tokio::spawn(f.outgoing.run());
        bus.publish(moved(card));
        tokio::time::timeout(Duration::from_secs(60), r.arrived.notified())
            .await
            .expect("the event is posted");
        assert_eq!(r.got.lock().unwrap().len(), 1);
    }
}
