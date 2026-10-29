//! The board engine (ADR 0004, ADR 0005): each column's behaviour is a Luau script, run in a sandbox for one card at a
//! time. A script defines `on_card(card)`, reads the card from the `card` table and acts through the `moebius` table.
//!
//! - `engine`: the engine itself, which runs the script of a card's column and applies what it did.
//! - `api`: the `moebius` table, the functions a script acts through.
//! - `card`: the `card` table, what a script knows of its card, and the verdict of its review.
//! - `review`: the review along two axes, Standards and Spec, read from the summaries of its runs.
//! - `sandbox`: the Luau sandbox every script runs in, with its memory and time limits.
//! - `limits`: the run, round and token limits scripts see.
//! - `workflow`: the columns and the default workflow's scripts built into the server, and a repository's overrides.
//! - `runs`: what the engine needs of a harness to start runs and chats (`Runs`), which `pibolt` implements.
//! - `dry_run`: `check_script`, which runs a script once against a made-up card, for `moebius check-script`.
//! - `scripts`: editing a column's script, saved as a commit in the repository.

mod api;
mod card;
mod dry_run;
mod engine;
mod limits;
mod review;
mod runs;
mod sandbox;
pub mod scripts;
#[cfg(test)]
mod testing;
mod workflow;

pub use card::{Review, latest_review};
pub use dry_run::check_script;
pub use engine::{BoardError, Engine, Step};
pub use limits::Limits;
pub use runs::{AgentInfo, Runs, Tui};
pub use workflow::{Column, Workflow, built_in_script, column_named, repository_script};
