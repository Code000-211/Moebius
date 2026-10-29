//! The lens (issue 114): fast lint and type feedback for an agent after its own edits, so that it sees its mistakes at
//! once and not one review round later. It is a Pi extension (`bridge/moebius-lens.ts`) loaded next to the bridge,
//! and this folder is its server side.
//!
//! The extension runs the checks, in tiers. After each edit of a file, within a time budget, it runs a syntax check
//! and the repository's fast linter on that file alone, and appends only the problems that are new, against the file's
//! diagnostics before the edit, to the tool's result. When the agent is done, before the run settles, it runs the slow
//! type check once and tells the agent in a message. Nothing starts a language server.
//!
//! What the extension runs comes from `plan`: the repository's `.moebius/lint.toml`, or the common tools detected from
//! the repository's files, handed to the extension as JSON in `MOEBIUS_LENS`. The decisions are the server's, through
//! `decide`: `lint_report` for which new findings are shown and `deep_check` for whether the slow check runs
//! (`report`, whose endpoints the extension calls with the run's token); `decider` is the rule that answers both until
//! the decision model takes them over. `report` also stores the time each check took as `lens_check` run events.

mod decider;
mod plan;
mod report;

use std::path::Path;

pub use decider::{FileKind, Lens, NAME, file_kind};
pub use plan::{LINT_FILE, Plan};
pub use report::{close_run, routes};

/// The extension, written into each run's directory.
pub const EXTENSION: &str = include_str!("../../../bridge/moebius-lens.ts");

/// The plan for the worktree at `worktree`, as the extension reads it from `MOEBIUS_LENS`.
pub fn plan_json(worktree: &Path) -> String {
    serde_json::to_string(&Plan::for_worktree(worktree)).expect("a plan is JSON")
}
