//! Ending a run, whatever its harness: the run's summary and events are kept as its records arrive (`keep`), and when its
//! process is gone `finish` commits what the agent left (a chat's work is offered as a change, a grilling's result
//! read), revokes the run's token and publishes the end. It removes the worktree, except for a workflow run, whose
//! worktree and branch stay for the run's later agents.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::GRILLING_AGENT;
use super::lifecycle::RunningRun;
use crate::app::App;
use crate::bus::Event;
use crate::router::Router;

/// The text of the last assistant message in an `agent_end` record, if it has any.
pub(crate) fn final_answer(agent_end: &Value) -> Option<String> {
    let messages = agent_end["messages"].as_array()?;
    let last = messages.iter().rev().find(|m| m["role"] == "assistant")?;
    let text: String = last["content"]
        .as_array()?
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    Some(text)
}

/// Stores what a record of the agent's harness says about the run (the final answer as its summary; for a chat, whether the agent works
/// or waits for the person), then the record as a run event. The run is updated before the event is published, so
/// that a client acting on the event reads the run as it is now.
pub(in crate::harness) fn keep(app: &App, run_id: i64, chat: bool, kind: &str, value: &Value) {
    if kind == "agent_end"
        && let Some(answer) = final_answer(value)
    {
        let _ = app.store.set_run_summary(run_id, &answer);
    }
    if chat && kind == "agent_start" {
        let _ = app.store.set_run_status(run_id, "running");
    }
    // A chat waits for the person's next message; only an abort ends it.
    if chat && kind == "agent_settled" && value["aborted"] != true {
        let _ = app.store.set_run_status(run_id, "waiting");
    }
    let _ = app.record_run_event(run_id, kind, value);
}

/// Ends a run whose Pi is gone: `Ok(aborted)` when it settled, `Err(why)` when it failed. What the agent left is
/// committed (and a chat's work offered as a change), the worktree removed, the token revoked and the end published.
pub(in crate::harness) fn finish<L>(
    app: &Arc<App>,
    run: RunningRun,
    router: &Router,
    lives: &Mutex<HashMap<i64, L>>,
    ending: Result<bool, String>,
) {
    router.revoke_token(&run.token);
    // What Moebius commits of a run's work: a chat's commits say what the chat did (see
    // `crate::app::App::chat_commit_message`), never just the chat's first message; a run of a card or a workflow
    // keeps the message it started with.
    let commit_message = if run.purpose == "chat" {
        app.run(run.id).map_or_else(
            |_| run.commit_message.clone(),
            |chat| app.chat_commit_message(&chat, "when the chat ended"),
        )
    } else {
        run.commit_message.clone()
    };
    // What the agent did on a runner is brought into the worktree first, so that all that follows sees it.
    let ending = ending.and_then(|aborted| run.bring_back(&commit_message).map(|()| aborted));
    let failed = |why: &str| {
        let _ = app.record_run_event(run.id, "moebius_error", &json!({ "message": why }));
        "failed"
    };
    // A grilling leaves its result for Moebius to read, not a change to offer.
    let grilling =
        run.purpose == "chat" && app.run(run.id).is_ok_and(|run| run.agent == GRILLING_AGENT);
    let status = match ending {
        Ok(aborted) if grilling => {
            if aborted {
                "stopped"
            } else {
                "succeeded"
            }
        }
        Ok(aborted) if run.purpose == "chat" => match run.commit(&commit_message) {
            // A chat that changed nothing has still succeeded.
            Ok(_) if aborted => "stopped",
            Ok(_) => "succeeded",
            Err(e) => failed(&e.to_string()),
        },
        Ok(true) => "stopped",
        Ok(false) if run.purpose == "workflow" => match run.commit(&run.commit_message) {
            // An agent of a workflow run may answer only, leaving the branch as it found it.
            Ok(_) => "succeeded",
            Err(e) => failed(&e.to_string()),
        },
        Ok(false) if !matches!(run.purpose.as_str(), "code" | "trial" | "attempt") => "succeeded",
        Ok(false) => match run.commit(&run.commit_message) {
            Ok(true) => "succeeded",
            Ok(false) => failed("the agent changed nothing"),
            Err(e) => failed(&e.to_string()),
        },
        Err(why) => failed(&why),
    };
    if grilling && status != "failed" {
        // Before the worktree is removed, with the result in it. What the person published already is kept.
        match app.store.grilling_for_run(run.id) {
            Err(e) => {
                failed(&e.to_string());
            }
            Ok(None) => {
                if let Err(e) = crate::harness::grilling::conclude(app, run.id, &run.worktree) {
                    failed(&format!("cannot read the result of the grilling: {e}"));
                }
            }
            Ok(Some(_)) => {}
        }
    }
    if run.purpose == "code" && status == "succeeded" {
        // Before the worktree is removed, with the change in it.
        app.localize_run_ended(run.id, &run.worktree);
    }
    if run.purpose == "workflow" {
        // The workflow run keeps its worktree, on the branch its run is named for: the run's later agents reuse it.
        app.terminals.close(run.id);
    } else {
        run.remove_worktree(app);
    }
    if run.purpose == "chat" && !grilling && status != "failed" {
        // Before the status is set, so that an ended chat has offered what it made.
        if let Err(e) = app.offer_chat_change(run.id) {
            failed(&format!("cannot offer the chat's work as a change: {e}"));
        }
    }
    // Before the status is set, so that a run that reads as ended can no longer be steered or stopped.
    lives
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&run.id);
    let _ = app.store.set_run_status(run.id, status);
    app.bus.publish(Event::RunEnded {
        run: run.id,
        status: status.into(),
    });
}
