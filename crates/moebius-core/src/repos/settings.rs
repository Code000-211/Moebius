//! A project's settings for the desktop API: the files of `repos/<repository name>/` in the harness configuration
//! repository other than the column scripts (those are [`crate::board::scripts`]): `rules.md`, `review.md`,
//! `checks`, `test-first` and `localize`. Reading lists them with the text each has; writing or removing
//! one is a commit there, as an edit of an agent is, and is refused when the text is not fit to be saved. Every
//! change is recorded as a decision of the person or agent who made it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::app::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::harness::repo::PROJECT_FILES;
use crate::store::Repository;

/// The kind of the decision recorded for a change to a project's rules.
const RULES_KIND: &str = "edit_repository_rules";
/// The kind of the decision recorded for a change to any other of the files.
const FILE_KIND: &str = "edit_project_file";

/// One file of a project's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingFile {
    pub file: String,
    /// Whether the project has the file; an empty `test-first` that exists turns test first on.
    pub exists: bool,
    pub text: String,
    /// The short id of the commit a `PUT` or `DELETE` made; left out by `GET` and when nothing changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// A project's settings as `GET /v1/repositories/{id}/settings` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Where the settings are, from the root of the harness configuration repository.
    pub path: String,
    /// Whether they can be changed: the server has a harness configuration repository.
    pub editable: bool,
    pub files: Vec<SettingFile>,
    /// The column scripts the project has of its own, such as `ready.luau`.
    pub scripts: Vec<String>,
    /// The files of a `.moebius/` folder the repository's default branch still has, which are not read.
    pub leftover: Vec<String>,
}

/// What `PUT /v1/repositories/{id}/settings/{file}` takes.
#[derive(Debug, Clone, Deserialize)]
pub struct NewSetting {
    pub text: String,
    /// Why the file changes, kept with the decision.
    #[serde(default)]
    pub reason: String,
}

fn view(app: &App, repository: &Repository, file: &str) -> SettingFile {
    let text = app.project_file(repository, file);
    SettingFile {
        file: file.into(),
        exists: text.is_some(),
        text: text.unwrap_or_default(),
        commit: None,
    }
}

/// The settings of repository `id`'s project.
pub fn settings(app: &App, id: i64) -> Result<Settings> {
    let repository = app.repository(id)?;
    let scripts = crate::board::Kanban::default()
        .columns
        .into_iter()
        .map(|column| column.script)
        .filter(|script| app.project_file(&repository, script).is_some())
        .collect();
    Ok(Settings {
        path: format!("repos/{}", repository.name),
        editable: app.harness_repo().is_ok(),
        files: PROJECT_FILES
            .iter()
            .map(|file| view(app, &repository, file))
            .collect(),
        scripts,
        leftover: app.leftover_settings(&repository),
    })
}

/// `file` as a name of one of [`PROJECT_FILES`], else `Invalid`.
fn known(file: &str) -> Result<&'static str> {
    PROJECT_FILES
        .iter()
        .find(|known| **known == file)
        .copied()
        .ok_or_else(|| {
            AppError::Invalid(format!(
                "{file} is not a file of a project's settings; they are {} and the column scripts (see the scripts \
                 routes)",
                PROJECT_FILES.join(", ")
            ))
        })
}

/// Why `text` is not fit to be saved as the project's `file`.
fn check(file: &str, text: &str) -> Result<()> {
    let checked = match file {
        "rules.md" => crate::decide::rules::check(text),
        _ => Ok(()),
    };
    checked.map_err(|why| AppError::Invalid(format!("{file}: {why}")))
}

/// Replaces the project's `file` of repository `id` with `new.text`, committed in the harness configuration
/// repository: refused with `Invalid` when the text is not fit (rules) or the server has no such
/// repository. Text that changes nothing makes no commit.
pub fn save(app: &App, by: &Actor, id: i64, file: &str, new: &NewSetting) -> Result<SettingFile> {
    let repository = app.repository(id)?;
    let file = known(file)?;
    check(file, &new.text)?;
    let mut text = new.text.clone();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let message = format!("Update {file} of {}", repository.name);
    let context = BTreeMap::from([("text".to_string(), text.clone())]);
    write(
        app,
        by,
        &repository,
        file,
        Some(&text),
        &message,
        &new.reason,
        context,
    )
}

/// Removes the project's `file` of repository `id`: `NotFound` when it has none.
pub fn remove(app: &App, by: &Actor, id: i64, file: &str, reason: &str) -> Result<SettingFile> {
    let repository = app.repository(id)?;
    let file = known(file)?;
    let message = format!("Remove {file} of {}", repository.name);
    write(
        app,
        by,
        &repository,
        file,
        None,
        &message,
        reason,
        BTreeMap::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn write(
    app: &App,
    by: &Actor,
    repository: &Repository,
    file: &str,
    text: Option<&str>,
    message: &str,
    reason: &str,
    mut context: BTreeMap<String, String>,
) -> Result<SettingFile> {
    if !reason.trim().is_empty() {
        context.insert("reason".into(), reason.trim().into());
    }
    let kind = if file == "rules.md" {
        RULES_KIND
    } else {
        FILE_KIND
    };
    let commit = app.project_harness_repo()?.save_project_file(
        app,
        by,
        (&repository.name, file),
        text,
        (kind, message),
        context,
    )?;
    if commit.is_some() {
        app.bus.publish(Event::ProjectSettingsChanged {
            repository: repository.id,
            file: file.into(),
        });
    }
    Ok(SettingFile {
        commit,
        ..view(app, repository, file)
    })
}
