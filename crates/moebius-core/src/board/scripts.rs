//! Editing a column's script from the board. A project overrides a column's built-in script with `<file>.luau` in its
//! settings, `repos/<repository name>/` of the harness configuration repository (see [`crate::board`]); this module
//! reads that file, and changes it as the other edits of that repository are made: a commit on its default branch,
//! pushed to `origin`, undone when the push fails. A script is checked the way `moebius check-script` checks it before
//! it is committed. Every change is recorded as a decision of the person or agent who made it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::app::{App, AppError, PERSON, Result};
use crate::board::{self, Column};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::Repository;

/// The kind of the decision recorded for a change to a script.
const KIND: &str = "edit_script";

/// A column's script as `GET /v1/repositories/{id}/scripts/{column}` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptView {
    pub column: String,
    /// Where the project keeps its own script for the column, from the root of the harness configuration repository.
    pub path: String,
    /// `repository` when the project has a script of its own, `built-in` when the column runs the one in the server.
    pub source: String,
    pub text: String,
}

/// What `PUT /v1/repositories/{id}/scripts/{column}` takes.
#[derive(Debug, Clone, Deserialize)]
pub struct NewScript {
    pub text: String,
    /// Why the script changes, kept with the decision.
    #[serde(default)]
    pub reason: String,
}

fn column_of(column: &str) -> Result<Column> {
    board::column_named(column).ok_or_else(|| AppError::NotFound(format!("column {column}")))
}

fn path_of(repository: &Repository, column: &Column) -> String {
    format!("repos/{}/{}", repository.name, column.script)
}

/// A script ends with a newline, as files do.
fn with_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn view(app: &App, repository: &Repository, column: &Column) -> Result<ScriptView> {
    let (source, text) = match board::repository_script(app, repository, &column.script) {
        Some(text) => ("repository", with_newline(text)),
        None => {
            let text = board::built_in_script(&column.script)
                .ok_or_else(|| AppError::NotFound(format!("script {}", column.script)))?;
            ("built-in", text.to_string())
        }
    };
    Ok(ScriptView {
        column: column.name.clone(),
        path: path_of(repository, column),
        source: source.into(),
        text,
    })
}

/// The script of `column` in repository `id`: the project's own, or the built-in one.
pub fn script(app: &App, id: i64, column: &str) -> Result<ScriptView> {
    let repository = app.repository(id)?;
    view(app, &repository, &column_of(column)?)
}

/// Replaces the project's script for `column` with `new.text`: refused with `Invalid` when the check fails or the
/// server has no harness configuration repository, committed there and pushed otherwise. Text that changes nothing
/// makes no commit.
pub fn save_script(
    app: &App,
    by: &Actor,
    id: i64,
    column: &str,
    new: &NewScript,
) -> Result<ScriptView> {
    let repository = app.repository(id)?;
    let column = column_of(column)?;
    let text = with_newline(new.text.clone());
    board::check_script(
        &column.script,
        &text,
        &column.name,
        "A card to check the script with",
        PERSON,
    )
    .map_err(AppError::Invalid)?;
    let message = format!(
        "Change the {} column's script of {}",
        column.name, repository.name
    );
    let context = BTreeMap::from([("text".to_string(), text.clone())]);
    edit(
        app,
        by,
        &repository,
        &column,
        Some(&text),
        (&message, &new.reason),
        context,
    )?;
    view(app, &repository, &column)
}

/// Removes the project's script for `column`, so the column runs the built-in one again.
pub fn delete_script(app: &App, by: &Actor, id: i64, column: &str) -> Result<ScriptView> {
    let repository = app.repository(id)?;
    let column = column_of(column)?;
    if board::repository_script(app, &repository, &column.script).is_none() {
        return Err(AppError::NotFound(format!(
            "a script of its own for the {} column in {}",
            column.name, repository.name
        )));
    }
    let message = format!(
        "Use the built-in script for the {} column of {}",
        column.name, repository.name
    );
    edit(
        app,
        by,
        &repository,
        &column,
        None,
        (&message, ""),
        BTreeMap::new(),
    )?;
    view(app, &repository, &column)
}

/// Commits the change to the column's file (`text`, or its removal) in the harness configuration repository, recorded
/// as an `edit_script` decision and announced as `script_changed` once it is through.
fn edit(
    app: &App,
    by: &Actor,
    repository: &Repository,
    column: &Column,
    text: Option<&str>,
    (message, reason): (&str, &str),
    mut context: BTreeMap<String, String>,
) -> Result<()> {
    context.insert("column".into(), column.name.clone());
    if !reason.trim().is_empty() {
        context.insert("reason".into(), reason.trim().into());
    }
    let commit = app.project_harness_repo()?.save_project_file(
        app,
        by,
        (&repository.name, &column.script),
        text,
        (KIND, message),
        context,
    )?;
    if commit.is_some() {
        app.bus.publish(Event::ScriptChanged {
            repository: repository.id,
            column: column.name.clone(),
            path: path_of(repository, column),
        });
    }
    Ok(())
}
