//! Lessons (issue 116) with a real `pi-bolt`: a code run is given the lessons that match its issue in its prompt, and
//! the tool `lessons`, which the bridge extension offers, answers with more of them from the server, authenticated by
//! the run's token. Set `MOEBIUS_PI_BOLT` to the `pi-bolt` executable to run it; without it the test is skipped.
#![cfg(unix)]

mod common;

use std::sync::{Arc, Mutex};

use common::{recorded_upstream, start, text_answer, tool_call_answer};
use moebius_core::store::NewLesson;
use serde_json::{Value, json};

#[tokio::test(flavor = "multi_thread")]
async fn a_run_is_given_lessons_in_its_prompt_and_asks_for_more_with_the_tool() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let origin = common::origin(dir.path());
    let answer = |body: &Value| {
        let turn = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "assistant")
            .count();
        match turn {
            0 => tool_call_answer(
                "call_1",
                "lessons",
                &json!({ "query": "tokenizer emoji" }),
                200,
            ),
            1 => tool_call_answer(
                "call_2",
                "write",
                &json!({ "path": "done.txt", "content": "done\n" }),
                200,
            ),
            _ => text_answer("Done.", 200),
        }
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone(), answer).await;
    let started = start(dir.path(), &pi_bolt, &upstream).await;
    let repo: Value = started
        .post(
            "/v1/repositories",
            json!({ "name": "demo", "url": origin.to_str().unwrap() }),
        )
        .await
        .json()
        .await
        .unwrap();
    let repository_id = repo["id"].as_i64().unwrap();
    let store = &started.server.app.store;
    for (situation, advice) in [
        (
            "Changing the tokenizer",
            "Add a test with a multi-byte character",
        ),
        ("Tokenizer and emoji", "Count characters, not bytes"),
    ] {
        store
            .add_lesson(&NewLesson {
                repository_id,
                kind: "rejection".into(),
                situation: situation.into(),
                lesson: "A review sent the change back".into(),
                advice: advice.into(),
                cards: vec![],
                retro_date: None,
            })
            .unwrap();
    }
    let card: Value = started
        .post(
            &format!("/v1/repositories/{repository_id}/issues"),
            json!({ "title": "Fix the tokenizer", "body": "It breaks on emoji." }),
        )
        .await
        .json()
        .await
        .unwrap();
    let card_id = card["id"].as_i64().unwrap();
    started.wait_for_column(card_id, "Ready to merge").await;

    let requests = requests.lock().unwrap();
    let first = requests[0]["messages"].to_string();
    assert!(first.contains("Lessons from earlier cards"), "{first}");
    assert!(first.contains("Count characters, not bytes"), "{first}");
    // The tool's answer is the model's next input.
    let result = requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool" && m["tool_call_id"] == "call_1")
        .map(|m| m["content"].to_string())
        .unwrap_or_else(|| panic!("no result of the tool in {:#?}", requests[1]));
    assert!(
        result.contains("Add a test with a multi-byte character"),
        "{result}"
    );
    // The prompt and the tool each counted as a retrieval.
    assert!(
        store
            .lessons(Some(repository_id), false)
            .unwrap()
            .iter()
            .all(|l| l.retrievals == 2)
    );
}
