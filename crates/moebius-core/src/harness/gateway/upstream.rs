//! One MCP server of a run as the gateway reaches it: a stdio server is started as a child process of the Moebius
//! server and spoken to in JSON-RPC, one message per line; a remote one is sent JSON-RPC over streamable HTTP and may
//! answer with JSON or with an event stream. Either is initialized and its tools listed when it is connected.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio as Piped;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::harness::mcp::McpServer;

/// The MCP version the gateway asks its servers for.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// How long a server may take to start and answer `initialize`, and to list its tools.
const OPEN_LIMIT: Duration = Duration::from_secs(60);

/// How many pages of tools are read from one server at most.
const MOST_PAGES: usize = 100;

/// A connected server.
pub(super) enum Upstream {
    Stdio(Box<Stdio>),
    Http(Http),
}

pub(super) struct Stdio {
    /// Killed when the server is dropped, which happens when the run ends.
    _child: Child,
    io: Mutex<(ChildStdin, BufReader<ChildStdout>)>,
    next_id: AtomicI64,
}

pub(super) struct Http {
    client: reqwest::Client,
    url: String,
    headers: BTreeMap<String, String>,
    /// The `Mcp-Session-Id` the server gave at `initialize`, if any.
    session: std::sync::Mutex<Option<String>>,
    next_id: AtomicI64,
}

/// Starts or connects to `server`, initializes it and lists its tools. A stdio server works in `worktree`, the run's.
pub(super) async fn connect(
    server: &McpServer,
    worktree: Option<&Path>,
) -> Result<(Upstream, Vec<Value>), String> {
    let upstream = match (&server.command, &server.url) {
        (Some(command), _) => Upstream::Stdio(Box::new(Stdio::start(command, server, worktree)?)),
        (None, Some(url)) => Upstream::Http(Http {
            client: reqwest::Client::new(),
            url: url.clone(),
            headers: server.headers.clone(),
            session: std::sync::Mutex::new(None),
            next_id: AtomicI64::new(1),
        }),
        (None, None) => return Err("the server has neither a command nor a url".into()),
    };
    let initialize = json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": { "name": "moebius", "version": env!("CARGO_PKG_VERSION") },
    });
    upstream
        .request("initialize", initialize, OPEN_LIMIT)
        .await?;
    upstream.notify("notifications/initialized").await?;
    let mut tools = vec![];
    let mut cursor: Option<Value> = None;
    for _ in 0..MOST_PAGES {
        let params = match &cursor {
            Some(cursor) => json!({ "cursor": cursor }),
            None => json!({}),
        };
        let page = upstream.request("tools/list", params, OPEN_LIMIT).await?;
        tools.extend(page["tools"].as_array().cloned().unwrap_or_default());
        cursor = page.get("nextCursor").filter(|c| !c.is_null()).cloned();
        if cursor.is_none() {
            break;
        }
    }
    Ok((upstream, tools))
}

impl Upstream {
    /// Sends the request `method` and waits up to `limit` for its result; an error answer is an `Err` with its message.
    pub(super) async fn request(
        &self,
        method: &str,
        params: Value,
        limit: Duration,
    ) -> Result<Value, String> {
        let asked = async {
            match self {
                Upstream::Stdio(stdio) => stdio.request(method, params).await,
                Upstream::Http(http) => http.request(method, params).await,
            }
        };
        let answer = tokio::time::timeout(limit, asked)
            .await
            .map_err(|_| format!("no answer to {method} within {} seconds", limit.as_secs()))??;
        if let Some(error) = answer.get("error") {
            let message = error["message"]
                .as_str()
                .unwrap_or("an error without a message");
            return Err(format!("{method} failed: {message}"));
        }
        Ok(answer.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn notify(&self, method: &str) -> Result<(), String> {
        let message = json!({ "jsonrpc": "2.0", "method": method });
        match self {
            Upstream::Stdio(stdio) => send(&mut stdio.io.lock().await.0, &message).await,
            Upstream::Http(http) => http.post(&message).await.map(drop),
        }
    }
}

/// Writes `message` to a stdio server as one line.
async fn send(stdin: &mut ChildStdin, message: &Value) -> Result<(), String> {
    let mut line = message.to_string();
    line.push('\n');
    let written = async {
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await
    };
    written
        .await
        .map_err(|e| format!("cannot write to the server: {e}"))
}

impl Stdio {
    fn start(command: &str, server: &McpServer, worktree: Option<&Path>) -> Result<Self, String> {
        let mut process = Command::new(command);
        process
            .args(&server.args)
            .envs(&server.env)
            .stdin(Piped::piped())
            .stdout(Piped::piped())
            .stderr(Piped::null())
            .kill_on_drop(true);
        if let Some(worktree) = worktree {
            process.current_dir(worktree);
        }
        let mut child = process
            .spawn()
            .map_err(|e| format!("cannot start {command:?}: {e}"))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        Ok(Self {
            _child: child,
            io: Mutex::new((stdin, BufReader::new(stdout))),
            next_id: AtomicI64::new(1),
        })
    }

    /// One request at a time: the answer is the next message with its id. A request of the server's own is answered
    /// that the gateway does not offer it, and notifications are skipped.
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut io = self.io.lock().await;
        let (stdin, stdout) = &mut *io;
        let asked = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        send(stdin, &asked).await?;
        let mut line = String::new();
        loop {
            line.clear();
            let read = stdout
                .read_line(&mut line)
                .await
                .map_err(|e| format!("cannot read from the server: {e}"))?;
            if read == 0 {
                return Err("the server exited".into());
            }
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if message.get("method").is_some() {
                if let Some(theirs) = message.get("id") {
                    let refusal = json!({ "jsonrpc": "2.0", "id": theirs,
                        "error": { "code": -32601, "message": "the Moebius gateway does not offer this" } });
                    send(stdin, &refusal).await?;
                }
                continue;
            }
            if message["id"] == id {
                return Ok(message);
            }
        }
    }
}

impl Http {
    /// POSTs `message` with the server's headers and the session, if there is one.
    async fn post(&self, message: &Value) -> Result<reqwest::Response, String> {
        let mut request = self
            .client
            .post(&self.url)
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", PROTOCOL_VERSION)
            .json(message);
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        let session = self
            .session
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(session) = session {
            request = request.header("mcp-session-id", session);
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("cannot reach the server: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("the server answered {}", response.status()));
        }
        Ok(response)
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let response = self
            .post(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await?;
        if let Some(session) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            *self.session.lock().unwrap_or_else(|p| p.into_inner()) = Some(session.to_string());
        }
        let streams = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        if !streams {
            return response
                .json()
                .await
                .map_err(|e| format!("the server's answer is not JSON: {e}"));
        }
        // An event stream: the answer is the event whose data is the message with the request's id.
        let mut body = response.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|e| format!("cannot read the server's answer: {e}"))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            let buffer_now = buffer.replace("\r\n", "\n");
            let mut events: Vec<&str> = buffer_now.split("\n\n").collect();
            let rest = events.pop().unwrap_or_default().to_string();
            for event in events {
                let data: Vec<&str> = event
                    .lines()
                    .filter_map(|line| line.strip_prefix("data:"))
                    .map(str::trim_start)
                    .collect();
                if let Ok(message) = serde_json::from_str::<Value>(&data.join("\n"))
                    && message["id"] == id
                    && message.get("method").is_none()
                {
                    return Ok(message);
                }
            }
            buffer = rest;
        }
        Err("the server's event stream ended without an answer".into())
    }
}
