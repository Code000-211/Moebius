//! The software factory (ADR 0031): what lets a server process other people's pull requests with as little of the
//! owner's work as possible, and still never merge what the owner's rules do not allow.
//!
//! - [`gate`]: the one gate every merge the server makes goes through (`App::merge_one`). It refuses with a reason and
//!   never fails open; a refusal puts the card in Ready to merge with an Inbox item that names the reason.
//! - [`freeze`]: a frozen repository starts no review round, merges nothing and starts no agent run. A person freezes
//!   it, the label `factory:freeze` freezes it, or a breaker does; only a person lifts what a person or a breaker froze.
//! - [`paths`]: the globs of the paths that never merge without a person.
//!
//! This file holds the settings a repository keeps for all of it, which a person edits with the repository's other
//! settings (`App::edit_working_with_others`), and the names the parts share.

pub mod freeze;
pub mod gate;
pub mod paths;

use super::{AppError, Result};
use crate::store::{FactorySettings, Repository};

/// The label that freezes a repository while it is on an issue.
pub const FREEZE_LABEL: &str = "factory:freeze";
/// The label of an issue whose change merges without a measurement.
pub const NO_MEASUREMENT_LABEL: &str = "no-measurement";
/// The kind of the inbox item that says why the server did not merge a change.
pub const MERGE_REFUSED: &str = "merge_refused";
/// The kind of the inbox item that says a repository is frozen.
pub const FACTORY_FROZEN: &str = "factory_frozen";
/// The kind of the decision whether the repository's rules allow a merge.
pub const MERGE_CHECK: &str = "merge_check";

/// How many entries each list setting may have, and how long an entry may be.
const LIST_AT_MOST: usize = 100;
const ENTRY_AT_MOST: usize = 200;

/// The factory settings of `repository` as they are now.
pub fn settings_of(repository: &Repository) -> FactorySettings {
    FactorySettings {
        require_measurement: repository.require_measurement,
        locked_paths: repository.locked_paths.clone(),
        review_brief: repository.review_brief.clone(),
        testing_label: repository.testing_label.clone(),
        measurers: repository.measurers.clone(),
        measurement_free_paths: repository.measurement_free_paths.clone(),
    }
}

/// The entries of a list setting, trimmed, without blanks or repeats; `what` names the setting in an error.
fn clean_list(
    what: &str,
    entries: &[String],
    fit: impl Fn(&str) -> std::result::Result<(), String>,
) -> Result<Vec<String>> {
    let mut clean: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() || clean.iter().any(|known| known == entry) {
            continue;
        }
        if entry.len() > ENTRY_AT_MOST || entry.contains(['\n', '\r', '\0']) {
            return Err(AppError::Invalid(format!(
                "{what}: {entry:?} is not an entry"
            )));
        }
        fit(entry).map_err(|why| AppError::Invalid(format!("{what}: {entry:?} {why}")))?;
        clean.push(entry.to_string());
    }
    if clean.len() > LIST_AT_MOST {
        return Err(AppError::Invalid(format!(
            "{what} has at most {LIST_AT_MOST} entries"
        )));
    }
    Ok(clean)
}

/// A file of the default branch: a path in the repository, never above it.
fn is_repository_path(path: &str) -> std::result::Result<(), String> {
    if path.starts_with('/') || path.contains('\\') || path.split('/').any(|name| name == "..") {
        return Err("is not a path inside the repository".into());
    }
    Ok(())
}

/// A login on a forge.
fn is_login(login: &str) -> std::result::Result<(), String> {
    if login
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        Ok(())
    } else {
        Err("is not a login".into())
    }
}

/// The settings `current` becomes when `edit` is applied; what is none stays. Refused, naming the setting, when a value
/// does not fit.
pub fn edited(current: &FactorySettings, edit: &FactoryEdit) -> Result<FactorySettings> {
    let label = edit
        .testing_label
        .as_deref()
        .map_or(current.testing_label.clone(), |l| l.trim().to_string());
    if label.is_empty() || label.len() > 50 || label.contains([',', '\n']) {
        return Err(AppError::Invalid(format!("{label:?} is not a label name")));
    }
    let locked = match &edit.locked_paths {
        Some(paths) => clean_list("locked_paths", paths, |_| Ok(()))?,
        None => current.locked_paths.clone(),
    };
    let brief = match &edit.review_brief {
        Some(paths) => clean_list("review_brief", paths, is_repository_path)?,
        None => current.review_brief.clone(),
    };
    let measurers = match &edit.measurers {
        Some(logins) => clean_list("measurers", logins, is_login)?,
        None => current.measurers.clone(),
    };
    let free = match &edit.measurement_free_paths {
        Some(paths) => clean_list("measurement_free_paths", paths, is_repository_path)?,
        None => current.measurement_free_paths.clone(),
    };
    Ok(FactorySettings {
        require_measurement: edit
            .require_measurement
            .unwrap_or(current.require_measurement),
        locked_paths: locked,
        review_brief: brief,
        testing_label: label,
        measurers,
        measurement_free_paths: free,
    })
}

