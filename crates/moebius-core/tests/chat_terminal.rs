//! A chat and its terminal are one conversation (issue #73): a real `pi-bolt` answers a chat, the person goes on in
//! Pi-Bolt's own terminal interface, what is said there arrives as the chat's events (read from the session file the
//! real Pi writes), and the chat's Pi then answers the next message knowing all of it.
//!
//! The model is a recorded OpenAI-compatible upstream: "Hello" and "Again" are answered with a call to Pi's `write`
//! tool and, once its result is in, a text; "Third" with a text. Set `MOEBIUS_PI_BOLT` to the `pi-bolt` executable to
//! run the test; without it the test is skipped.

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::{last_message, recorded_upstream, start, text_answer, tool_call_answer};
use futures_util::{SinkExt, StreamExt};
use moebius_core::terminal::tui_key;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

/// What the person said last in a request to the upstream.
fn last_said(body: &Value) -> String {
    body["messages"]
        .as_array()
        .and_then(|messages| messages.iter().rfind(|m| m["role"] == "user"))
        .map(|m| m["content"].to_string())
        .unwrap_or_default()
}

fn answer(body: &Value) -> String {
    let said = last_said(body);
    let (file, text) = if said.contains("Again") {
        ("again.txt", "Once more.")
    } else if said.contains("Third") {
        return text_answer("Third answer.", 100);
    } else {
        ("hello.txt", "Done.")
    };
    if last_message(body)["role"] == "tool" {
        text_answer(text, 100)
    } else {
        let arguments = json!({ "path": file, "content": "written\n" });
        tool_call_answer("call_1", "write", &arguments, 100)
    }
}

