//! The Moebius agent (issue #31) through a real `pi-bolt`: a chat with it, in a repository, writes a column script
//! with Pi's `write` tool, checks it with `"$MOEBIUS" check-script` in the `bash` tool (a call that passes through the
//! bridge and the guard) and answers; when the chat ends, the script is offered as a change without a card, which a
//! person merges.
//!
//! The model is a recorded OpenAI-compatible upstream. The server runs in this test, so the runs are told that
//! `$MOEBIUS` is the `moebius` binary built for it (`Config::moebius`), not the test. Set `MOEBIUS_PI_BOLT` to the
//! `pi-bolt` executable to run the test; without it the test is skipped.

#[path = "../../moebius-core/tests/common/mod.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::{origin, recorded_upstream, start_as, text_answer, tool_call_answer};
use moebius_core::repos::git;
use serde_json::{Value, json};

const SCRIPT: &str = "function on_card(card)\n  if #card.blocked_by > 0 then\n    moebius.wait(\"blocked\")\n  else\n    moebius.move(\"In progress\")\n  end\nend\n";

/// The recorded answers: write the script, check it, say "Done.". Which one is by how many tool results the request
/// carries.
fn answer(body: &Value) -> String {
    let results = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .count();
    match results {
        0 => {
            let arguments = json!({ "path": ".moebius/ready.luau", "content": SCRIPT });
            tool_call_answer("call_write", "write", &arguments, 400)
        }
        1 => {
            let command = "\"$MOEBIUS\" check-script .moebius/ready.luau --column Ready";
            tool_call_answer("call_check", "bash", &json!({ "command": command }), 450)
        }
        _ => text_answer("Done.", 500),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_with_the_moebius_agent_writes_a_column_script_and_it_is_offered_as_a_change() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let origin = origin(dir.path());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone(), answer).await;
    let moebius = Path::new(env!("CARGO_BIN_EXE_moebius"));
    let started = start_as(dir.path(), &pi_bolt, &upstream, Some(moebius)).await;
    let app = &started.server.app;
    let repo: Value = started
        .post(
            "/v1/repositories",
            json!({ "name": "demo", "url": origin.to_str().unwrap() }),
        )
        .await
        .json()
        .await
        .unwrap();

    // No agent is named: the chat is with the Moebius agent.
    let message = "Start cards in Ready as soon as nothing blocks them.";
    let run: Value = started
        .post(
            "/v1/chats",
            json!({ "message": message, "repository": repo["id"] }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(run["agent"], "Moebius");
    let run_id = run["id"].as_i64().unwrap();
    let wait_for = |status: &'static str| async move {
        let deadline = Instant::now() + Duration::from_secs(120);
        while app.run(run_id).unwrap().status != status {
            let events = app.store.events(run_id).unwrap();
            assert!(
                Instant::now() < deadline && app.run(run_id).unwrap().status != "failed",
                "the chat is not {status}: {events:#?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    wait_for("waiting").await;
    assert_eq!(app.run(run_id).unwrap().summary.as_deref(), Some("Done."));

    // The check ran through the bridge, and `moebius check-script` said what the script does.
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    let last = requests[2].to_string();
    assert!(last.contains("moved the card to In progress"), "{last}");
    let first = requests[0].to_string();
    assert!(
        first.contains("moebius"),
        "the skill is in the first request"
    );

    // Ending the chat offers the script as a change without a card.
    let ended = started
        .post(&format!("/v1/chats/{run_id}/end"), json!({}))
        .await;
    assert_eq!(ended.status(), 200);
    wait_for("succeeded").await;
    let repo_id = repo["id"].as_i64().unwrap();
    let change = app.store.changes(repo_id).unwrap().pop().unwrap();
    assert_eq!(change.card_id, None);
    assert_eq!(change.branch, format!("moebius/chat-{run_id}"));
    let inbox = app.store.inbox().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(
        (inbox[0].kind.as_str(), inbox[0].data["change"].as_i64()),
        ("ready_to_merge", Some(change.id))
    );

    // A person merges it and the script is on the default branch.
    let merged = started
        .post(&format!("/v1/changes/{}/merge", change.id), json!({}))
        .await;
    assert_eq!(merged.status(), 200);
    let clone = PathBuf::from(repo["path"].as_str().unwrap());
    assert_eq!(
        git(&clone, &["show", "main:.moebius/ready.luau"]).unwrap(),
        SCRIPT.trim()
    );
}
