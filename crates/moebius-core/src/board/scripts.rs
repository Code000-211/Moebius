//! Editing a column's script from the board. A project overrides a column's built-in script with `<file>.luau` in its
//! settings, `projects/<project name>/` of the harness configuration repository (see [`crate::board`]), so two
//! projects that share a repository can work differently; this module reads that file, and changes it as the other
//! edits of that repository are made: a commit on its default branch, pushed to `origin`, undone when the push fails.
//! A script is checked the way `moebius check-script` checks it before it is committed. Every change is recorded as a decision of the person or agent who made it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::app::columns::{ColumnView, KANBAN_FILE};
use crate::app::{App, AppError, OWNER_AUTHOR, Result};
use crate::board::{self, Column};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::Project;

/// The kind of the decision recorded for a change to a script.
const KIND: &str = "edit_script";

/// A column's script as `GET /v1/projects/{id}/scripts/{column}` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptView {
    pub column: String,
    /// Where the project keeps its own script for the column, from the root of the harness configuration repository.
    pub path: String,
    /// `project` when the project has a script of its own, `built-in` when the column runs the one in the server.
    pub source: String,
    pub text: String,
}

/// What `PUT /v1/projects/{id}/scripts/{column}` takes.
#[derive(Debug, Clone, Deserialize)]
pub struct NewScript {
    pub text: String,
    /// Why the script changes, kept with the decision.
    #[serde(default)]
    pub reason: String,
}

/// The column of the project's board called `column`, with the file of its script.
fn column_of(app: &App, project: &Project, column: &str) -> Result<Column> {
    app.project_columns(project)
        .column(column)
        .map(|c| Column {
            name: c.name.clone(),
            script: c.script.clone(),
        })
        .ok_or_else(|| AppError::NotFound(format!("column {column}")))
}

fn path_of(project: &Project, column: &Column) -> String {
    format!("projects/{}/{}", project.name, column.script)
}

/// A script ends with a newline, as files do.
fn with_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn view(app: &App, project: &Project, column: &Column) -> Result<ScriptView> {
    let (source, text) = match app.project_file(project, &column.script) {
        Some(text) => ("project", with_newline(text)),
        None => {
            // A custom column whose file is not there yet runs, and starts from, the script of its class.
            let text = board::built_in_script(&column.script)
                .or_else(|| {
                    let class = app.column_class(project.id, &column.name);
                    let fallback = board::column_named(class)?;
                    board::built_in_script(&fallback.script)
                })
                .ok_or_else(|| AppError::NotFound(format!("script {}", column.script)))?;
            ("built-in", text.to_string())
        }
    };
    Ok(ScriptView {
        column: column.name.clone(),
        path: path_of(project, column),
        source: source.into(),
        text,
    })
}

/// The scripts of every column of project `id`'s board, left to right: the project's own, or the built-in ones.
pub fn scripts(app: &App, id: i64) -> Result<Vec<ScriptView>> {
    let project = app.project(id)?;
    let board = app.project_columns(&project);
    board
        .columns
        .iter()
        .map(|c| Column {
            name: c.name.clone(),
            script: c.script.clone(),
        })
        .map(|column| view(app, &project, &column))
        .collect()
}

/// The script of `column` in project `id`: the project's own, or the built-in one.
pub fn script(app: &App, id: i64, column: &str) -> Result<ScriptView> {
    let project = app.project(id)?;
    view(app, &project, &column_of(app, &project, column)?)
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
    let project = app.project(id)?;
    let column = column_of(app, &project, column)?;
    let text = with_newline(new.text.clone());
    board::check_script_in(
        &app.project_columns(&project),
        &column.script,
        &text,
        &column.name,
        "A card to check the script with",
        OWNER_AUTHOR,
    )
    .map_err(AppError::Invalid)?;
    let message = format!(
        "Change the {} column's script of {}",
        column.name, project.name
    );
    let context = BTreeMap::from([("text".to_string(), text.clone())]);
    edit(
        app,
        by,
        &project,
        &column,
        Some(&text),
        (&message, &new.reason),
        context,
    )?;
    view(app, &project, &column)
}

/// Removes the project's script for `column`, so the column runs the built-in one again.
pub fn delete_script(app: &App, by: &Actor, id: i64, column: &str) -> Result<ScriptView> {
    let project = app.project(id)?;
    let column = column_of(app, &project, column)?;
    if app.project_file(&project, &column.script).is_none() {
        return Err(AppError::NotFound(format!(
            "a script of its own for the {} column in {}",
            column.name, project.name
        )));
    }
    let message = format!(
        "Use the built-in script for the {} column of {}",
        column.name, project.name
    );
    edit(
        app,
        by,
        &project,
        &column,
        None,
        (&message, ""),
        BTreeMap::new(),
    )?;
    view(app, &project, &column)
}

