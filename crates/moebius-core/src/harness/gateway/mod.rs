//! The MCP gateway (spec 4): one MCP server for every harness, which bundles the MCP servers of the run's agent. It is
//! served on the router's listener as `POST /mcp` (streamable HTTP, answered with JSON), authenticated by the run's
//! token like the router and the check; an unknown or missing token gets 401. A run's `mcp.json` names only the gateway
//! (see `mcp::pi_config`), and the servers the run may use are the ones its token was given (`Router::allow_mcp`).
//!
//! At the run's first request the gateway starts its stdio servers, as child processes of the Moebius server working
//! in the run's worktree, and connects to its remote ones (see `upstream`), and lists their tools under the server's
//! name, as `<server>__<tool>`. A server that cannot be reached leaves out its tools, with an `mcp_error` run event.
//! Every call of a tool is checked like any other tool call, through `decide("tool_call")` with the tool named
//! `mcp__<server>__<tool>`: a refused call never reaches the server and its result is the refusal, naming the rule.
//! What comes back has its secrets removed before the harness sees it, and the call is recorded as an `mcp_call` run
//! event. The servers of a run are stopped when the run ends.

mod upstream;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::Router as Routes;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};
use tokio::sync::OnceCell;
use tokio::sync::broadcast::error::RecvError;

use crate::app::App;
use crate::decide::check::{self, CheckRequest};
use crate::harness::mcp::McpServer;
use crate::router::{Grant, Router, token};
use upstream::Upstream;

/// How long a tool of a server may take to answer a call.
const CALL_LIMIT: Duration = Duration::from_secs(10 * 60);

/// Between a server's name and its tool's in the names the gateway gives the tools.
const SEPARATOR: &str = "__";

/// The MCP gateway: the servers of every run that has asked for them.
pub struct Gateway {
    app: Arc<App>,
    router: Arc<Router>,
    rules_file: Option<PathBuf>,
    /// By the run's token.
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

/// The servers of one run, started or connected at its first request.
struct Session {
    run_id: i64,
    servers: BTreeMap<String, McpServer>,
    opened: OnceCell<Opened>,
}

/// The servers of a run that could be reached, and their tools.
struct Opened {
    upstreams: BTreeMap<String, Upstream>,
    tools: Vec<Tool>,
}

/// A tool of a server, as the gateway lists it.
struct Tool {
    /// `<server>__<tool>`.
    name: String,
    server: String,
    tool: String,
    /// The server's description of the tool, with the gateway's name.
    definition: Value,
}

impl Gateway {
    /// The gateway for the runs whose tokens `router` issued; their tool calls are checked by the rules of
    /// `rules_file`, the server's rules file, and the repositories'.
    pub fn new(app: Arc<App>, router: Arc<Router>, rules_file: Option<PathBuf>) -> Self {
        Self {
            app,
            router,
            rules_file,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn routes(self: Arc<Self>) -> Routes {
        Routes::new()
            .route("/mcp", post(serve).get(not_offered).delete(not_offered))
            .with_state(self)
    }

    /// Stops the servers of every run that ends, until the bus closes.
    pub async fn stop_ended_runs(self: Arc<Self>) {
        let mut bus = self.app.bus.subscribe();
        loop {
            match bus.recv().await {
                Ok(event) if event.kind == "run_ended" => self.stop_ended(),
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => self.stop_ended(),
                Err(RecvError::Closed) => return,
            }
        }
    }

    /// Drops the servers of the runs whose tokens were withdrawn, which stops them.
    fn stop_ended(&self) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        sessions.retain(|token, _| self.router.mcp_grant(token).is_some());
    }

    /// The session of the run of `token`, which may use `servers`; a new one when the run was given other servers
    /// since.
    fn session(
        &self,
        token: &str,
        run_id: i64,
        servers: BTreeMap<String, McpServer>,
    ) -> Arc<Session> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        match sessions.get(token) {
            Some(session) if session.servers == servers => session.clone(),
            _ => {
                let session = Arc::new(Session {
                    run_id,
                    servers,
                    opened: OnceCell::new(),
                });
                sessions.insert(token.to_string(), session.clone());
                session
            }
        }
    }

