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
