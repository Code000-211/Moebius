//! The Moebius agent operates Moebius with its run's token (`$MOEBIUS_TOKEN`): the desktop API accepts the token of a
//! run of that agent while the run lasts and records what it decides as `agent:Moebius` with the run's id. The tokens of
//! other agents and revoked tokens get nothing, and the two things only a person may do (keys, shells) are refused.

use std::sync::Arc;

use moebius_core::config::Config;
use moebius_core::router::Grant;
use moebius_core::server::Server;
use reqwest::Method;
use serde_json::{Value, json};

struct Fixture {
    server: Arc<Server>,
    api: String,
    client: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl Fixture {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("moebius.toml");
        std::fs::write(&file, "[router]\nlisten = \"127.0.0.1:0\"\n").unwrap();
        let config = Config::load(&file).unwrap();
        let api = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let router = tokio::net::TcpListener::bind(config.router_listen)
            .await
            .unwrap();
        let (api_addr, router_addr) = (api.local_addr().unwrap(), router.local_addr().unwrap());
        let server = Arc::new(Server::new(config, router_addr).unwrap());
        tokio::spawn(server.clone().serve(api, router));
        Self {
            server,
            api: format!("http://{api_addr}"),
            client: reqwest::Client::new(),
            _dir: dir,
        }
    }