    /// How many runs have their servers going, for tests.
    #[cfg(test)]
    fn running(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }
}

impl Session {
    /// Starts or connects to the run's servers once; a server that cannot be reached is left out with an `mcp_error`
    /// run event.
    async fn open(&self, app: &App) -> &Opened {
        self.opened
            .get_or_init(|| async {
                let worktree = app
                    .store
                    .run(self.run_id)
                    .ok()
                    .flatten()
                    .and_then(|run| run.worktree)
                    .map(PathBuf::from);
                let connecting = self.servers.iter().map(|(name, server)| {
                    let worktree = worktree.as_deref();
                    async move { (name, upstream::connect(server, worktree).await) }
                });
                let mut opened = Opened {
                    upstreams: BTreeMap::new(),
                    tools: vec![],
                };
                for (name, connected) in futures_util::future::join_all(connecting).await {
                    match connected {
                        Ok((upstream, tools)) => {
                            opened
                                .tools
                                .extend(tools.into_iter().filter_map(|definition| {
                                    let tool = definition["name"].as_str()?.to_string();
                                    let exposed = format!("{name}{SEPARATOR}{tool}");
                                    let mut definition = definition;
                                    definition["name"] = json!(exposed);
                                    Some(Tool {
                                        name: exposed,
                                        server: name.clone(),
                                        tool,
                                        definition,
                                    })
                                }));
                            opened.upstreams.insert(name.clone(), upstream);
                        }
                        Err(message) => {
                            let event = json!({ "server": name, "message": message });
                            let _ = app.record_run_event(self.run_id, "mcp_error", &event);
                        }
                    }
                }
                opened
            })
            .await
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "not a run's token" })),
    )
        .into_response()
}

/// The gateway answers every request with JSON and offers no stream of its own, nor ends sessions on request.
async fn not_offered(State(gateway): State<Arc<Gateway>>, headers: HeaderMap) -> Response {
    match token(&headers).and_then(|t| gateway.router.mcp_grant(t)) {
        None => unauthorized(),
        Some(_) => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

/// One JSON-RPC message, or a batch of them: requests are answered, notifications and responses taken with 202.
async fn serve(State(gateway): State<Arc<Gateway>>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(token) = token(&headers) else {
        return unauthorized();
    };
    let Some((grant, servers)) = gateway.router.mcp_grant(token) else {
        return unauthorized();
    };
    gateway.stop_ended();
    let session = gateway.session(token, grant.run_id, servers);
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        let error = rpc_error(&Value::Null, -32700, "the body is not JSON");
        return (StatusCode::BAD_REQUEST, Json(error)).into_response();
    };
    let call = Call {
        gateway: &gateway,
        grant: &grant,
        session: &session,
    };
    match message {
        Value::Array(batch) => {
            let mut answers = vec![];
            for message in &batch {
                answers.extend(call.answer(message).await);
            }
            if answers.is_empty() {
                StatusCode::ACCEPTED.into_response()
            } else {
                Json(Value::Array(answers)).into_response()
            }
        }
        message => match call.answer(&message).await {
            Some(answer) => Json(answer).into_response(),
            None => StatusCode::ACCEPTED.into_response(),
        },
    }
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// A request of a run, with what it may use.
struct Call<'a> {
    gateway: &'a Gateway,
    grant: &'a Grant,
    session: &'a Session,
}

impl Call<'_> {
    /// The answer to `message`, or none for a notification or a response.
    async fn answer(&self, message: &Value) -> Option<Value> {
        let id = message.get("id").filter(|id| !id.is_null())?;
        let method = message["method"].as_str()?;
        let params = &message["params"];
        let result = match method {
            "initialize" => {
                self.session.open(&self.gateway.app).await;
                let version = params["protocolVersion"].as_str().unwrap_or("2025-06-18");
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "moebius", "version": env!("CARGO_PKG_VERSION") },
                })
            }
            "ping" => json!({}),
            "tools/list" => {
                let opened = self.session.open(&self.gateway.app).await;
                let tools: Vec<&Value> = opened.tools.iter().map(|t| &t.definition).collect();
                json!({ "tools": tools })
            }
            "tools/call" => match self.call_tool(id, params).await {
                Ok(result) => result,
                Err(message) => return Some(rpc_error(id, -32602, &message)),
            },
            _ => return Some(rpc_error(id, -32601, &format!("no method {method:?}"))),
        };
        Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    /// Checks a call of a tool and, when it is allowed, makes it; the result has its secrets removed, and the call is
    /// recorded as an `mcp_call` run event. `Err` for a tool the run does not have.
    async fn call_tool(&self, id: &Value, params: &Value) -> Result<Value, String> {
        let name = params["name"].as_str().unwrap_or_default();
        let opened = self.session.open(&self.gateway.app).await;
        let tool = opened
            .tools
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| format!("no tool {name:?}"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let id = match id {
            Value::String(id) => id.clone(),
            id => id.to_string(),
        };
        let request = CheckRequest {
            tool: format!("mcp__{}__{}", tool.server, tool.tool),
            input: arguments.clone(),
            tool_call_id: format!("mcp-{id}"),
        };
        let (decision, reason) = self.decide(request).await;
        let result = if decision == "allow" {
            let params = json!({ "name": tool.tool, "arguments": arguments });
            match opened.upstreams[&tool.server]
                .request("tools/call", params, CALL_LIMIT)
                .await
            {
                Ok(result) => crate::blobs::redact_json(&result),
                Err(message) => failure(&format!("the MCP server {:?}: {message}", tool.server)),
            }
        } else {
            failure(&format!("Moebius refused this call: {reason}"))
        };
        let event = json!({
            "server": tool.server,
            "tool": tool.tool,
            "tool_call_id": format!("mcp-{id}"),
            "input": arguments,
            "decision": decision,
            "reason": reason,
            "result": result,
        });
        let _ = self
            .gateway
            .app
            .record_run_event(self.grant.run_id, "mcp_call", &event);
        Ok(result)
    }

    /// The decision on a call; anything that fails while deciding refuses it.
    async fn decide(&self, request: CheckRequest) -> (String, String) {
        let app = self.gateway.app.clone();
        let rules_file = self.gateway.rules_file.clone();
        let grant = self.grant.clone();
        let decided = tokio::task::spawn_blocking(move || {
            check::decide_call(&app, rules_file.as_deref().map(Path::new), &grant, &request)
        })
        .await;
        match decided {
            Ok(Some(reply)) => (reply.decision, reply.reason),
            _ => (
                "refuse".into(),
                "refused: the call could not be decided".into(),
            ),
        }
    }
}

