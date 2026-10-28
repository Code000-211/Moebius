//! Editing a column's script from the board. A repository overrides a column's built-in script with
//! `.moebius/<file>.luau` on its default branch (see [`crate::board`]); this module reads that file, and changes it
//! the way [`crate::files`] changes any file: a commit on the default branch of Moebius's clone, pushed to `origin` for
//! a repository on a forge, undone when the push fails. A script is checked the way `moebius check-script` checks it
//! before it is committed. Every change is recorded as a decision of the person or agent who made it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{App, AppError, PERSON, Result};
use crate::board::{self, Column};
use crate::bus::AppEvent;
use crate::decide::Actor;
use crate::files::{self, Change};
use crate::store::Repository;

/// The kind of the decision recorded for a change to a script.
const KIND: &str = "edit_script";

/// A column's script as `GET /v1/repositories/{id}/scripts/{column}` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptView {
    pub column: String,
    /// Where the repository keeps its own script for the column.
    pub path: String,
    /// `repository` when the repository has its own script, `built-in` when the column runs the one in the server.
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

fn path_of(column: &Column) -> String {
    format!(".moebius/{}", column.script)
}

/// A script ends with a newline, as files do; the clone's `git show` has it trimmed.
fn with_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn view(repository: &Repository, column: &Column) -> Result<ScriptView> {
    let (source, text) = match board::repository_script(repository, &column.script) {
        Some(text) => ("repository", with_newline(text)),
        None => {
            let text = board::built_in_script(&column.script)
                .ok_or_else(|| AppError::NotFound(format!("script {}", column.script)))?;
            ("built-in", text.to_string())
        }
    };
    Ok(ScriptView {
        column: column.name.clone(),
        path: path_of(column),
        source: source.into(),
        text,
    })
}

/// The script of `column` in repository `id`: its own on the default branch, or the built-in one.
pub fn script(app: &App, id: i64, column: &str) -> Result<ScriptView> {
    let repository = app.repository(id)?;
    view(&repository, &column_of(column)?)
}

/// Replaces the repository's script for `column` with `new.text`: refused with `Invalid` when the check fails,
/// committed and, on a forge, pushed otherwise. Text that changes nothing makes no commit.
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
    let path = path_of(&column);
    let message = format!("Change the {} column's script", column.name);
    let clone = Path::new(&repository.path);
    let write = || {
        std::fs::create_dir_all(clone.join(".moebius"))?;
        Ok(std::fs::write(clone.join(&path), &text)?)
    };
    let context = BTreeMap::from([("text".to_string(), text.clone())]);
    edit(
        app,
        by,
        &repository,
        &column,
        &message,
        &new.reason,
        context,
        write,
    )?;
    view(&repository, &column)
}

/// Removes the repository's script for `column`, so the column runs the built-in one again.
pub fn delete_script(app: &App, by: &Actor, id: i64, column: &str) -> Result<ScriptView> {
    let repository = app.repository(id)?;
    let column = column_of(column)?;
    if board::repository_script(&repository, &column.script).is_none() {
        return Err(AppError::NotFound(format!(
            "a script of its own for the {} column in {}",
            column.name, repository.name
        )));
    }
    let path = path_of(&column);
    let message = format!("Use the built-in script for the {} column", column.name);
    let clone = Path::new(&repository.path);
    let write = || Ok(std::fs::remove_file(clone.join(&path))?);
    edit(
        app,
        by,
        &repository,
        &column,
        &message,
        "",
        BTreeMap::new(),
        write,
    )?;
    view(&repository, &column)
}

/// Commits the change `write` does to the column's file through [`files::commit`], recorded as an `edit_script`
/// decision and announced as `script_changed`.
#[allow(clippy::too_many_arguments)]
fn edit(
    app: &App,
    by: &Actor,
    repository: &Repository,
    column: &Column,
    message: &str,
    reason: &str,
    mut context: BTreeMap<String, String>,
    write: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let path = path_of(column);
    context.insert("column".into(), column.name.clone());
    let change = Change {
        kind: KIND,
        path: &path,
        message,
        reason,
        context,
        event: AppEvent::new(
            "script_changed",
            json!({ "repository": repository.id, "column": column.name, "path": path }),
        ),
    };
    files::commit(app, by, repository, &change, write)
}
