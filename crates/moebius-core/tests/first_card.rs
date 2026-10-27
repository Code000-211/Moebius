//! The first card through the workflow (issue #12): an issue in a plain git repository is coded by the Coder agent on
//! a real `pi-bolt`, committed on its branch, offered as a change and merged by a person through the desktop API.
//!
//! The model is a recorded OpenAI-compatible upstream: it answers the first request with a call to Pi's `write` tool
//! and the request carrying the tool's result with "Done.". Set `MOEBIUS_PI_BOLT` to the `pi-bolt` executable to run
//! the test; without it the test is skipped.

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use common::{last_message, origin, recorded_upstream, start, text_answer, tool_call_answer};
use moebius_core::repos::git;
use serde_json::{Value, json};

/// The recorded answer to a request: a tool call first, then "Done." once the tool's result is in.
fn answer(body: &Value) -> String {
    if last_message(body)["role"] == "tool" {
        text_answer("Done.", 300)
    } else {
        let arguments = json!({ "path": "hello.txt", "content": "Hello from Moebius\n" });
        tool_call_answer("call_1", "write", &arguments, 250)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_issue_becomes_a_merged_change_through_a_real_pi_bolt() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let origin = origin(dir.path());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone(), answer).await;
    let started = start(dir.path(), &pi_bolt, &upstream).await;
    let server = &started.server;
    let (repo, card) = started
        .add_issue(&origin, "Say hello", "Add hello.txt saying hello.")
        .await;
    let card_id = card["id"].as_i64().unwrap();

    // The board takes the card from the backlog through a Coder run to "Ready to merge".
    started.wait_for_column(card_id, "Ready to merge").await;

    let run = server
        .app
        .store
        .runs_for_card(card_id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        (run.agent.as_str(), run.status.as_str()),
        ("Coder", "succeeded")
    );
    let kinds: Vec<String> = server
        .app
        .store
        .events(run.id)
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert!(
        kinds.contains(&"tool_execution_end".to_string()),
        "{kinds:?}"
    );
    assert_eq!(kinds.last().map(String::as_str), Some("agent_settled"));
    assert!(requests.lock().unwrap().len() >= 2);
    let used: i64 = server
        .app
        .store
        .usage_for_run(run.id)
        .unwrap()
        .iter()
        .map(|u| u.total())
        .sum();
    assert_eq!(used, 250 + 30 + 300 + 2);

    // A person merges the change through the desktop API.
    let change = server
        .app
        .store
        .changes(repo["id"].as_i64().unwrap())
        .unwrap()
        .pop()
        .unwrap();
    let merged = started
        .post(
            &format!("/v1/changes/{}/merge", change.id),
            json!({ "reason": "ship it" }),
        )
        .await;
    assert_eq!(merged.status(), 200);
    let clone = PathBuf::from(repo["path"].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(clone.join("hello.txt")).unwrap(),
        "Hello from Moebius\n"
    );
    assert_eq!(server.app.card(card_id).unwrap().column, "Done");
    assert!(
        git(&clone, &["log", "--oneline", "-3"])
            .unwrap()
            .contains("Merge #1: Say hello")
    );
}