/// Waits up to two minutes for `done`.
async fn until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while !done() {
        assert!(Instant::now() < deadline, "never: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn what_is_said_in_a_chats_terminal_is_the_chats_conversation_through_a_real_pi_bolt() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone(), answer).await;
    let started = start(dir.path(), &pi_bolt, &upstream).await;
    let app = started.server.app.clone();
    let run: Value = started
        .post("/v1/chats", json!({ "agent": "Coder", "message": "Hello" }))
        .await
        .json()
        .await
        .unwrap();
    let id = run["id"].as_i64().unwrap();
    let status = || app.store.run(id).unwrap().unwrap().status;
    until("the chat waits", || {
        status() == "waiting"
            && app
                .store
                .events(id)
                .unwrap()
                .iter()
                .any(|e| e.kind == "agent_settled")
    })
    .await;
    let seen = app.store.events(id).unwrap().len();

    // The person opens the chat's terminal and writes in Pi-Bolt's own interface.
    let path = format!("{}/v1/chats/{id}/tui", started.api.replace("http", "ws"));
    let mut request = path.into_client_request().unwrap();
    let bearer = format!("Bearer {}", started.server.token);
    request
        .headers_mut()
        .insert("authorization", bearer.parse().unwrap());
    let (mut tui, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let mut printed = String::new();
    let ready = async {
        // The footer names the provider once Pi has started; until then Enter does not send.
        while !printed.contains("(moebius) fake-coder") {
            match tui.next().await {
                Some(Ok(Message::Binary(bytes))) => {
                    printed.push_str(&String::from_utf8_lossy(&bytes))
                }
                Some(Ok(_)) => {}
                other => panic!("the terminal closed: {other:?} {printed:?}"),
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(120), ready)
        .await
        .expect("Pi-Bolt's interface never finished starting");
    for key in ["A", "g", "a", "i", "n", "\r"] {
        tui.send(Message::Binary(key.as_bytes().to_vec().into()))
            .await
            .unwrap();
    }

    // What it says is recorded as the chat's events, in the shapes of the records of Pi's RPC mode.
    let settled = || {
        app.store.events(id).unwrap()[seen..]
            .iter()
            .any(|e| e.kind == "agent_settled")
    };
    let deadline = Instant::now() + Duration::from_secs(120);
    while !settled() {
        if Instant::now() > deadline {
            let sessions = app.data_dir.join(format!("runs/{id}/sessions"));
            let files: Vec<String> = std::fs::read_dir(&sessions)
                .unwrap()
                .flatten()
                .map(|f| std::fs::read_to_string(f.path()).unwrap())
                .collect();
            let kinds: Vec<String> = app
                .store
                .events(id)
                .unwrap()
                .into_iter()
                .map(|e| e.kind)
                .collect();
            panic!(
                "the answer in the terminal was never recorded: {kinds:?}\nsessions: {files:#?}\nprinted: {printed:?}"
            );
        }
        if let Ok(Some(Ok(Message::Binary(bytes)))) =
            tokio::time::timeout(Duration::from_millis(100), tui.next()).await
        {
            printed.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    let events = app.store.events(id).unwrap();
    // The bridge still checks the terminal's tool calls. Its check is recorded as Pi asks for it, so it can come
    // before the session's lines are read: it is left out of the order.
    let check = events[seen..]
        .iter()
        .find(|e| e.kind == "tool_check")
        .expect("the terminal's tool call is checked");
    assert_eq!(check.data["tool"], "write");
    assert_eq!(check.data["decision"], "allow");
    let added: Vec<(&str, &Value)> = events[seen..]
        .iter()
        .filter(|e| e.kind != "tool_check")
        .map(|e| (e.kind.as_str(), &e.data))
        .collect();
    let kinds: Vec<&str> = added.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(
        kinds,
        [
            "tui_opened",
            "said",
            "agent_start",
            "tool_execution_start",
            "tool_execution_end",
            "message_update",
            "agent_end",
            "agent_settled"
        ],
        "{added:#?}"
    );
    assert_eq!(added[1].1, &json!({ "message": "Again" }));
    // The same tool call reads the same as the chat's own Pi recorded it for "Hello".
    let rpc_start = events[..seen]
        .iter()
        .find(|e| e.kind == "tool_execution_start")
        .unwrap();
    assert_eq!(added[3].1["toolName"], rpc_start.data["toolName"]);
    assert_eq!(added[3].1["toolCallId"], rpc_start.data["toolCallId"]);
    assert_eq!(
        added[3].1["args"],
        json!({ "path": "again.txt", "content": "written\n" })
    );
    assert_eq!(added[4].1["toolName"], "write");
    assert_eq!(added[4].1["isError"], false);
    let rpc_end = events[..seen]
        .iter()
        .find(|e| e.kind == "tool_execution_end")
        .unwrap();
    let wrote =
        |file: &str| json!([{ "type": "text", "text": format!("Successfully wrote to {file}") }]);
    assert_eq!(rpc_end.data["result"]["content"], wrote("hello.txt"));
    assert_eq!(added[4].1["result"]["content"], wrote("again.txt"));
    assert_eq!(
        added[5].1["assistantMessageEvent"],
        json!({ "type": "text_delta", "contentIndex": 0, "delta": "Once more." })
    );
    assert_eq!(app.run(id).unwrap().summary.as_deref(), Some("Once more."));

    // Closing the terminal gives the chat back to its own Pi, which knows what was said in the terminal.
    tui.close(None).await.unwrap();
    until("the chat takes messages", || {
        !app.terminals.is_open(tui_key(id))
    })
    .await;
    assert_eq!(status(), "waiting");
    let said = started
        .post(
            &format!("/v1/chats/{id}/messages"),
            json!({ "message": "Third" }),
        )
        .await;
    assert_eq!(said.status(), 200);
    until("the third answer", || {
        app.run(id).unwrap().summary.as_deref() == Some("Third answer.")
    })
    .await;
    let requests = requests.lock().unwrap();
    let third = requests.last().unwrap().to_string();
    for earlier in ["Hello", "Done.", "Again", "Once more.", "Third"] {
        assert!(third.contains(earlier), "{earlier} is missing: {third}");
    }
}