/// A tool result that says the call did not happen, or failed, and why.
fn failure(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use moebius_plugin_api::{
        Call as PluginCall, DecideRequest, Host, Kind, Manifest, Plugin, PluginError,
    };

    use super::*;
    use crate::bus::AppEvent;
    use crate::harness::mcp::parse_file;

    /// Allows a call whose input says hello, and refuses every other naming a rule.
    struct Rules;
    impl Plugin for Rules {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "rules".into(),
                version: "0".into(),
                kinds: vec![Kind::Decider],
            }
        }
        fn call(&self, call: PluginCall) -> Result<Value, PluginError> {
            let r: DecideRequest = serde_json::from_value(call.params).unwrap();
            Ok(if r.context["input"].contains("hello") {
                json!({"choice": "allow", "reason": "rule friendly: greetings are harmless"})
            } else {
                json!({"choice": "refuse", "reason": "rule hello-only: say hello first"})
            })
        }
    }

    struct Setup {
        _dir: tempfile::TempDir,
        app: Arc<App>,
        router: Arc<Router>,
        gateway: Arc<Gateway>,
        url: String,
        token: String,
        run: i64,
    }

    /// A gateway on port 0 and a run whose token may use `servers` (the tables of an `mcp.toml`).
    async fn setup(servers: &str) -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::store::Store::in_memory(dir.path()).unwrap());
        let run = store.add_run(None, "Coder", "pi-bolt", "code").unwrap();
        store
            .set_run_worktree(run.id, &dir.path().to_string_lossy())
            .unwrap();
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Rules);
        let app = App::new(store.clone(), Arc::new(host), dir.path());
        app.decisions.route("tool_call", "rules");
        let app = Arc::new(app);
        let router = Arc::new(Router::new(store, vec![]));
        let token = router.issue_token(Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        });
        let servers = parse_file(servers, &[])
            .unwrap()
            .into_iter()
            .map(|(name, server)| (name, server.unwrap()))
            .collect();
        router.allow_mcp(&token, servers);
        let gateway = Arc::new(Gateway::new(app.clone(), router.clone(), None));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let served = gateway.clone().routes();
        tokio::spawn(async move { axum::serve(listener, served).await });
        Setup {
            _dir: dir,
            app,
            router,
            gateway,
            url,
            token,
            run: run.id,
        }
    }

    fn echo() -> String {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/echo-mcp");
        format!("[echo]\ncommand = {:?}\n", script.display().to_string())
    }

    impl Setup {
        async fn rpc(&self, method: &str, params: Value) -> Value {
            let message = json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params });
            let response = reqwest::Client::new()
                .post(&self.url)
                .bearer_auth(&self.token)
                .json(&message)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            response.json().await.unwrap()
        }

        async fn call(&self, tool: &str, text: &str) -> Value {
            let params = json!({ "name": tool, "arguments": { "text": text } });
            self.rpc("tools/call", params).await["result"].clone()
        }

        fn events(&self, kind: &str) -> Vec<Value> {
            let events = self.app.store.events(self.run).unwrap();
            events
                .into_iter()
                .filter(|event| event.kind == kind)
                .map(|event| event.data)
                .collect()
        }
    }

    // Unix only: the stdio MCP server is a Python script started by its `#!` line.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_run_uses_a_tool_of_its_stdio_server_through_the_gateway() {
        let s = setup(&echo()).await;
        let init = s
            .rpc("initialize", json!({ "protocolVersion": "2025-03-26" }))
            .await;
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "moebius");
        let notified = reqwest::Client::new()
            .post(&s.url)
            .bearer_auth(&s.token)
            .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .send()
            .await
            .unwrap();
        assert_eq!(notified.status(), 202);

        let tools = s.rpc("tools/list", json!({})).await;
        let tools = tools["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "echo__echo");
        assert_eq!(tools[0]["inputSchema"]["required"], json!(["text"]));

        let result = s.call("echo__echo", "hello").await;
        assert_eq!(result["content"][0]["text"], "echo: hello");
        let calls = s.events("mcp_call");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["server"], "echo");
        assert_eq!(calls[0]["tool"], "echo");
        assert_eq!(calls[0]["decision"], "allow");
        assert_eq!(calls[0]["input"], json!({ "text": "hello" }));
        assert_eq!(calls[0]["result"]["content"][0]["text"], "echo: hello");
        let judgements = s.app.store.judgements("tool_call").unwrap();
        assert_eq!(judgements.len(), 1);
        let context: BTreeMap<String, String> =
            serde_json::from_str(&s.app.store.blobs().get(&judgements[0].context[0]).unwrap())
                .unwrap();
        assert_eq!(context["tool"], "mcp__echo__echo");

        // Secrets in what the server answers reach neither the harness nor the store.
        let key = format!("sk-ant-{}", "k".repeat(30));
        let result = s.call("echo__echo", &format!("hello {key}")).await;
        let text = result["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "echo: hello [redacted:anthropic-key]");
        let stored = serde_json::to_string(&s.events("mcp_call")).unwrap();
        assert!(!stored.contains(&key), "{stored}");

        let unknown = s.rpc("tools/call", json!({ "name": "echo__nope" })).await;
        assert_eq!(unknown["error"]["code"], -32602);
        let other = s.rpc("resources/list", json!({})).await;
        assert_eq!(other["error"]["code"], -32601);
    }

    /// A remote MCP server on port 0 with one tool, `echo`, that counts the calls of it. It answers `initialize` with
    /// JSON and a session, and everything else with an event stream, and refuses requests without the session.
    async fn remote(calls: Arc<AtomicUsize>) -> String {
        let answer = move |headers: HeaderMap, Json(message): Json<Value>| {
            let calls = calls.clone();
            async move {
                let Some(id) = message.get("id").cloned() else {
                    return StatusCode::ACCEPTED.into_response();
                };
                let method = message["method"].as_str().unwrap_or_default();
                if method == "initialize" {
                    let result = json!({ "protocolVersion": "2025-06-18", "capabilities": {},
                        "serverInfo": { "name": "remote", "version": "1" } });
                    let answer = json!({ "jsonrpc": "2.0", "id": id, "result": result });
                    return ([("mcp-session-id", "s-1")], Json(answer)).into_response();
                }
                if headers.get("mcp-session-id").is_none_or(|v| v != "s-1")
                    || headers.get("x-key").is_none_or(|v| v != "k")
                {
                    return StatusCode::BAD_REQUEST.into_response();
                }
                let result = match method {
                    "tools/list" => json!({ "tools": [{ "name": "echo",
                        "inputSchema": { "type": "object" } }] }),
                    _ => {
                        calls.fetch_add(1, Ordering::SeqCst);
                        let text = message["params"]["arguments"]["text"].as_str().unwrap();
                        json!({ "content": [{ "type": "text", "text": format!("remote: {text}") }] })
                    }
                };
                let ping = json!({ "jsonrpc": "2.0", "method": "notifications/progress" });
                let answer = json!({ "jsonrpc": "2.0", "id": id, "result": result });
                let body =
                    format!("event: message\ndata: {ping}\n\nevent: message\ndata: {answer}\n\n");
                ([("content-type", "text/event-stream")], body).into_response()
            }
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let routes = Routes::new().route("/mcp", post(answer));
        tokio::spawn(async move { axum::serve(listener, routes).await });
        url
    }

    #[tokio::test]
    async fn a_refused_call_never_reaches_the_server() {
        let calls = Arc::new(AtomicUsize::new(0));
        let url = remote(calls.clone()).await;
        let s = setup(&format!(
            "[remote]\nurl = {url:?}\nheaders = {{ x-key = \"k\" }}\n"
        ))
        .await;
        let refused = s.call("remote__echo", "goodbye").await;
        assert_eq!(refused["isError"], true);
        let text = refused["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("rule hello-only"), "{text}");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let checked = s.events("mcp_call");
        assert_eq!(checked[0]["decision"], "refuse");
        assert!(
            checked[0]["reason"]
                .as_str()
                .unwrap()
                .contains("rule hello-only")
        );

        let allowed = s.call("remote__echo", "hello").await;
        assert_eq!(allowed["content"][0]["text"], "remote: hello");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn an_unknown_or_missing_token_gets_401() {
        let s = setup(&echo()).await;
        let client = reqwest::Client::new();
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
        let requests = [
            client
                .post(&s.url)
                .bearer_auth("moebius-run-nope")
                .json(&body),
            client.post(&s.url).json(&body),
            client.post(&s.url).body("not json"),
            client.get(&s.url).bearer_auth("moebius-run-nope"),
        ];
        for request in requests {
            assert_eq!(request.send().await.unwrap().status(), 401);
        }
        assert_eq!(s.gateway.running(), 0);
        let stream = client.get(&s.url).bearer_auth(&s.token).send().await;
        assert_eq!(stream.unwrap().status(), 405);
    }

    // Unix only: the stdio MCP server is a Python script started by its `#!` line.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_server_that_cannot_start_is_reported_and_the_others_still_serve() {
        let s = setup(&format!(
            "{}[broken]\ncommand = \"/nonexistent/mcp\"\n",
            echo()
        ))
        .await;
        let tools = s.rpc("tools/list", json!({})).await;
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["echo__echo"]);
        let errors = s.events("mcp_error");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0]["server"], "broken");
        assert!(
            errors[0]["message"]
                .as_str()
                .unwrap()
                .contains("cannot start")
        );
    }

    #[tokio::test]
    async fn the_servers_of_a_run_stop_when_it_ends() {
        let s = setup(&echo()).await;
        tokio::spawn(s.gateway.clone().stop_ended_runs());
        s.rpc("tools/list", json!({})).await;
        assert_eq!(s.gateway.running(), 1);
        s.router.revoke_token(&s.token);
        s.app.bus.publish(AppEvent::new(
            "run_ended",
            json!({ "run": s.run, "status": "succeeded" }),
        ));
        while s.gateway.running() > 0 {
            tokio::task::yield_now().await;
        }
        let ended = reqwest::Client::new()
            .post(&s.url)
            .bearer_auth(&s.token)
            .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .send()
            .await
            .unwrap();
        assert_eq!(ended.status(), 401);
    }
}
