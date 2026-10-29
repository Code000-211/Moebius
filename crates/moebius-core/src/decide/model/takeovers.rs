//! A person's decision about a takeover, recorded as a `takeover` judgement: an open one merged, which routes its kind
//! to the model, or dismissed, and a merged one released, which hands the kind back.

use std::collections::BTreeMap;

use serde_json::json;

use crate::app::{App, AppError, Result};
use crate::bus::AppEvent;
use crate::decide::{Actor, Question};
use crate::store::Takeover;

fn publish_status(app: &App, takeover: i64, status: &str) {
    app.bus.publish(AppEvent::new(
        "takeover_status",
        json!({ "takeover": takeover, "status": status }),
    ));
}

/// Records the person's decision about a takeover, then moves it from `from` to `to`.
fn decide_takeover(
    app: &App,
    by: &Actor,
    id: i64,
    from: &str,
    (choice, other): (&str, &str),
    reason: &str,
    to: &str,
) -> Result<Takeover> {
    let takeover = app
        .store
        .takeover(id)?
        .ok_or_else(|| AppError::NotFound(format!("takeover {id}")))?;
    if takeover.status != from {
        return Err(AppError::Conflict(format!(
            "takeover {id} is {}, not {from}",
            takeover.status
        )));
    }
    app.decisions.record_by(
        by,
        &Question {
            kind: "takeover".into(),
            context: BTreeMap::from([
                ("takeover".to_string(), id.to_string()),
                ("kind".to_string(), takeover.kind.clone()),
                ("agreement".to_string(), takeover.agreement.to_string()),
                ("count".to_string(), takeover.count.to_string()),
            ]),
            options: vec![choice.into(), other.into()],
            fail_closed: other.into(),
            ..Default::default()
        },
        choice,
        reason,
    )?;
    if !app.store.move_takeover(id, from, to)? {
        return Err(AppError::Conflict(format!(
            "takeover {id} is not {from} any more"
        )));
    }
    app.store.resolve_inbox_for_takeover(id)?;
    publish_status(app, id, to);
    Ok(Takeover {
        status: to.into(),
        ..takeover
    })
}

/// A person merges an open takeover: from now on the decision model decides the kind.
pub fn merge(app: &App, by: &Actor, id: i64, reason: &str) -> Result<Takeover> {
    let takeover = decide_takeover(app, by, id, "open", ("merge", "dismiss"), reason, "merged")?;
    app.decisions.routed().add(&takeover.kind);
    Ok(takeover)
}

/// A person dismisses an open takeover.
pub fn dismiss(app: &App, by: &Actor, id: i64, reason: &str) -> Result<Takeover> {
    decide_takeover(
        app,
        by,
        id,
        "open",
        ("dismiss", "merge"),
        reason,
        "dismissed",
    )
}

/// A person hands a merged kind back to the decider it had before.
pub fn release(app: &App, by: &Actor, id: i64, reason: &str) -> Result<Takeover> {
    let takeover = decide_takeover(
        app,
        by,
        id,
        "merged",
        ("release", "keep"),
        reason,
        "released",
    )?;
    app.decisions.routed().remove(&takeover.kind);
    Ok(takeover)
}
