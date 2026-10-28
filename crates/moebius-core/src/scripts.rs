//! Editing a column's script from the board. A repository overrides a column's built-in script with
//! `.moebius/<file>.luau` on its default branch (see [`crate::board`]); this module reads that file, and changes it
//! with a commit on the default branch of Moebius's clone, pushed to `origin` for a repository on a forge. A script is
//! checked the way `moebius check-script` checks it before it is committed, and a push that fails undoes the commit,
//! so the clone never drifts from `origin`. Every change is recorded as a decision of the person or agent who made it.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{App, AppError, PERSON, Result};
use crate::blobs::redact;
use crate::board::{self, Column};
use crate::bus::AppEvent;
use crate::decide::{Actor, Question};
use crate::repos::{GitError, git};
use crate::store::Repository;

/// The kind of the decision recorded for a change to a script.
const KIND: &str = "edit_script";

/// Edits take turns, so that two commits never meet in one clone.
static EDITING: Mutex<()> = Mutex::new(());

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

/// Makes the change `write` does to the column's file a commit on the default branch of the clone, pushes it for a
/// repository on a forge, and records the decision and publishes the event. Whatever fails before the push is
/// through, the default branch is as it was. A change that changes nothing makes no commit and no decision.
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
    let _editing = EDITING.lock().unwrap_or_else(|e| e.into_inner());
    let clone = Path::new(&repository.path);
    let path = path_of(column);
    let on_forge = repository.forge != "none";
    let header = if on_forge {
        let url = app.repos.origin_url(clone).map_err(refused)?;
        app.git_auth(&url)?
    } else {
        None
    };
    if on_forge {
        // Start from the forge's default branch, so the push is a fast-forward.
        app.repos.fetch(clone, header.as_deref()).map_err(refused)?;
        let upstream = repository.base_ref();
        git(clone, &["merge", "--ff-only", "--quiet", &upstream]).map_err(refused)?;
    }
    let before = git(clone, &["rev-parse", "HEAD"])?;
    let undo = |error: AppError| {
        let _ = git(clone, &["reset", "--hard", "--quiet", &before]);
        let _ = git(clone, &["clean", "-fdq", "--", &path]);
        error
    };
    write().map_err(undo)?;
    git(clone, &["add", "-A", "--", &path]).map_err(|e| undo(e.into()))?;
    if git(clone, &["diff", "--cached", "--quiet"]).is_ok() {
        return Ok(());
    }
    git(clone, &["commit", "--quiet", "-m", message]).map_err(|e| undo(e.into()))?;
    if on_forge {
        app.repos
            .push(clone, &repository.default_branch, header.as_deref())
            .map_err(|e| undo(refused(e)))?;
    }
    context.insert("repository".into(), repository.name.clone());
    context.insert("column".into(), column.name.clone());
    context.insert("path".into(), path.clone());
    context.insert("change".into(), message.into());
    app.decisions.record_by(
        by,
        &Question {
            kind: KIND.into(),
            context,
            options: vec![KIND.into(), "leave".into()],
            fail_closed: "leave".into(),
            run_id: None,
            card_id: None,
        },
        KIND,
        reason,
    )?;
    app.bus.publish(AppEvent::new(
        "script_changed",
        json!({ "repository": repository.id, "column": column.name, "path": path }),
    ));
    Ok(())
}

/// A git failure that the forge or the clone's state caused, as the 409 that says why, with keys redacted.
fn refused(error: GitError) -> AppError {
    AppError::Conflict(redact(&error.to_string()))
}