impl super::App {
    /// A person accepts a measurement for the current head of the change: the merge gate then lets a repository that
    /// requires one merge this version of it. A later push makes a new head, which needs its own. Refused for an agent
    /// or a script, whose word is not a measurement, and for a change that is not open or has no known head.
    pub fn accept_measurement(
        &self,
        by: &crate::decide::Actor,
        change_id: i64,
        reason: &str,
    ) -> Result<crate::store::Change> {
        if !by.is_person() {
            return Err(AppError::Forbidden(
                "only a person accepts a measurement".into(),
            ));
        }
        let change = self.change(change_id)?;
        if change.state != "open" {
            return Err(AppError::Invalid(format!(
                "change {change_id} is {}",
                change.state
            )));
        }
        let head = change
            .head
            .clone()
            .ok_or_else(|| AppError::Invalid("the head of the change is not known yet".into()))?;
        self.store
            .add_measurement(change.id, &head, &by.decider(), "person")?;
        self.record_action(
            by,
            "accept_measurement",
            change.card_id,
            None,
            std::collections::BTreeMap::from([
                ("change".into(), change.id.to_string()),
                ("head".into(), head),
            ]),
            reason,
        )?;
        let change = self.change(change_id)?;
        self.bus
            .publish(crate::bus::Event::ChangeUpdated(change.clone()));
        Ok(change)
    }
}

/// An edit of a repository's factory settings; what is none stays as it is.
#[derive(Debug, Clone, Default)]
pub struct FactoryEdit {
    pub require_measurement: Option<bool>,
    pub locked_paths: Option<Vec<String>>,
    pub review_brief: Option<Vec<String>>,
    pub testing_label: Option<String>,
    pub measurers: Option<Vec<String>>,
    pub measurement_free_paths: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> FactorySettings {
        FactorySettings {
            require_measurement: false,
            locked_paths: Vec::new(),
            review_brief: vec!["REVIEW.md".into()],
            testing_label: "ready-for-testing".into(),
            measurers: Vec::new(),
            measurement_free_paths: Vec::new(),
        }
    }

    #[test]
    fn an_edit_changes_what_it_names_and_cleans_the_lists() {
        let edit = FactoryEdit {
            require_measurement: Some(true),
            locked_paths: Some(vec![" kernels/** ".into(), "".into(), "kernels/**".into()]),
            review_brief: Some(vec!["notes/backlog/CARDS.md".into(), "AGENTS.md".into()]),
            measurers: Some(vec!["ada".into()]),
            ..Default::default()
        };
        let after = edited(&defaults(), &edit).unwrap();
        assert!(after.require_measurement);
        assert_eq!(after.locked_paths, vec!["kernels/**".to_string()]);
        assert_eq!(after.review_brief.len(), 2);
        assert_eq!(after.measurers, vec!["ada".to_string()]);
    }

    #[test]
    fn a_value_that_does_not_fit_is_refused_naming_the_setting() {
        let refused = |edit: FactoryEdit| edited(&defaults(), &edit).unwrap_err().to_string();
        assert!(
            refused(FactoryEdit {
                review_brief: Some(vec!["../secret".into()]),
                ..Default::default()
            })
            .contains("review_brief")
        );
        assert!(
            refused(FactoryEdit {
                review_brief: Some(vec!["/etc/passwd".into()]),
                ..Default::default()
            })
            .contains("review_brief")
        );
        assert!(
            refused(FactoryEdit {
                measurers: Some(vec!["not a login".into()]),
                ..Default::default()
            })
            .contains("measurers")
        );
        assert!(
            refused(FactoryEdit {
                testing_label: Some("a,b".into()),
                ..Default::default()
            })
            .contains("label")
        );
    }
}
