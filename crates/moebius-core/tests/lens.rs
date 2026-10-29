//! The lens (issue 114): a real `pi-bolt` with the Moebius lens loaded edits a file while the repository's fake linter
//! and fake type checker watch. The model is given only the problems an edit made new, the ones the decision point
//! `lint_report` lets through; a check over its time budget is skipped and noted; the slow check runs when the agent
//! is done, as `deep_check` decides, and its findings come once as a message. Every decision is a judgement with its
//! outcome, and the time of each check is a run event.
//!
//! The linters are shell scripts (`testdata/fake-lint`, `testdata/fake-types`), so the test is Unix only. Set
//! `MOEBIUS_PI_BOLT` to the `pi-bolt` executable to run it; without it the test is skipped.
#![cfg(unix)]

mod common;

use std::sync::{Arc, Mutex};

use common::{recorded_upstream, start, text_answer, tool_call_answer};
use moebius_core::repos::git;
use serde_json::{Value, json};

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

/// A repository whose files are checked by the fake tools in `testdata`.
fn origin_with_lint(dir: &std::path::Path) -> std::path::PathBuf {
    let origin = common::origin(dir);
    let testdata = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
    std::fs::create_dir_all(origin.join(".moebius")).unwrap();
    std::fs::write(
        origin.join(".moebius/lint.toml"),
        format!(
            "budget_ms = 700\nmax_lines = 5\n[[check]]\nglob = \"*.fake\"\nedit = \"{0}/fake-lint {{file}}\"\n\
             deep = \"{0}/fake-types {{files}}\"\n",
            testdata.display()
        ),
    )
    .unwrap();
    std::fs::write(
        origin.join("a.fake"),
        "line one\nWARN old warning\nline three\n",
    )
    .unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "lint"]).unwrap();
    origin
}

fn edit(old: &str, new: &str) -> Value {
    json!({ "path": "a.fake", "edits": [{ "oldText": old, "newText": new }] })
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_is_told_new_problems_at_once_and_the_slow_check_once_at_the_end() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let origin = origin_with_lint(dir.path());
    let answer = |body: &Value| {
        let turn = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "assistant")
            .count();
        match turn {
            0 => tool_call_answer("call_1", "edit", &edit("line three", "ERROR here"), 200),
            1 => {
                let new = "fine\nGLOBAL\nWARN new warning\nTYPE-ERROR in types";
                tool_call_answer("call_2", "edit", &edit("ERROR here", new), 200)
            }
            2 => {
                let arguments = json!({ "path": "slow.fake", "content": "SLOW\n" });
                tool_call_answer("call_3", "write", &arguments, 200)
            }
            3 => text_answer("Done.", 200),
            4 => tool_call_answer(
                "call_4",
                "edit",
                &edit("TYPE-ERROR in types", "types fine"),
                200,
            ),
            _ => text_answer("Done again.", 200),
        }
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone(), answer).await;
    let started = start(dir.path(), &pi_bolt, &upstream).await;
    let server = &started.server;
    let (_, card) = started.add_issue(&origin, "Edit a.fake", "Please.").await;
    let card_id = card["id"].as_i64().unwrap();
    started.wait_for_column(card_id, "Ready to merge").await;

    let requests = requests.lock().unwrap();
    // After the first edit: the error is new, the warning that was there before is not told.
    let first = result_of(&requests[1], "call_1");
    assert!(
        first.contains("1 new problem") && first.contains("a.fake:3:1 error L001"),
        "{first}"
    );
    assert!(!first.contains("found a warning marker"), "{first}");
    // After the second: the new warning on a touched line, but not the global one on line 1, which the rule hides.
    let second = result_of(&requests[2], "call_2");
    assert!(second.contains("a.fake:5:1 warning L002"), "{second}");
    assert!(!second.contains("global marker"), "{second}");
    // The slow linter is over its budget: skipped, and said once.
    let third = result_of(&requests[3], "call_3");
    assert!(third.contains("fake-lint was skipped"), "{third}");
    // The type check runs when the agent is done and tells it in one message.
    let fourth = requests[4]["messages"].as_array().unwrap().last().unwrap();
    let message = fourth["content"].to_string();
    assert_eq!(fourth["role"], "user");
    assert!(
        message.contains("the type check found 1 new problem")
            && message.contains("a.fake:6:1 error T001"),
        "{message}"
    );
    // Once it is fixed it is not told again.
    assert_eq!(requests.len(), 6, "{requests:#?}");

    let run = server
        .app
        .store
        .runs_for_card(card_id)
        .unwrap()
        .pop()
        .unwrap();
    let judged = |kind: &str| server.app.store.judgements(kind).unwrap();
    let context =
        |j: &moebius_core::store::Judgement| -> std::collections::BTreeMap<String, String> {
            serde_json::from_str(&server.app.store.blobs().get(&j.context[0]).unwrap()).unwrap()
        };
    let lint = judged("lint_report");
    let summary: Vec<_> = lint
        .iter()
        .map(|j| {
            (
                context(j)["message"].clone(),
                j.choice.as_str(),
                j.outcome.clone().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "found an error marker".to_string(),
                "show",
                "fixed".to_string()
            ),
            // By line within an edit: the global warning is on line 1.
            (
                "a global marker is somewhere".to_string(),
                "hide",
                "ignored".to_string()
            ),
            (
                "found a warning marker".to_string(),
                "show",
                "ignored".to_string()
            ),
        ]
    );
    assert_eq!(lint[0].decider, "lens/rules");
    assert_eq!(lint[0].run_id, Some(run.id));
    let shown = context(&lint[2]);
    assert_eq!(
        (
            shown["file"].as_str(),
            shown["line"].as_str(),
            shown["touched"].as_str()
        ),
        ("a.fake", "5", "true")
    );
    assert!(shown["edit"].contains("WARN new warning"), "{shown:?}");
    assert_eq!(shown["edit_tool"], "edit");

    let deep = judged("deep_check");
    let outcomes: Vec<_> = deep
        .iter()
        .map(|j| (j.choice.as_str(), j.outcome.as_deref().unwrap_or("")))
        .collect();
    assert_eq!(
        outcomes,
        [("run", "found something new"), ("run", "nothing new")]
    );
    assert!(context(&deep[0])["code_files"].contains("a.fake"));

    // The time of each check is a run event.
    let events = server.app.store.events(run.id).unwrap();
    let checks: Vec<&Value> = events
        .iter()
        .filter(|e| e.kind == "lens_check")
        .map(|e| &e.data)
        .collect();
    assert!(checks.iter().any(|c| c["phase"] == "lint"
        && c["tool"] == "fake-lint"
        && c["file"] == "a.fake"
        && c["ms"].is_u64()));
    assert!(
        checks
            .iter()
            .any(|c| c["skipped"] == "over budget" && c["file"] == "slow.fake")
    );
    assert_eq!(checks.iter().filter(|c| c["phase"] == "deep").count(), 2);
}
