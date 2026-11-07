//! The Retro analyst's branches for Moebius's own configuration turned into proposals: each checked as what it
//! changes requires, stored with its diff and put in the inbox with the Retro's reason.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{KEYS, REPOSITORY, SETTINGS, files, unified};
use crate::app::{App, PERSON, Result};
use crate::board::{Kanban, check_script};
use crate::bus::Event;
use crate::harness::repo::OwnBranch;
use crate::server::settings::{Changes, Live};
use crate::store::OwnEdits;

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
        app.bus.publish(Event::ProposalStatus {
            proposal: id,
            status: "untried".into(),
        });
    }
    Ok(warnings)
}

/// What the branch proposes, checked: a change to `moebius.toml` or to one project's settings files.
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
    let diff = crate::blobs::redact(&strip_url_credentials(&unified(
        "moebius.toml",
        &before,
        &after,
    )));
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

/// `text` with the `user:password@` of every URL in it removed, since the lines around a change show URLs as they are
/// written in `moebius.toml`.
fn strip_url_credentials(text: &str) -> String {
    static CREDENTIALS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"([A-Za-z][A-Za-z0-9+.-]*://)[^/\s@]+@").expect("pattern compiles")
    });
    CREDENTIALS.replace_all(text, "$1").into_owned()
}

/// A change to the settings files of one project: each file one of [`files`], rules that can be saved and
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
            .and_then(|rest| rest.split_once('/'))
        else {
            return Err(format!(
                "moebius/{path} is neither moebius/{SETTINGS_FILE} nor a file of moebius/{REPOSITORIES}<name>/"
            ));
        };
        if name.is_some_and(|name| name != repository) {
            return Err("it changes the files of more than one repository".into());
        }
        name = Some(repository);
        if !files().iter().any(|f| f == file) {
            return Err(format!(
                "{file} is not a file the Retro may change; the files are {}",
                files().join(", ")
            ));
        }
        check(file, text).map_err(|why| format!("{file}: {why}"))?;
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
        let before = app.project_file(&repository, file).unwrap_or_default();
        let path = format!("repos/{}/{file}", repository.name);
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

/// Whether the text of the project's `<file>` could be saved: rules as `PUT /v1/rules` takes them, a column's script as
/// `moebius check-script` checks it.
fn check(file: &str, text: &str) -> std::result::Result<(), String> {
    if file == "rules.md" {
        return crate::decide::rules::check(text);
    }
    let column = Kanban::default()
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

#[cfg(test)]
mod tests {
    use super::{settings, strip_url_credentials};
    use crate::config::Config;
    use crate::server::settings::{Changes, Live};

    #[test]
    fn a_settings_proposal_shows_no_url_password_and_no_token() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("moebius.toml");
        let token = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        std::fs::write(
            &file,
            format!(
                "# proxy https://leon:hunter2@proxy.example/v1\n# token {token}\nrun_limit = 3\n# end\n"
            ),
        )
        .unwrap();
        let live = Live::new(Config::load(&file).unwrap());
        let changes = Changes {
            set: [("run_limit".to_string(), serde_json::json!(4))].into(),
            remove: vec![],
            ..Changes::default()
        };
        let edits = settings(&live, &serde_json::to_string(&changes).unwrap()).unwrap();
        assert!(edits.diff.contains("+run_limit = 4"), "{}", edits.diff);
        assert!(
            edits.diff.contains("https://proxy.example/v1"),
            "{}",
            edits.diff
        );
        assert!(!edits.diff.contains("hunter2"), "{}", edits.diff);
        assert!(!edits.diff.contains("abcdefghij"), "{}", edits.diff);
    }

    #[test]
    fn the_credentials_of_a_url_are_removed_from_a_diff() {
        let diff = " base_url = \"https://leon:hunter2@proxy.example/v1\"\n+timeout = 5\n url = \"http://token@host:8080/x\"\n";
        assert_eq!(
            strip_url_credentials(diff),
            " base_url = \"https://proxy.example/v1\"\n+timeout = 5\n url = \"http://host:8080/x\"\n"
        );
    }
}
