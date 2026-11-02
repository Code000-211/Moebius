//! A person's decision about a takeover, recorded as a `takeover` judgement: an open one merged, which routes its kind
//! to the model, or dismissed, and a merged one released, which hands the kind back.

use std::collections::BTreeMap;

use crate::app::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::{Actor, Question};
use crate::store::Takeover;

fn publish_status(app: &App, takeover: i64, status: &str) {
    app.bus.publish(Event::TakeoverStatus {
        takeover,
        status: status.to_string(),
    });
}

/// Moves a takeover from `from` to `to` and records the person's decision only when that move succeeded (see
/// `move_then_record`).
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
    move_then_record(app, by, takeover, from, (choice, other), reason, to)
}

/// Moves `takeover` from `from` to `to` and, only when this call made the move, records the decision: of two
/// concurrent decisions about one takeover only the one that moved it leaves a judgement.
fn move_then_record(
    app: &App,
    by: &Actor,
    takeover: Takeover,
    from: &str,
    (choice, other): (&str, &str),
    reason: &str,
    to: &str,
) -> Result<Takeover> {
    let id = takeover.id;
    if !app.store.move_takeover(id, from, to)? {
        return Err(AppError::Conflict(format!(
            "takeover {id} is not {from} any more"
        )));
    }
    let recorded = app.decisions.record_by(
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
    );
    if let Err(error) = recorded {
        // Without its judgement the decision did not happen: the takeover is as it was.
        let _ = app.store.move_takeover(id, to, from);
        return Err(error.into());
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::Store;

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let app = App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(5))),
            dir.path(),
        );
        (dir, app)
    }

    #[test]
    fn a_decision_that_lost_the_race_leaves_no_judgement() {
        let (_dir, app) = app();
        let takeover = app.store.add_takeover("merge", 0.99, 20).unwrap();
        let person = Actor::Person;
        // Another call read the takeover as open too, and then dismissed it first.
        let stale = app.store.takeover(takeover.id).unwrap().unwrap();
        dismiss(&app, &person, takeover.id, "no").unwrap();
        let lost = move_then_record(
            &app,
            &person,
            stale,
            "open",
            ("merge", "dismiss"),
            "yes",
            "merged",
        );
        assert!(matches!(lost, Err(AppError::Conflict(_))));
        let judgements = app.store.judgements("takeover").unwrap();
        assert_eq!(judgements.len(), 1);
        assert_eq!(judgements[0].choice, "dismiss");
        assert_eq!(
            app.store.takeover(takeover.id).unwrap().unwrap().status,
            "dismissed"
        );
    }
}
