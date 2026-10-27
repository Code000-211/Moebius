//! A refused tool call (issue #21): a real `pi-bolt` with the Moebius bridge loaded tries a destructive command, is
//! refused with the rule, and finishes the issue anyway.
//!
//! The recorded upstream first calls `bash` with a recursive delete, then, once the refusal is the last message, writes
//! `hello.txt`, and says "Done." after that. The delete names a directory the test made outside the worktree, so a
//! bridge that failed open would delete it and the test would notice, and nothing else is at risk. Set
//! `MOEBIUS_PI_BOLT` to the `pi-bolt` executable to run the test; without it the test is skipped.

mod common;

use std::sync::{Arc, Mutex};

use common::{last_message, origin, recorded_upstream, start, text_answer, tool_call_answer};
use serde_json::{Value, json};

#[tokio::test(flavor = "multi_thread")]
async fn a_destructive_command_is_refused_with_the_rule_and_the_run_goes_on() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let origin = origin(dir.path());
    let outside = dir.path().join("outside-precious");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("file"), "keep me\n").unwrap();
    let command = format!("rm -rf {}", outside.display());

    let answer = {
        let command = command.clone();
        move |body: &Value| {
            let last = last_message(body);
            if last["role"] != "tool" {
                tool_call_answer("call_1", "bash", &json!({ "command": command }), 250)
            } else if last["tool_call_id"] == "call_1" {
                let arguments = json!({ "path": "hello.txt", "content": "Hello from Moebius\n" });
                tool_call_answer("call_2", "write", &arguments, 300)
            } else {
                text_answer("Done.", 350)
            }
        }
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone(), answer).await;
    let started = start(dir.path(), &pi_bolt, &upstream).await;
    let server = &started.server;
    let (repo, card) = started
        .add_issue(&origin, "Say hello", "Add hello.txt saying hello.")
        .await;
    let card_id = card["id"].as_i64().unwrap();
    started.wait_for_column(card_id, "Ready to merge").await;

    assert!(outside.join("file").exists(), "the command was run");

    // The command left one refusal, decided by the pattern guard, naming the rule.
    let judgements = server.app.store.judgements("tool_call").unwrap();
    let refusal = judgements
        .iter()
        .find(|j| j.choice == "refuse")
        .unwrap_or_else(|| panic!("no refusal in {judgements:#?}"));
    assert_eq!(refusal.decider, "guard/patterns");
    assert!(
        refusal.reason.starts_with("no-destructive-commands"),
        "{}",
        refusal.reason
    );

    // The run saw both checks, the failed bash call and the allowed write, and succeeded.
    let run = server
        .app
        .store
        .runs_for_card(card_id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(run.status, "succeeded");
    let events = server.app.store.events(run.id).unwrap();
    let check = |tool: &str, decision: &str| {
        events.iter().any(|e| {
            e.kind == "tool_check"
                && e.data["tool"] == tool
                && e.data["decision"] == decision
                && e.data["reason"].as_str().is_some_and(|r| {
                    decision == "allow" || r.starts_with("no-destructive-commands")
                })
        })
    };
    assert!(check("bash", "refuse"), "{events:#?}");
    assert!(check("write", "allow"), "{events:#?}");
    assert!(
        events.iter().any(|e| e.kind == "tool_execution_end"
            && e.data["toolName"] == "bash"
            && e.data["isError"] == true),
        "{events:#?}"
    );

    // The model was told why, in the tool's result.
    let requests = requests.lock().unwrap();
    let told = requests
        .iter()
        .map(last_message)
        .find(|m| m["role"] == "tool" && m["tool_call_id"] == "call_1")
        .expect("the refusal reached the model");
    let told = told["content"].to_string();
    assert!(
        told.contains("Moebius refused this call: no-destructive-commands"),
        "{told}"
    );

    // The change carries the file the run went on to write.
    let change = server
        .app
        .store
        .changes(repo["id"].as_i64().unwrap())
        .unwrap()
        .pop()
        .unwrap();
    let clone = std::path::PathBuf::from(repo["path"].as_str().unwrap());
    let shown =
        moebius_core::repos::git(&clone, &["show", &format!("{}:hello.txt", change.branch)])
            .unwrap();
    assert_eq!(shown, "Hello from Moebius");
}