/// Commits the change to the column's file (`text`, or its removal) in the harness configuration repository, recorded
/// as an `edit_script` decision and announced as `script_changed` once it is through.
fn edit(
    app: &App,
    by: &Actor,
    project: &Project,
    column: &Column,
    text: Option<&str>,
    (message, reason): (&str, &str),
    mut context: BTreeMap<String, String>,
) -> Result<()> {
    context.insert("column".into(), column.name.clone());
    if !reason.trim().is_empty() {
        context.insert("reason".into(), reason.trim().into());
    }
    // The name is read again with the clone locked, so that a rename meanwhile does not leave the file behind.
    let named = || Ok(app.project(project.id)?.name);
    let commit = app.project_harness_repo()?.save_project_file(
        app,
        by,
        (&named, &column.script),
        text,
        (KIND, message),
        context,
    )?;
    if commit.is_some() {
        app.bus.publish(Event::ScriptChanged {
            project: project.id,
            column: column.name.clone(),
            path: path_of(project, column),
        });
    }
    Ok(())
}

/// What `GET /v1/projects/{id}/columns` answers: the project's `kanban.toml`, which defines its custom columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnsView {
    /// Where the project keeps the file, from the root of the harness configuration repository.
    pub path: String,
    /// `project` when the project has the file, `default` when its board is the six built-in columns.
    pub source: String,
    /// The file's text; an example, all in comments, when the project has none.
    pub text: String,
    /// The board the file makes, left to right.
    pub columns: Vec<ColumnView>,
    /// What is wrong with the file or the board, if anything.
    pub problem: Option<String>,
}

/// What a project without a `kanban.toml` starts from when it adds one.
const EXAMPLE_COLUMNS: &str = "# A column of your own, among the built-in ones. Its script is testing.luau of this folder;\n# without `script` it runs the script of the column it counts as.\n#\n# [[column]]\n# name = \"Ready for testing\"\n# script = \"testing.luau\"\n# after = \"In review\"\n# counts_as = \"In review\"\n";

fn columns_view(app: &App, project: &Project) -> ColumnsView {
    let board = app.project_columns(project);
    let (source, text) = match app.project_file(project, KANBAN_FILE) {
        Some(text) => ("project", text),
        None => ("default", EXAMPLE_COLUMNS.to_string()),
    };
    ColumnsView {
        path: format!("projects/{}/{KANBAN_FILE}", project.name),
        source: source.into(),
        text,
        columns: board.views(),
        problem: board.problem.as_ref().map(|(_, problem)| problem.clone()),
    }
}

/// The `kanban.toml` of project `id` and the board it makes.
pub fn columns(app: &App, id: i64) -> Result<ColumnsView> {
    let project = app.project(id)?;
    Ok(columns_view(app, &project))
}

/// Replaces the project's `kanban.toml` with `new.text`: refused with `Invalid` when it is not a valid board and with
/// `Conflict` when it removes a column that cards are in (the message names them), committed and pushed otherwise.
pub fn save_columns(app: &App, by: &Actor, id: i64, new: &NewScript) -> Result<ColumnsView> {
    let project = app.project(id)?;
    let text = with_newline(new.text.clone());
    app.check_kanban_save(&project, Some(&text))?;
    let message = format!("Change the columns of {}", project.name);
    let context = BTreeMap::from([("text".to_string(), text.clone())]);
    edit_columns(
        app,
        by,
        &project,
        Some(&text),
        (&message, &new.reason),
        context,
    )?;
    Ok(columns_view(app, &project))
}

/// Removes the project's `kanban.toml`, so its board is the six built-in columns again: refused with `Conflict` while
/// cards are in a custom column.
pub fn delete_columns(app: &App, by: &Actor, id: i64) -> Result<ColumnsView> {
    let project = app.project(id)?;
    if app.project_file(&project, KANBAN_FILE).is_none() {
        return Err(AppError::NotFound(format!(
            "a {KANBAN_FILE} for {}",
            project.name
        )));
    }
    app.check_kanban_save(&project, None)?;
    let message = format!("Use the built-in columns for {}", project.name);
    edit_columns(app, by, &project, None, (&message, ""), BTreeMap::new())?;
    Ok(columns_view(app, &project))
}

/// Commits the change to the project's `kanban.toml` and announces it as `script_changed` for the file.
fn edit_columns(
    app: &App,
    by: &Actor,
    project: &Project,
    text: Option<&str>,
    (message, reason): (&str, &str),
    mut context: BTreeMap<String, String>,
) -> Result<()> {
    if !reason.trim().is_empty() {
        context.insert("reason".into(), reason.trim().into());
    }
    let named = || Ok(app.project(project.id)?.name);
    let commit = app.project_harness_repo()?.save_project_file(
        app,
        by,
        (&named, KANBAN_FILE),
        text,
        (KIND, message),
        context,
    )?;
    if commit.is_some() {
        app.bus.publish(Event::ScriptChanged {
            project: project.id,
            column: KANBAN_FILE.into(),
            path: format!("projects/{}/{KANBAN_FILE}", project.name),
        });
    }
    Ok(())
}
