//! The Retro analyst's branches for Moebius's own configuration turned into proposals: each checked as what it
//! changes requires, stored with its diff and put in the inbox with the Retro's reason.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{KEYS, REPOSITORY, SETTINGS, files, unified};
use crate::app::{App, PERSON, Result};
use crate::board::{Workflow, check_script};
use crate::bus::AppEvent;
use crate::harness::repo::OwnBranch;
use crate::repos::git_bytes;
use crate::server::settings::{Changes, Live};
use crate::store::{OwnEdits, Repository};

/// The file of a branch that changes `moebius.toml`.
const SETTINGS_FILE: &str = "settings.json";
/// The directory of a branch that holds a repository's files.
const REPOSITORIES: &str = "repositories/";

/// Stores each of the analyst's `branches` of the Retro of `day` as an `untried` proposal with an inbox item, and
/// returns the warnings for those that are left out and why.
pub fn store(app: &App, live: &Live, day: &str, branches: &[OwnBranch]) -> Result<Vec<String>> {
    let mut warnings = vec![];
    for branch in branches {
        let own = match edits_of(app, live, branch) {
            Ok(own) => own,
            Err(why) => {
                warnings.push(format!("{} is left out: {why}", branch.branch));
                continue;
            }
        };
        let id = app.store.add_own_proposal(
            day,
            &branch.branch,
            &branch.title,
            &branch.body,
            &branch.cards,
            &own,
        )?;
        app.store.add_inbox_item(
            "proposal",
            None,
            &branch.title,
            &json!({
                "proposal": id,
                "branch": branch.branch,
                "kind": own.kind,
                "repository": own.repository,
                "reason": branch.body,
                "diff": own.diff,
            }),
        )?;
        app.bus.publish(AppEvent::new(
            "proposal_status",
            json!({ "proposal": id, "status": "untried" }),
        ));
    }
    Ok(warnings)
}

/// What the branch proposes, checked: a change to `moebius.toml` or to one repository's `.moebius/` files.
fn edits_of(app: &App, live: &Live, branch: &OwnBranch) -> std::result::Result<OwnEdits, String> {
    if branch.files.contains_key(SETTINGS_FILE) {
        if branch.files.len() > 1 {
            return Err(format!(
                "it changes moebius/{SETTINGS_FILE} and other files, which are two proposals"
            ));
        }
        return settings(live, &branch.files[SETTINGS_FILE]);
    }
    repository(app, &branch.files)
}

/// A change to `moebius.toml`: only keys of [`KEYS`], and a file that is still a valid configuration with it.
fn settings(live: &Live, text: &str) -> std::result::Result<OwnEdits, String> {
    let changes: Changes = serde_json::from_str(text)
        .map_err(|e| format!("moebius/{SETTINGS_FILE} is not a change of the settings: {e}"))?;
    let mut keys = changes.set.keys().chain(&changes.remove);
    if let Some(key) = keys.find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!(
            "it changes {key}, which the Retro may not change; it may change {}",
            KEYS.join(", ")
        ));
    }
    let (before, after) = live.preview(&changes).map_err(|e| e.to_string())?;
    let diff = unified("moebius.toml", &before, &after);
    if diff.is_empty() {
        return Err("it changes nothing in moebius.toml".into());
    }
    Ok(OwnEdits {
        kind: SETTINGS.into(),
        diff,
        edits: serde_json::to_value(&changes).unwrap_or_default(),
        repository: None,
    })
}

/// A change to the `.moebius/` files of one repository: each file one of [`files`], rules that can be saved and
/// scripts that pass the check of `moebius check-script`.
fn repository(
    app: &App,
    branch_files: &BTreeMap<String, String>,
) -> std::result::Result<OwnEdits, String> {
    let mut name = None;
    let mut texts = BTreeMap::new();
    for (path, text) in branch_files {
        let Some((repository, file)) = path
            .strip_prefix(REPOSITORIES)
            .and_then(|rest| rest.split_once("/.moebius/"))
        else {
            return Err(format!(
                "moebius/{path} is neither moebius/{SETTINGS_FILE} nor a file of moebius/{REPOSITORIES}<name>/.moebius/"
            ));
        };
        if name.is_some_and(|name| name != repository) {
            return Err("it changes the files of more than one repository".into());
        }
        name = Some(repository);
        if !files().iter().any(|f| f == file) {
            return Err(format!(
                ".moebius/{file} is not a file the Retro may change; the files are {}",
                files().join(", ")
            ));
        }
        check(file, text).map_err(|why| format!(".moebius/{file}: {why}"))?;
        texts.insert(file, text);
    }
    let name = name.ok_or("it changes nothing")?;
    let repository = app
        .store
        .repositories()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.name == name)
        .ok_or_else(|| format!("there is no repository {name}"))?;
    let mut diff = String::new();
    let mut edits = serde_json::Map::new();
    for (file, text) in texts {
        let before = current(&repository, file);
        let path = format!(".moebius/{file}");
        diff += &unified(&path, &before, text);
        edits.insert(path, Value::String(text.clone()));
    }
    if diff.is_empty() {
        return Err(format!("it changes nothing in {name}"));
    }
    Ok(OwnEdits {
        kind: REPOSITORY.into(),
        diff,
        edits: json!({ "files": edits }),
        repository: Some(repository.id),
    })
}

/// The text of `.moebius/<file>` on the repository's default branch, as it is; empty when it has none.
fn current(repository: &Repository, file: &str) -> String {
    let spec = format!("{}:.moebius/{file}", repository.base_ref());
    git_bytes(std::path::Path::new(&repository.path), &["show", &spec])
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

/// Whether the text of `.moebius/<file>` could be saved: rules as `PUT /v1/rules` takes them, a column's script as
/// `moebius check-script` checks it.
fn check(file: &str, text: &str) -> std::result::Result<(), String> {
    if file == "rules.md" {
        return crate::decide::rules::check(text);
    }
    let column = Workflow::default()
        .columns
        .into_iter()
        .find(|c| c.script == file)
        .ok_or_else(|| format!("{file} is no column's script"))?;
    check_script(
        &column.script,
        text,
        &column.name,
        "A card to check the script with",
        PERSON,
    )
    .map(drop)
}
