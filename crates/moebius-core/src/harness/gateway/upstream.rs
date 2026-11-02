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
        // The server gets the variables its table names (`env`), not the server's whole environment.
        crate::harness::clear_environment(process.as_std_mut());
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
            .map_err(|e| format!("cannot reach the server: {}", e.without_url()))?;
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
                .map_err(|e| format!("the server's answer is not JSON: {}", e.without_url()));
        }
        // An event stream: the answer is the event whose data is the message with the request's id.
        let mut body = response.bytes_stream();
        // Bytes are buffered and only complete events are decoded, so that a character split across chunks is whole.
        let mut buffer: Vec<u8> = Vec::new();
        // How much of the buffer is known to hold no end of an event, so that a chunk is scanned once.
        let mut scanned = 0;
        while let Some(chunk) = body.next().await {
            let chunk = chunk
                .map_err(|e| format!("cannot read the server's answer: {}", e.without_url()))?;
            buffer.extend_from_slice(&chunk);
            while let Some((end, gap)) = event_end(&buffer, scanned) {
                let event: Vec<u8> = buffer.drain(..end + gap).take(end).collect();
                scanned = 0;
                let event = String::from_utf8_lossy(&event).replace("\r\n", "\n");
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
            scanned = buffer.len();
        }
        Err("the server's event stream ended without an answer".into())
    }
}

/// Where the first event of `buffer` ends, and how long the blank line after it is: at the first empty line, whether
/// the lines end in `\n`, `\r\n` or `\r`, mixed or not. The first `scanned` bytes are known to hold no such line
/// (a blank line cut by them is found again from three bytes before).
fn event_end(buffer: &[u8], scanned: usize) -> Option<(usize, usize)> {
    let mut at = scanned.saturating_sub(3);
    while at < buffer.len() {
        let end = line_end(&buffer[at..]);
        if let Some(first) = end
            && let Some(second) = line_end(&buffer[at + first..])
        {
            return Some((at, first + second));
        }
        at += 1;
    }
    None
}

/// The length of the line terminator `rest` starts with, if it does.
fn line_end(rest: &[u8]) -> Option<usize> {
    match rest {
        [b'\r', b'\n', ..] => Some(2),
        [b'\n', ..] | [b'\r', ..] => Some(1),
        _ => None,
    }
}

#[cfg(test)]
mod sse_tests {
    use super::*;

    #[test]
    fn an_event_ends_at_a_blank_line_of_any_line_endings_and_is_found_across_chunks() {
        assert_eq!(event_end(b"data: x\n\r\nrest", 0), Some((7, 3)));
        assert_eq!(event_end(b"data: x\r\n\nrest", 0), Some((7, 3)));
        assert_eq!(event_end(b"data: x\r\rrest", 0), Some((7, 2)));
        assert_eq!(event_end(b"data: x\n", 0), None);
        // Fed in pieces, with only the new part scanned, the same end is found.
        let whole = b"data: x\r\n\r\nrest";
        for size in 1..5 {
            let mut buffer = Vec::new();
            let mut scanned = 0;
            let mut found = None;
            for piece in whole.chunks(size) {
                buffer.extend_from_slice(piece);
                found = event_end(&buffer, scanned);
                if found.is_some() {
                    break;
                }
                scanned = buffer.len();
            }
            assert_eq!(found.map(|(end, _)| end), Some(7), "size {size}");
        }
    }

    #[tokio::test]
    async fn a_character_split_across_chunks_of_an_event_stream_stays_whole() {
        use axum::body::Body;
        use axum::http::header;
        use axum::response::IntoResponse;
        let app = axum::Router::new().route(
            "/mcp",
            axum::routing::post(|| async {
                // The euro sign is three bytes; the chunks cut it after the first, and an event after its `data:`.
                let event = "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"text\":\"5 \u{20ac}\"}}\r\n\r\n";
                let bytes = event.as_bytes();
                let cut = event.find('\u{20ac}').unwrap() + 1;
                let chunks: Vec<Result<Vec<u8>, std::io::Error>> = vec![
                    Ok(bytes[..5].to_vec()),
                    Ok(bytes[5..cut].to_vec()),
                    Ok(bytes[cut..].to_vec()),
                ];
                (
                    [(header::CONTENT_TYPE, "text/event-stream")],
                    Body::from_stream(futures_util::stream::iter(chunks)),
                )
                    .into_response()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        let http = Http {
            client: reqwest::Client::new(),
            url,
            headers: BTreeMap::new(),
            session: std::sync::Mutex::new(None),
            next_id: AtomicI64::new(1),
        };
        let answer = http.request("tools/list", json!({})).await.unwrap();
        assert_eq!(answer["result"]["text"], "5 \u{20ac}");
    }
}

// Unix only: the stdio test starts `sh`.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_stdio_server_gets_its_own_variables_not_the_servers_environment() {
        // Cargo sets this variable for the test, as a key named by `api_key_env` is set for the server.
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
        let server = McpServer {
            command: Some("sh".into()),
            args: vec![
                "-c".into(),
                "echo \"[$CARGO_MANIFEST_DIR][$OWN][$PATH]\"".into(),
            ],
            env: BTreeMap::from([("OWN".to_string(), "mine".to_string())]),
            url: None,
            headers: BTreeMap::new(),
        };
        let started = Stdio::start("sh", &server, None).unwrap();
        let mut line = String::new();
        started
            .io
            .lock()
            .await
            .1
            .read_line(&mut line)
            .await
            .unwrap();
        assert!(
            line.starts_with("[][mine][/") || line.starts_with("[][mine]["),
            "{line}"
        );
        assert!(!line.contains("[mine][]"), "PATH is kept: {line}");
    }

    #[tokio::test]
    async fn an_error_of_a_remote_server_does_not_show_the_url_with_its_key() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let http = Http {
            client: reqwest::Client::new(),
            url: format!("http://127.0.0.1:{port}/mcp?apiKey=very-secret-key-1234"),
            headers: BTreeMap::new(),
            session: std::sync::Mutex::new(None),
            next_id: AtomicI64::new(1),
        };
        let error = http.post(&json!({})).await.unwrap_err();
        assert!(error.starts_with("cannot reach the server"), "{error}");
        assert!(!error.contains("very-secret-key"), "{error}");
    }
}
