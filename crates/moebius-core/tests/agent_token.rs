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
async fn keys_and_shells_stay_with_a_person() {
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
