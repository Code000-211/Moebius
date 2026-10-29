//! Keeping tool outputs small (issue #115): a real `pi-bolt` with the Moebius bridge runs many commands. The model's
//! requests hold placeholders for the outputs of the older turns, errors whole, and a long output cut to its head and
//! tail with where the whole is; the run's events keep every output, and the saved outputs are not committed.
//!
//! The recorded upstream calls `bash` once per turn (the second call fails), then writes `hello.txt` and says "Done.".
//! The Coder hides outputs older than three turns. Set `MOEBIUS_PI_BOLT` to the `pi-bolt` executable to run the test;
//! without it the test is skipped.

mod common;

use std::sync::{Arc, Mutex};

use common::{origin, recorded_upstream, start_with_agents, text_answer, tool_call_answer};
use moebius_core::repos::git;
use serde_json::{Value, json};

/// The commands of the turns, one call each: `call_1` to `call_7`.
const COMMANDS: [&str; 7] = [
    "seq 1 40",
    "seq 1 40; exit 3",
    "seq 101 140",
    "seq 201 240",
    "seq 301 340",
    "seq 401 440",
    "seq 1 1000",
];

/// The text the model was given as the result of `id` in `request`.
fn result_of(request: &Value, id: &str) -> String {
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool" && m["tool_call_id"] == id)
        .map(|m| m["content"].to_string())
        .unwrap_or_else(|| panic!("no result of {id} in {request:#}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn old_outputs_are_hidden_and_long_ones_cut_while_the_events_keep_them() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let origin = origin(dir.path());
    let answer = |body: &Value| {
        let turn = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "assistant")
            .count();
        match turn {
            n if n < COMMANDS.len() => tool_call_answer(
                &format!("call_{}", n + 1),
                "bash",
                &json!({ "command": COMMANDS[n] }),
                200,
            ),
            n if n == COMMANDS.len() => {
                let arguments = json!({ "path": "hello.txt", "content": "Hello\n" });
                tool_call_answer("call_8", "write", &arguments, 200)
            }
            _ => text_answer("Done.", 200),
        }
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone(), answer).await;
    let agents = "[agents.Coder]\nmodel = \"fake-coder\"\n[agents.Coder.tool_output]\nhide_after_turns = 3\n";
    let started = start_with_agents(dir.path(), &pi_bolt, &upstream, agents).await;
    let server = &started.server;
    let (repo, card) = started
        .add_issue(&origin, "Say hello", "Add hello.txt.")
        .await;
    let card_id = card["id"].as_i64().unwrap();
    started.wait_for_column(card_id, "Ready to merge").await;

    // The last request: eight turns before it, so the outputs of turns one to five are hidden, but not the error.
    let requests = requests.lock().unwrap();
    let last = requests.last().unwrap();
    for id in ["call_1", "call_3", "call_4", "call_5"] {
        let given = result_of(last, id);
        assert!(given.contains("output hidden: 40 lines"), "{id}: {given}");
        assert!(
            given.contains("bash") && given.contains("seq "),
            "{id}: {given}"
        );
    }
    let error = result_of(last, "call_2");
    assert!(
        !error.contains("output hidden") && error.contains("40"),
        "{error}"
    );
    let recent = result_of(last, "call_6");
    assert!(
        recent.contains("440") && !recent.contains("output hidden"),
        "{recent}"
    );
    // An earlier request still had the first output whole.
    assert!(result_of(&requests[1], "call_1").contains("40"));
    assert!(!result_of(&requests[1], "call_1").contains("output hidden"));
    // The long output came cut to the budget, with where the whole is.
    let long = result_of(last, "call_7");
    assert!(
        long.contains("lines left out; the full output is in .moebius/out/1.txt"),
        "{long}"
    );
    assert!(!long.contains("\\n500\\n"), "{long}");

    // The run's events keep every output whole, and what the cut saved.
    let run = server
        .app
        .store
        .runs_for_card(card_id)
        .unwrap()
        .pop()
        .unwrap();
    let events = server.app.store.events(run.id).unwrap();
    let first = events
        .iter()
        .find(|e| e.kind == "tool_execution_end" && e.data["toolCallId"] == "call_1")
        .expect("the first call ended");
    let text = first.data["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(text.trim_end().lines().count(), 40, "{text}");
    let cut = events
        .iter()
        .find(|e| e.kind == "tool_output" && e.data["tool_call_id"] == "call_7")
        .unwrap_or_else(|| panic!("no tool_output event in {events:#?}"));
    assert!(
        cut.data["steps"]
            .as_array()
            .unwrap()
            .contains(&json!("budget"))
    );
    assert!(cut.data["bytes_after"].as_i64() < cut.data["bytes_before"].as_i64());
    assert_eq!(cut.data["saved"], ".moebius/out/1.txt");

    // The saved output is the run's scratch, not part of the change.
    let change = server
        .app
        .store
        .changes(repo["id"].as_i64().unwrap())
        .unwrap()
        .pop()
        .unwrap();
    let clone = std::path::PathBuf::from(repo["path"].as_str().unwrap());
    let files = git(&clone, &["ls-tree", "-r", "--name-only", &change.branch]).unwrap();
    assert!(files.contains("hello.txt"), "{files}");
    assert!(!files.contains(".moebius"), "{files}");
}
