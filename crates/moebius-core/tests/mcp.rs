//! MCP servers (issues #40 and #97): a real `pi-bolt` runs an agent whose server is a small stdio MCP server, which it
//! reaches only through the MCP gateway. It calls the server's tool; the call passes through the bridge's check like any
//! tool call and then through the gateway's, which records it, and the tool's result reaches the model.
//!
//! The model is a recorded OpenAI-compatible upstream that calls the tool Pi exposes for the server's `echo` and then
//! says "Done.". Set `MOEBIUS_PI_BOLT` to the `pi-bolt` executable to run the test; without it the test is skipped.

mod common;

use std::path::Path;
use std::sync::{Arc, Mutex};

use common::{
    last_message, origin, recorded_upstream, start_harness, text_answer, tool_call_answer,
};
use moebius_core::repos::git;
use serde_json::{Value, json};

/// Pi names the tools of an MCP server `mcp__<server>__<tool>`; its only server is the gateway, `moebius`, which names
/// the tools of the agent's servers `<server>__<tool>`.
const TOOL: &str = "mcp__moebius__echo__echo";

/// The recorded answer to a request: a call of the MCP tool first, then "Done." once its result is in.
fn answer(body: &Value) -> String {
    if last_message(body)["role"] == "tool" {
        text_answer("Done.", 300)
    } else {
        tool_call_answer("call_1", TOOL, &json!({ "text": "hello" }), 250)
    }
}

/// A harness configuration repository whose Coder has the server `echo`, which is the `echo-mcp` script.
fn harness(dir: &Path) -> std::path::PathBuf {
    let work = dir.join("harness");
    std::fs::create_dir_all(work.join("agents")).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(
        work.join("agents/Coder.toml"),
        "model = \"fake-coder\"\ntoken_budget = 100000\nmcp = [\"echo\"]\n",
    )
    .unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/echo-mcp");
    std::fs::write(
        work.join("mcp.toml"),
        format!("[echo]\ncommand = {:?}\n", script.display().to_string()),
    )
    .unwrap();
    git(&work, &["add", "-A"]).unwrap();
    git(&work, &["commit", "--quiet", "-m", "Start"]).unwrap();
    work
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_of_the_agents_mcp_server_is_called_through_the_gateway_by_a_real_pi_bolt() {
    let Some(pi_bolt) = std::env::var_os("MOEBIUS_PI_BOLT") else {
        eprintln!("skipped: set MOEBIUS_PI_BOLT to the pi-bolt executable to run this test");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let origin = origin(dir.path());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let upstream = recorded_upstream(requests.clone(), answer).await;
    let started = start_harness(dir.path(), &pi_bolt, &upstream, &harness(dir.path())).await;
    let (_, card) = started
        .add_issue(&origin, "Echo", "Call the echo tool.")
        .await;
    let card_id = card["id"].as_i64().unwrap();

    // The tool call does not change the repository, so the run has failed; what counts is what it did.
    let run = loop {
        if let Some(run) = started
            .server
            .app
            .store
            .runs_for_card(card_id)
            .unwrap()
            .into_iter()
            .find(|run| run.ended_at.is_some())
        {
            break run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };

    let requests = requests.lock().unwrap();
    assert!(
        requests[0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["function"]["name"] == TOOL),
        "{:#}",
        requests[0]["tools"]
    );
    let result = last_message(requests.last().unwrap());
    assert_eq!(result["role"], "tool");
    assert!(
        result["content"].to_string().contains("echo: hello"),
        "{result}"
    );

    let events = started.server.app.store.events(run.id).unwrap();
    let checked: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "tool_check" && event.data["tool"] == TOOL)
        .collect();
    assert_eq!(checked.len(), 1, "{events:#?}");
    assert_eq!(checked[0].data["decision"], "allow");

    // The gateway checked the call under the server's name, and recorded it with the server's answer.
    let calls: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "mcp_call")
        .collect();
    assert_eq!(calls.len(), 1, "{events:#?}");
    assert_eq!(calls[0].data["server"], "echo");
    assert_eq!(calls[0].data["decision"], "allow");
    assert_eq!(calls[0].data["result"]["content"][0]["text"], "echo: hello");
    let judged = started.server.app.store.judgements("tool_call").unwrap();
    assert_eq!(judged.len(), 2, "the bridge's check and the gateway's");
}
