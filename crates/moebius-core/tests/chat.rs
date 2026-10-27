//! A chat (issue #29): a real `pi-bolt` answers two messages in a row, the second one continuing the first, and both
//! answers arrive as events through the desktop API.
//!
//! The model is a recorded OpenAI-compatible upstream that answers "Hello" and "Again" with a text each. Set
//! `MOEBIUS_PI_BOLT` to the `pi-bolt` executable to run the test; without it the test is skipped.

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::{last_message, recorded_upstream, start, text_answer};
use serde_json::{Value, json};

/// The recorded answer to a request: by what the last message says.
fn answer(body: &Value) -> String {
    let said = last_message(body)["content"].to_string();
    text_answer(
        if said.contains("Again") {
            "Once more."
        } else {
            "Hi there."
        },
        200,
    )
}

/// The texts of the assistant's answers among the events, in order.
fn answers(events: &Value) -> Vec<String> {
    events
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent_end")
        .filter_map(|e| {
            let last = e["data"]["messages"].as_array()?.last()?.clone();
            Some(last["content"][0]["text"].as_str()?.to_string())
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_answers_two_messages_in_a_row_through_a_real_pi_bolt() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone(), answer).await;
    let started = start(dir.path(), &pi_bolt, &upstream).await;

    let run: Value = started
        .post("/v1/chats", json!({ "agent": "Coder", "message": "Hello" }))
        .await
        .json()
        .await
        .unwrap();
    let path = format!("/v1/runs/{}/events", run["id"]);
    let mut seen = Vec::new();
    for (message, expected) in [(None, 1), (Some("Again"), 2)] {
        if let Some(message) = message {
            let said = started
                .post(
                    &format!("/v1/chats/{}/messages", run["id"]),
                    json!({ "message": message }),
                )
                .await;
            assert_eq!(said.status(), 200);
        }
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let events = started.get(&path).await;
            seen = answers(&events);
            if seen.len() == expected {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the answer never came: {events:#}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    assert_eq!(seen, ["Hi there.", "Once more."]);

    // The second request carried the first exchange: it is one conversation.
    {
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].to_string().contains("Hi there."));
    }

    // The chat waits for the next message until a person ends it.
    let chats = started.get("/v1/chats").await;
    assert_eq!(chats[0]["title"], "Hello");
    let ended = started
        .post(&format!("/v1/chats/{}/end", run["id"]), json!({}))
        .await;
    assert_eq!(ended.status(), 200);
    let deadline = Instant::now() + Duration::from_secs(60);
    while started.get(&format!("/v1/runs/{}", run["id"])).await["status"] != "succeeded" {
        assert!(Instant::now() < deadline, "the chat did not end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
