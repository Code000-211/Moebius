//! A replay made and recorded: the copy made, the call run in it and the copy removed, the sentence that becomes the
//! judgement's outcome, and the replay stored and published, or why it could not be run.

use std::time::Duration;

use serde_json::json;

use super::call::{Call, call_of};
use super::copy::{Changes, compare, fill, listing};
use super::process::run_in_copy;
use crate::app::{App, Result, tail};
use crate::bus::AppEvent;
use crate::store::{Judgement, Replay};

/// `1 file`, `3 files`.
fn files(count: usize) -> String {
    match count {
        1 => "1 file".into(),
        n => format!("{n} files"),
    }
}

/// The sentence that is the judgement's outcome.
fn sentence(exit: Option<i32>, timed_out: bool, changes: &Changes) -> String {
    let ended = match (timed_out, exit) {
        (true, _) => "timed out".to_string(),
        (false, Some(code)) => format!("exit {code}"),
        (false, None) => "ended by a signal".to_string(),
    };
    let parts: Vec<String> = [
        ("created", changes.created.len()),
        ("changed", changes.changed.len()),
        ("deleted", changes.deleted.len()),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(what, count)| format!("{what} {}", files(count)))
    .collect();
    let what = if parts.is_empty() {
        "no file changed".to_string()
    } else {
        parts.join(", ")
    };
    format!("replayed: {ended}; {what}")
}

/// The replay of the judgement `id` to store: how the call ended, what it printed and what it did to the files.
fn record(
    id: i64,
    call: &Call,
    (exit, timed_out): (Option<i32>, bool),
    output: String,
    changes: Changes,
) -> Replay {
    Replay {
        id: 0,
        judgement: id,
        run: call.run.id,
        exit,
        timed_out,
        output,
        created: changes.created,
        changed: changes.changed,
        deleted: changes.deleted,
        at: 0,
    }
}

/// Makes the copy, runs the call in it and removes the copy: the replay to store and the outcome, or why it could not
/// be run.
fn try_replay(
    app: &App,
    call: &Call,
    id: i64,
    limit: Duration,
) -> std::result::Result<(Replay, String), String> {
    let directory = app.data_dir.join("replays");
    let (copy, log, tarball) = (
        directory.join(id.to_string()),
        directory.join(format!("{id}.log")),
        directory.join(format!("{id}.tar")),
    );
    let _ = std::fs::remove_dir_all(&copy);
    let made = std::fs::create_dir_all(&copy)
        .map_err(|e| format!("cannot make the copy: {e}"))
        .and_then(|()| fill(app, call, &copy, &tarball))
        .and_then(|()| {
            let before = listing(&copy).map_err(|e| format!("cannot list the copy: {e}"))?;
            let ended = run_in_copy(call, &copy, &log, limit)
                .map_err(|e| format!("cannot run the call: {e}"))?;
            let after = listing(&copy).map_err(|e| format!("cannot list the copy: {e}"))?;
            Ok((ended, compare(&before, &after)))
        });
    let output = tail(&log);
    let _ = std::fs::remove_dir_all(&copy);
    let _ = std::fs::remove_file(&log);
    let _ = std::fs::remove_file(&tarball);
    let ((exit, timed_out), changes) = made?;
    let outcome = sentence(exit, timed_out, &changes);
    Ok((
        record(id, call, (exit, timed_out), output, changes),
        outcome,
    ))
}

/// Replays the judgement's call, stores the replay, sets the outcome and publishes `replayed`.
pub(super) fn replay_judgement(
    app: &App,
    judgement: &Judgement,
    limit: Duration,
) -> Result<Replay> {
    let call = call_of(app, judgement)?;
    let (replay, outcome) = try_replay(app, &call, judgement.id, limit).unwrap_or_else(|why| {
        let why = crate::blobs::redact(&why.replace('\n', " "));
        let outcome = format!("replayed: not run; {why}");
        let replay = record(judgement.id, &call, (None, false), why, Changes::default());
        (replay, outcome)
    });
    let replay = app.store.add_replay(replay)?;
    app.store.set_outcome(judgement.id, &outcome)?;
    app.bus.publish(AppEvent::new(
        "replayed",
        json!({ "judgement": judgement.id, "run": replay.run, "outcome": outcome }),
    ));
    Ok(replay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_outcome_says_how_the_call_ended_and_what_it_did_to_the_files() {
        let changes = |c: usize, m: usize, d: usize| Changes {
            created: vec!["f".into(); c],
            changed: vec!["f".into(); m],
            deleted: vec!["f".into(); d],
        };
        assert_eq!(
            sentence(Some(0), false, &changes(0, 0, 3)),
            "replayed: exit 0; deleted 3 files"
        );
        assert_eq!(
            sentence(Some(2), false, &changes(1, 1, 1)),
            "replayed: exit 2; created 1 file, changed 1 file, deleted 1 file"
        );
        assert_eq!(
            sentence(Some(0), false, &changes(0, 0, 0)),
            "replayed: exit 0; no file changed"
        );
        assert_eq!(
            sentence(None, true, &changes(0, 2, 0)),
            "replayed: timed out; changed 2 files"
        );
        assert_eq!(
            sentence(None, false, &changes(0, 0, 0)),
            "replayed: ended by a signal; no file changed"
        );
    }
}
