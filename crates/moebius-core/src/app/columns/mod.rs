//! The columns of a board: the six built-in ones of the default workflow (spec 6) and the custom ones a project adds
//! in `projects/<project name>/kanban.toml` of the harness configuration repository (ADR 0030).
//!
//! - this file: the names of the built-in columns and what is asked of a column's *class*, the built-in column a
//!   column counts as. Built-in logic (is the card done, in review, ready to merge) asks for the class, never for the
//!   literal name, so that a card in a custom column counts as its class does everywhere.
//! - `layout`: reading and checking a `kanban.toml` ([`ProjectColumns`]).
//! - `project`: what the app does with it: the columns of a project, kept for as long as the file is the same, and the
//!   table that lets SQL ask for a column's class.

mod layout;
mod project;

pub use layout::{ColumnInfo, KANBAN_FILE, ProblemKind, ProjectColumns, parse, safe_script_name};
pub(crate) use project::Cached;

pub const BACKLOG: &str = "Backlog";
pub const NOT_STARTED: &str = "Not started";
pub const IN_PROGRESS: &str = "In progress";
pub const IN_REVIEW: &str = "In review";
pub const READY_TO_MERGE: &str = "Ready to merge";
pub const DONE: &str = "Done";

/// The built-in columns, left to right. Also the six classes a column can count as.
pub const BUILT_IN: [&str; 6] = [
    BACKLOG,
    NOT_STARTED,
    IN_PROGRESS,
    IN_REVIEW,
    READY_TO_MERGE,
    DONE,
];

/// The built-in column called `name` (in any case; "Ready" is the alias of "Not started"), as it is written here.
pub fn built_in(name: &str) -> Option<&'static str> {
    let name = if name.eq_ignore_ascii_case("Ready") {
        NOT_STARTED
    } else {
        name
    };
    BUILT_IN
        .iter()
        .copied()
        .find(|b| b.eq_ignore_ascii_case(name))
}

/// A column as the desktop API answers it, in the order of the board.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ColumnView {
    pub name: String,
    /// The built-in column it counts as (its own name for a built-in column).
    pub counts_as: String,
    /// Whether the project defined it in its `kanban.toml`.
    pub custom: bool,
    /// The file of its script in the project's settings.
    pub script: String,
    /// Whether a card that enters it is queued for the test lane (`lane = true`).
    #[serde(default)]
    pub lane: bool,
}

impl ProjectColumns {
    /// The board's columns as the API answers them.
    pub fn views(&self) -> Vec<ColumnView> {
        self.columns
            .iter()
            .map(|c| ColumnView {
                name: c.name.clone(),
                counts_as: c.counts_as.to_string(),
                custom: c.custom,
                script: c.script.clone(),
                lane: c.lane,
            })
            .collect()
    }
}