    /// A run of `agent` and the token it was started with.
    fn run(&self, agent: &str) -> (i64, String) {
        let run = self
            .server
            .app
            .store
            .add_run(None, agent, "pi-bolt", "chat")
            .unwrap();
        let token = self.server.router.issue_token(Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        });
        (run.id, token)
    }

    /// A run of the Grilling partner in a chat about `repository`, and its token.
    fn grilling(&self, repository: i64) -> (i64, String) {
        let (run, token) = self.run("Grilling-partner");
        self.server
            .app
            .store
            .set_run_repository(run, repository)
            .unwrap();
        (run, token)
    }

    async fn send(&self, method: Method, path: &str, token: &str, body: Option<Value>) -> u16 {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.api))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        request.send().await.unwrap().status().as_u16()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_moebius_agent_run_acts_as_the_agent_and_its_decisions_say_so() {
    let f = Fixture::start().await;
    let (run, token) = f.run("Moebius");
    assert_eq!(f.send(Method::GET, "/v1/snapshot", &token, None).await, 200);
    let edit = json!({ "set": { "board.round_limit": 7 } });
    assert_eq!(
        f.send(Method::PUT, "/v1/settings", &token, Some(edit))
            .await,
        200
    );
    // The same edit by a person is recorded as the person's.
    let by_person = json!({ "set": { "board.round_limit": 8 } });
    let person = f.server.token.to_string();
    assert_eq!(
        f.send(Method::PUT, "/v1/settings", &person, Some(by_person))
            .await,
        200
    );

    let judgements: Vec<Value> = f
        .client
        .get(format!("{}/v1/judgements?kind=edit_settings", f.api))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let deciders: Vec<(&str, &Value)> = judgements
        .iter()
        .map(|j| (j["decider"].as_str().unwrap(), &j["run"]))
        .collect();
    // Newest first.
    assert_eq!(
        deciders,
        [("person", &Value::Null), ("agent:Moebius", &json!(run))]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn other_agents_and_revoked_tokens_are_refused() {
    let f = Fixture::start().await;
    let (_, coder) = f.run("Coder");
    assert_eq!(f.send(Method::GET, "/v1/health", &coder, None).await, 401);
    // Neither can another agent's token read or write the settings and scripts of a project.
    for (method, path, body) in [
        (Method::GET, "/v1/repositories/1/settings", None),
        (
            Method::PUT,
            "/v1/repositories/1/settings/rules.md",
            Some(json!({ "text": "## x\ny\n" })),
        ),
        (Method::DELETE, "/v1/repositories/1/settings/checks", None),
        (
            Method::PUT,
            "/v1/repositories/1/scripts/Backlog",
            Some(json!({ "text": "" })),
        ),
    ] {
        assert_eq!(f.send(method, path, &coder, body).await, 401, "{path}");
    }
    let (_, moebius) = f.run("Moebius");
    assert_eq!(f.send(Method::GET, "/v1/health", &moebius, None).await, 200);
    f.server.router.revoke_token(&moebius);
    assert_eq!(f.send(Method::GET, "/v1/health", &moebius, None).await, 401);
    assert_eq!(
        f.send(Method::GET, "/v1/health", "nonsense", None).await,
        401
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn keys_shells_and_the_harness_setup_stay_with_a_person() {
    let f = Fixture::start().await;
    let (run, token) = f.run("Moebius");
    let secret = json!({ "value": "sk-test" });
    assert_eq!(
        f.send(
            Method::PUT,
            "/v1/secrets/anthropic",
            &token,
            Some(secret.clone())
        )
        .await,
        403
    );
    let person = f.server.token.to_string();
    assert_eq!(
        f.send(Method::PUT, "/v1/secrets/anthropic", &person, Some(secret))
            .await,
        200
    );
    // Moving the agents into a harness configuration repository is the person's choice.
    assert_eq!(
        f.send(Method::POST, "/v1/harness/setup", &token, None)
            .await,
        403
    );
    // A shell is asked for with a WebSocket upgrade.
    let response = f
        .client
        .get(format!("{}/v1/runs/{run}/terminal", f.api))
        .bearer_auth(&token)
        .header("connection", "upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 403);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_apps_report_stays_with_a_person_and_the_agent_reads_it() {
    let f = Fixture::start().await;
    let (_, token) = f.run("Moebius");
    let report = json!({
        "program": "/opt/moebius-desktop", "version": "1.2.3", "os": "linux", "arch": "x86_64",
        "host": "laptop", "server_address": "http://127.0.0.1:1"
    });
    assert_eq!(
        f.send(Method::PUT, "/v1/client", &token, Some(report.clone()))
            .await,
        403
    );
    assert_eq!(f.send(Method::GET, "/v1/client", &token, None).await, 404);
    let person = f.server.token.to_string();
    assert_eq!(
        f.send(Method::PUT, "/v1/client", &person, Some(report))
            .await,
        200
    );
    assert_eq!(f.send(Method::GET, "/v1/client", &token, None).await, 200);
    assert_eq!(f.send(Method::GET, "/v1/server", &token, None).await, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_moebius_agent_may_block_and_unblock_a_card_but_only_a_person_deletes_one() {
    let f = Fixture::start().await;
    let store = &f.server.app.store;
    let repository = store
        .add_repository("demo", "/srv/demo", "none", "main")
        .unwrap();
    let card = store
        .add_card(repository.id, "Work", "", "leon", "Backlog")
        .unwrap();
    let (run, token) = f.run("Moebius");
    let (_, coder) = f.run("Coder");
    let card_path = format!("/v1/cards/{}", card.id);

    let block = json!({ "reason": "needs a design" });
    assert_eq!(
        f.send(
            Method::POST,
            &format!("{card_path}/block"),
            &token,
            Some(block)
        )
        .await,
        200
    );
    let held = store.card(card.id).unwrap().unwrap().held.unwrap();
    assert_eq!(
        (held.by.as_str(), held.reason.as_deref()),
        ("agent:Moebius", Some("needs a design"))
    );
    let blocks = store.judgements("block_card").unwrap();
    assert_eq!(
        (blocks[0].decider.as_str(), blocks[0].run_id),
        ("agent:Moebius", Some(run))
    );
    assert_eq!(
        f.send(Method::POST, &format!("{card_path}/unblock"), &token, None)
            .await,
        200
    );
    assert_eq!(store.card(card.id).unwrap().unwrap().held, None);
    assert_eq!(store.judgements("unblock_card").unwrap().len(), 1);

    // Deleting stays with a person; another agent's token is no token at all.
    assert_eq!(f.send(Method::DELETE, &card_path, &token, None).await, 403);
    assert_eq!(f.send(Method::DELETE, &card_path, &coder, None).await, 401);
    assert!(store.card(card.id).unwrap().is_some());
    assert!(store.judgements("delete_card").unwrap().is_empty());
    let person = f.server.token.to_string();
    assert_eq!(f.send(Method::DELETE, &card_path, &person, None).await, 200);
    assert_eq!(store.card(card.id).unwrap(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_moebius_agent_uses_the_tracker_routes_and_no_other_agent_does() {
    let f = Fixture::start().await;
    let store = &f.server.app.store;
    let repository = store
        .add_repository("demo", "/srv/demo", "none", "main")
        .unwrap();
    let issue = |title: &str| {
        store
            .add_card(repository.id, title, "", "person", "Backlog")
            .unwrap()
    };
    let (map, ticket, other) = (issue("Map"), issue("Ticket"), issue("Other"));
    let (run, moebius) = f.run("Moebius");
    let (_, coder) = f.run("Coder");
    let path =
        |card: &moebius_core::store::Card, tail: &str| format!("/v1/cards/{}{tail}", card.id);
    let routes = |token: &str| {
        vec![
            (
                Method::GET,
                format!("/v1/repositories/{}/issues", repository.id),
                None,
            ),
            (Method::GET, path(&ticket, ""), None),
            (
                Method::PUT,
                path(&ticket, "/parent"),
                Some(json!({ "parent": map.number })),
            ),
            (
                Method::PUT,
                path(&ticket, "/labels"),
                Some(json!({ "labels": ["wayfinder:spike"] })),
            ),
            (
                Method::POST,
                path(&ticket, "/blockers"),
                Some(json!({ "blocker": other.number })),
            ),
            (
                Method::DELETE,
                path(&ticket, &format!("/blockers/{}", other.number)),
                None,
            ),
            (
                Method::POST,
                path(&ticket, "/claim"),
                Some(json!({ "by": "session 1" })),
            ),
            (
                Method::POST,
                path(&ticket, "/comments"),
                Some(json!({ "body": "Use a queue." })),
            ),
            (Method::GET, path(&ticket, "/comments"), None),
            (
                Method::DELETE,
                path(&ticket, "/claim"),
                Some(json!({ "by": "session 1" })),
            ),
            (
                Method::POST,
                path(&ticket, "/close"),
                Some(json!({ "reason": "decided" })),
            ),
            (Method::POST, path(&ticket, "/reopen"), Some(json!({}))),
            (
                Method::POST,
                format!("/v1/repositories/{}/issues", repository.id),
                Some(json!({ "title": token, "parent": map.number })),
            ),
        ]
    };

    // A coding run's token is no token for any of them, and changes nothing.
    for (method, path, body) in routes("by a coder") {
        assert_eq!(f.send(method, &path, &coder, body).await, 401, "{path}");
    }
    assert_eq!(store.card(ticket.id).unwrap().unwrap().parent, None);
    assert_eq!(store.cards(repository.id).unwrap().len(), 3);

    for (method, path, body) in routes("by the agent") {
        assert_eq!(
            f.send(method.clone(), &path, &moebius, body).await,
            200,
            "{method} {path}"
        );
    }
    let tracked = store.card(ticket.id).unwrap().unwrap();
    assert_eq!(tracked.parent, Some(map.number));
    assert_eq!(tracked.labels, ["wayfinder:spike"]);
    assert_eq!(
        store.comments(ticket.id).unwrap()[0].author,
        "agent:Moebius"
    );
    let closed = store.judgements("close_issue").unwrap();
    assert_eq!(
        (closed[0].decider.as_str(), closed[0].run_id),
        ("agent:Moebius", Some(run))
    );
    assert_eq!(store.cards(repository.id).unwrap().len(), 4);

    // It does not release the claim of somebody else, though a person does.
    let person = f.server.token.to_string();
    let claim = path(&other, "/claim");
    assert_eq!(
        f.send(
            Method::POST,
            &claim,
            &person,
            Some(json!({ "by": "session 9" }))
        )
        .await,
        200
    );
    assert_eq!(f.send(Method::DELETE, &claim, &moebius, None).await, 403);
    assert_eq!(f.send(Method::DELETE, &claim, &person, None).await, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_grilling_partner_keeps_a_wayfinder_map_with_the_tracker_routes_and_nothing_else() {
    let f = Fixture::start().await;
    let store = &f.server.app.store;
    let repository = store
        .add_repository("demo", "/srv/demo", "none", "main")
        .unwrap();
    let (run, grilling) = f.grilling(repository.id);
    let (_, reviewer) = f.run("Reviewer");
    let issues = format!("/v1/repositories/{}/issues", repository.id);
    let person = f.server.token.to_string();
    let call = |method: Method, path: String, token: String, body: Option<Value>| {
        let client = f.client.clone();
        let url = format!("{}{path}", f.api);
        async move {
            let mut request = client.request(method, url).bearer_auth(token);
            if let Some(body) = body {
                request = request.json(&body);
            }
            let response = request.send().await.unwrap();
            let status = response.status().as_u16();
            (
                status,
                response.json::<Value>().await.unwrap_or(Value::Null),
            )
        }
    };

    // The map and a ticket under it, as the wayfinder skill creates them.
    let (status, map) = call(
        Method::POST,
        issues.clone(),
        grilling.clone(),
        Some(json!({ "title": "Map", "body": "## Destination", "labels": ["wayfinder:map"] })),
    )
    .await;
    assert_eq!(status, 200, "{map}");
    let map_id = map["id"].as_i64().unwrap();
    let map_number = map["number"].as_i64().unwrap();
    let (status, ticket) = call(
        Method::POST,
        issues.clone(),
        grilling.clone(),
        Some(json!({ "title": "Which queue?", "parent": map_number, "labels": ["wayfinder:grilling"] })),
    )
    .await;
    assert_eq!(status, 200, "{ticket}");
    let ticket_id = ticket["id"].as_i64().unwrap();
    let ticket_path = format!("/v1/cards/{ticket_id}");

    // What an issue created through Moebius is: a sub-issue that counts, so the map shows its ticket and the
    // frontier lists it.
    let (_, shown) = call(
        Method::GET,
        format!("/v1/cards/{map_id}"),
        grilling.clone(),
        None,
    )
    .await;
    assert_eq!(shown["children"].as_array().unwrap().len(), 1, "{shown}");
    let (_, listed) = call(
        Method::GET,
        format!("{issues}?parent={map_number}&state=open&claimed=false&blocked=false"),
        grilling.clone(),
        None,
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");

    let routes = vec![
        (Method::GET, ticket_path.clone(), None),
        (
            Method::PUT,
            ticket_path.clone(),
            Some(json!({ "title": "Which queue?", "body": "Edited." })),
        ),
        (
            Method::PUT,
            format!("{ticket_path}/parent"),
            Some(json!({ "parent": map_number })),
        ),
        (
            Method::PUT,
            format!("{ticket_path}/labels"),
            Some(json!({ "labels": ["wayfinder:research"] })),
        ),
        (
            Method::POST,
            format!("{ticket_path}/blockers"),
            Some(json!({ "blocker": map_number + 5 })),
        ),
        (
            Method::POST,
            format!("{ticket_path}/claim"),
            Some(json!({ "by": "session 1" })),
        ),
        (
            Method::POST,
            format!("{ticket_path}/comments"),
            Some(json!({ "body": "Use a queue." })),
        ),
        (Method::GET, format!("{ticket_path}/comments"), None),
        (
            Method::DELETE,
            format!("{ticket_path}/claim"),
            Some(json!({ "by": "session 1" })),
        ),
        (
            Method::POST,
            format!("{ticket_path}/close"),
            Some(json!({ "reason": "decided" })),
        ),
        (
            Method::POST,
            format!("{ticket_path}/reopen"),
            Some(json!({})),
        ),
    ];
    for (method, path, body) in routes {
        let expected = if path.ends_with("/blockers") {
            404
        } else {
            200
        };
        let (status, answer) = call(method.clone(), path.clone(), grilling.clone(), body).await;
        assert_eq!(status, expected, "{method} {path}: {answer}");
    }
    // A blocker that exists is added and removed too.
    let blocker = format!("{ticket_path}/blockers");
    let (status, _) = call(
        Method::POST,
        blocker.clone(),
        grilling.clone(),
        Some(json!({ "blocker": map_number })),
    )
    .await;
    assert_eq!(
        status, 422,
        "a blocker that is the ticket's parent makes a cycle"
    );
    let (status, other) = call(
        Method::POST,
        issues.clone(),
        grilling.clone(),
        Some(json!({ "title": "Other", "labels": ["wayfinder:spike"] })),
    )
    .await;
    assert_eq!(status, 200);
    let other_number = other["number"].as_i64().unwrap();
    let (status, _) = call(
        Method::POST,
        blocker.clone(),
        grilling.clone(),
        Some(json!({ "blocker": other_number })),
    )
    .await;
    assert_eq!(status, 200);
    let (status, _) = call(
        Method::DELETE,
        format!("{blocker}/{other_number}"),
        grilling.clone(),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        store.comments(ticket_id).unwrap()[0].author,
        "agent:Grilling-partner"
    );
    let closed = store.judgements("close_issue").unwrap();
    assert_eq!(
        (closed[0].decider.as_str(), closed[0].run_id),
        ("agent:Grilling-partner", Some(run))
    );

    // Nothing else: not deleting an issue, not the settings, the snapshot, the harness, the keys or the chats.
    for (method, path, body) in [
        (Method::DELETE, ticket_path.clone(), None),
        (Method::GET, "/v1/snapshot".to_string(), None),
        (Method::GET, "/v1/health".to_string(), None),
        (Method::GET, "/v1/settings".to_string(), None),
        (
            Method::PUT,
            "/v1/settings".to_string(),
            Some(json!({ "set": { "board.round_limit": 7 } })),
        ),
        (
            Method::POST,
            format!("{ticket_path}/block"),
            Some(json!({ "reason": "x" })),
        ),
        (
            Method::POST,
            format!("{ticket_path}/approve"),
            Some(json!({})),
        ),
        (
            Method::PUT,
            "/v1/secrets/anthropic".to_string(),
            Some(json!({ "value": "sk-test" })),
        ),
        (Method::GET, "/v1/chats".to_string(), None),
        (
            Method::GET,
            format!("/v1/repositories/{}/settings", repository.id),
            None,
        ),
        (
            Method::GET,
            format!("/v1/repositories/{}/issues/x", repository.id),
            None,
        ),
        (Method::GET, "/v1/cards/x".to_string(), None),
    ] {
        let (status, _) = call(method.clone(), path.clone(), grilling.clone(), body).await;
        assert_eq!(status, 401, "{method} {path}");
    }
    assert!(store.card(ticket_id).unwrap().is_some());
    assert!(store.judgements("edit_settings").unwrap().is_empty());

    // Another agent's token still gets nothing, and a person's is unchanged.
    let (status, _) = call(Method::GET, issues.clone(), reviewer, None).await;
    assert_eq!(status, 401);
    let (status, _) = call(Method::GET, issues, person, None).await;
    assert_eq!(status, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_chats_research_grillings_exports_and_replays_stays_with_a_person() {
    let f = Fixture::start().await;
    let (_, token) = f.run("Moebius");
    let store = &f.server.app.store;
    let (chat, _) = f.run("Coder");
    let (grilled, _) = f.run("Grilling-partner");
    for run in [chat, grilled] {
        store.set_run_status(run, "stopped").unwrap();
    }
    let repo = store
        .add_repository("demo", "/tmp/demo", "none", "main")
        .unwrap();
    let research = store
        .add_research(
            repo.id,
            "goal",
            "cat x",
            "lower",
            moebius_core::store::Budget {
                kind: moebius_core::store::BudgetKind::Tokens,
                amount: 1,
            },
        )
        .unwrap();
    let grilling = store
        .add_grilling(grilled, repo.id, &[], None, "empty", "")
        .unwrap();
    let paths = [
        format!("/v1/chats/{chat}"),
        format!("/v1/research/{}", research.id),
        format!("/v1/grillings/{}", grilling.id),
        "/v1/exports/1".to_string(),
        "/v1/replays/1".to_string(),
    ];
    for path in &paths {
        assert_eq!(
            f.send(Method::DELETE, path, &token, None).await,
            403,
            "{path}"
        );
    }
    assert!(store.run(chat).unwrap().is_some());
    assert!(store.research(research.id).unwrap().is_some());
    assert!(store.grilling(grilling.id).unwrap().is_some());
    // A person's request reaches the thing: the chat and the others are deleted, the missing are not found.
    let person = f.server.token.to_string();
    for (path, status) in paths.iter().zip([200, 200, 200, 404, 404]) {
        assert_eq!(
            f.send(Method::DELETE, path, &person, None).await,
            status,
            "{path}"
        );
    }
}

/// A Grilling partner's world for the scope tests: two repositories, a wayfinder map and ticket and a plain card in the
/// one its chat is about, and a card in the other.
struct Scoped {
    f: Fixture,
    token: String,
    repository: i64,
    elsewhere: i64,
    map: moebius_core::store::Card,
    ticket: moebius_core::store::Card,
    plain: moebius_core::store::Card,
    foreign: moebius_core::store::Card,
}

impl Scoped {
    async fn start() -> Self {
        let f = Fixture::start().await;
        let store = &f.server.app.store;
        let repository = store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let other = store
            .add_repository("other", "/srv/other", "none", "main")
            .unwrap();
        let card = |repository: i64, title: &str, labels: &[&str]| {
            let card = store
                .add_card(repository, title, "", "person", "Backlog")
                .unwrap();
            let labels = labels.iter().map(|l| l.to_string()).collect();
            store.set_labels(card.id, &labels).unwrap();
            store.card(card.id).unwrap().unwrap()
        };
        let map = card(repository.id, "Map", &["wayfinder:map"]);
        let ticket = card(repository.id, "Ticket", &["Wayfinder:Spike"]);
        let plain = card(repository.id, "Plain", &["bug"]);
        let foreign = card(other.id, "Foreign", &["wayfinder:map"]);
        let (_, token) = f.grilling(repository.id);
        Self {
            f,
            token,
            repository: repository.id,
            elsewhere: other.id,
            map,
            ticket,
            plain,
            foreign,
        }
    }

    async fn call(&self, method: Method, path: String, body: Option<Value>) -> u16 {
        self.f.send(method, &path, &self.token, body).await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_grilling_partner_is_held_to_the_repository_of_its_chat() {
    let s = Scoped::start().await;
    let (foreign, elsewhere) = (s.foreign.id, s.elsewhere);
    let labelled = json!({ "title": "New", "labels": ["wayfinder:map"] });
    // Another repository is answered as if it were not there, and so are its cards, for every route.
    for (method, path, body) in [
        (
            Method::POST,
            format!("/v1/repositories/{elsewhere}/issues"),
            Some(labelled.clone()),
        ),
        (
            Method::GET,
            format!("/v1/repositories/{elsewhere}/issues"),
            None,
        ),
        (Method::GET, format!("/v1/cards/{foreign}"), None),
        (Method::GET, format!("/v1/cards/{foreign}/comments"), None),
        (
            Method::PUT,
            format!("/v1/cards/{foreign}"),
            Some(json!({ "title": "x", "body": "y" })),
        ),
        (
            Method::POST,
            format!("/v1/cards/{foreign}/close"),
            Some(json!({})),
        ),
        (
            Method::PUT,
            format!("/v1/cards/{foreign}/labels"),
            Some(json!({ "labels": ["wayfinder:map"] })),
        ),
        (
            Method::POST,
            format!("/v1/cards/{foreign}/comments"),
            Some(json!({ "body": "x" })),
        ),
    ] {
        assert_eq!(
            s.call(method.clone(), path.clone(), body).await,
            404,
            "{method} {path}"
        );
    }
    // A card that does not exist is answered the same.
    assert_eq!(
        s.call(Method::GET, "/v1/cards/9999".into(), None).await,
        404
    );
    assert!(s.f.server.app.store.comments(foreign).unwrap().is_empty());
    // Its own repository works, and a person is not held to it.
    assert_eq!(
        s.call(
            Method::POST,
            format!("/v1/repositories/{}/issues", s.repository),
            Some(labelled.clone())
        )
        .await,
        200
    );
    let person = s.f.server.token.to_string();
    assert_eq!(
        s.f.send(
            Method::POST,
            &format!("/v1/repositories/{elsewhere}/issues"),
            &person,
            Some(json!({ "title": "New" }))
        )
        .await,
        200
    );
    // A run in no repository has no issues at all.
    let (_, lost) = s.f.run("Grilling-partner");
    assert_eq!(
        s.f.send(
            Method::GET,
            &format!("/v1/repositories/{}/issues", s.repository),
            &lost,
            None
        )
        .await,
        403
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_grilling_partner_creates_and_changes_wayfinder_cards_only() {
    let s = Scoped::start().await;
    let issues = format!("/v1/repositories/{}/issues", s.repository);
    let (plain, ticket, map) = (s.plain.id, s.ticket.id, s.map.id);

    // Creating: a wayfinder label is needed (in any case), and a parent or blocker is a wayfinder card.
    for body in [
        json!({ "title": "No labels" }),
        json!({ "title": "Other labels", "labels": ["bug"] }),
        json!({ "title": "Child of a task", "labels": ["wayfinder:spike"], "parent": s.plain.number }),
        json!({ "title": "Blocked by a task", "labels": ["wayfinder:spike"], "blocked_by": [s.plain.number] }),
    ] {
        assert_eq!(
            s.call(Method::POST, issues.clone(), Some(body.clone()))
                .await,
            403,
            "{body}"
        );
    }
    assert_eq!(s.f.server.app.store.cards(s.repository).unwrap().len(), 3);
    let ok = json!({ "title": "Ok", "labels": ["WAYFINDER:Research"], "parent": s.map.number, "blocked_by": [s.ticket.number] });
    assert_eq!(s.call(Method::POST, issues.clone(), Some(ok)).await, 200);

    // A card that is no part of a wayfinder map can be read but not changed.
    assert_eq!(
        s.call(Method::GET, format!("/v1/cards/{plain}"), None)
            .await,
        200
    );
    assert_eq!(
        s.call(Method::GET, format!("/v1/cards/{plain}/comments"), None)
            .await,
        200
    );
    for (method, tail, body) in [
        (Method::PUT, "", Some(json!({ "title": "x", "body": "y" }))),
        (Method::POST, "/close", Some(json!({}))),
        (Method::POST, "/reopen", Some(json!({}))),
        (Method::POST, "/claim", Some(json!({ "by": "1" }))),
        (Method::DELETE, "/claim", Some(json!({ "by": "1" }))),
        (Method::POST, "/comments", Some(json!({ "body": "x" }))),
        (
            Method::POST,
            "/blockers",
            Some(json!({ "blocker": s.map.number })),
        ),
        (
            Method::PUT,
            "/parent",
            Some(json!({ "parent": s.map.number })),
        ),
        (
            Method::PUT,
            "/labels",
            Some(json!({ "labels": ["wayfinder:map"] })),
        ),
    ] {
        let path = format!("/v1/cards/{plain}{tail}");
        assert_eq!(
            s.call(method.clone(), path.clone(), body).await,
            403,
            "{method} {path}"
        );
    }
    let card = s.f.server.app.store.card(plain).unwrap().unwrap();
    assert_eq!(
        (card.labels.clone(), card.parent, card.closed),
        (vec!["bug".to_string()], None, false)
    );

    // On a wayfinder card the relations stay inside wayfinder, and its last wayfinder label stays.
    let path = |tail: &str| format!("/v1/cards/{ticket}{tail}");
    assert_eq!(
        s.call(
            Method::PUT,
            path("/parent"),
            Some(json!({ "parent": s.plain.number }))
        )
        .await,
        403
    );
    assert_eq!(
        s.call(
            Method::POST,
            path("/blockers"),
            Some(json!({ "blocker": s.plain.number }))
        )
        .await,
        403
    );
    assert_eq!(
        s.call(
            Method::PUT,
            path("/labels"),
            Some(json!({ "labels": ["bug"] }))
        )
        .await,
        403
    );
    assert_eq!(
        s.call(Method::PUT, path("/labels"), Some(json!({ "labels": [] })))
            .await,
        403
    );
    assert_eq!(
        s.call(
            Method::PUT,
            path("/parent"),
            Some(json!({ "parent": s.map.number }))
        )
        .await,
        200
    );
    assert_eq!(
        s.call(
            Method::PUT,
            path("/labels"),
            Some(json!({ "labels": ["bug", "wayfinder:research"] }))
        )
        .await,
        200
    );
    assert_eq!(
        s.call(
            Method::POST,
            format!("/v1/cards/{map}/close"),
            Some(json!({}))
        )
        .await,
        200
    );

    // A person is not held to any of it.
    let person = s.f.server.token.to_string();
    assert_eq!(
        s.f.send(
            Method::PUT,
            &format!("/v1/cards/{plain}/labels"),
            &person,
            Some(json!({ "labels": [] }))
        )
        .await,
        200
    );
}
