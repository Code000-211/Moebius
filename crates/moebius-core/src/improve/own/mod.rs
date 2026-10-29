//! The Retro's proposals that change Moebius's own configuration rather than the harness configuration (spec 7): a
//! change to `moebius.toml` (kind `settings`) or to a repository's own rules and column scripts in its `.moebius/`
//! (kind `repository`).
//!
//! The Retro analyst states one as it states any proposal, as a branch `proposal/<date>-<name>` of the harness
//! configuration clone with one commit, but the branch holds only files under `moebius/`: `moebius/settings.json`, a
//! change as `PUT /v1/settings` takes it, limited to [`KEYS`], or `moebius/repositories/<name>/.moebius/<file>`, the
//! whole new text of one of the [`files`] of one repository. Such a branch is read and deleted, never pushed
//! (`HarnessRepo::take_own_branches`). Each proposal is checked, stored with a diff of what it changes, `untried`, and
//! put in the inbox at once with the Retro's reason: trials do not apply to it. Nothing is applied without a person:
//! merging a `settings` proposal saves the change through the settings code, and the server restarts when a key needs
//! it; merging a `repository` proposal offers the change on the repository through its forge, to be merged as any
//! change is.
//!
//! This module holds what the analyst is told about it (`brief`); `read` turns the branches into proposals, `apply`
//! merges and dismisses them, and `diff` shows what one changes.

mod apply;
mod diff;
mod read;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::retro::Report;
use crate::app::{App, Result};
use crate::board::{Workflow, repository_script};
use crate::server::settings::Live;

pub use apply::{dismiss, merge};
pub use diff::unified;
pub use read::store;

/// The kind of a proposal that changes `moebius.toml`.
pub const SETTINGS: &str = "settings";
/// The kind of a proposal that changes a repository's `.moebius/` files.
pub const REPOSITORY: &str = "repository";

/// The keys of `moebius.toml` the Retro may propose to set or remove: limits, budgets, windows and the teacher, never
/// where the server listens, its repositories, secrets or agents.
pub const KEYS: &[&str] = &[
    "run_limit",
    "retro_at",
    "trial_window.start",
    "trial_window.end",
    "board.round_limit",
    "budgets.day_tokens",
    "checks.timeout_seconds",
    "teacher.model",
    "teacher.protocol",
    "teacher.timeout_seconds",
];

/// The files of a repository's `.moebius/` the Retro may propose to change: its rules and its column scripts.
pub fn files() -> Vec<String> {
    let mut files = vec!["rules.md".to_string()];
    files.extend(Workflow::default().columns.into_iter().map(|c| c.script));
    files
}

/// What the Retro analyst is told about proposals for Moebius's own configuration: how to state them, the values of
/// [`KEYS`] now, and the `.moebius/` files the repositories of the report's cards have now.
pub fn brief(app: &App, live: &Live, report: &Report) -> Result<String> {
    let values = live.view()?.values;
    let values: BTreeMap<&str, &Value> = KEYS
        .iter()
        .filter_map(|key| Some((*key, &values.get(*key)?["value"])))
        .collect();
    let cards: BTreeSet<i64> = report.errors.iter().filter_map(|e| e.card).collect();
    let mut repositories = BTreeSet::new();
    for card in cards {
        if let Some(card) = app.store.card(card)? {
            repositories.insert(card.repository_id);
        }
    }
    let mut own = serde_json::Map::new();
    for id in repositories {
        let Some(repository) = app.store.repository(id)? else {
            continue;
        };
        let texts: serde_json::Map<String, Value> = files()
            .into_iter()
            .filter_map(|file| {
                let text = repository_script(&repository, &file)?;
                Some((format!(".moebius/{file}"), json!(text)))
            })
            .collect();
        own.insert(repository.name.clone(), Value::Object(texts));
    }
    Ok(format!(
        "Some errors are better avoided by Moebius's own configuration than by the harness configuration. You may \
         propose that too, each proposal again its own branch proposal/{day}-<short-name> with one commit whose \
         message is as above, but whose commit adds only files under moebius/, which is not harness configuration:\n\
         - To change moebius.toml, add only moebius/settings.json: a JSON object {{\"set\": {{\"<key>\": <value>}}, \
         \"remove\": [\"<key>\"]}} with dotted keys, as PUT /v1/settings of the desktop API takes it. Only these keys \
         may be in it; their values now are:\n{values}\n\
         - To change a repository's own rules or column scripts, add only files moebius/repositories/<repository>/\
         .moebius/<file>, each the whole new text of that file, all for one repository. The files are {files}. A \
         column without its own script runs the built-in one (see the column-scripts reference). The repositories of \
         the report's cards and the files they have now are:\n{own}\n\
         These are never tried: a person reads each as a diff with your reason and decides.\n",
        day = report.date,
        values = serde_json::to_string_pretty(&values).unwrap_or_default(),
        files = files().join(", "),
        own = serde_json::to_string_pretty(&own).unwrap_or_default(),
    ))
}
