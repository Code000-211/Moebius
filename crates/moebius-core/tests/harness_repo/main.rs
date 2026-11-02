//! The harness configuration repository: agents and rules come from a git repository, a run uses them, an edit through
//! the API is a commit to it, and a commit someone else pushes arrives at the next refresh.
//!
//! One module per part: `reading` the agents, rules, skills and MCP servers read from it and refreshed, `edits` the edits
//! through the API, `skill_files` a skill's other files read and edited, `proposals` the chats' branches offered and merged, `packages` Pi packages installed, updated and removed (with `registry`, a fake npm registry); `fixtures` holds what they share.

mod edits;
// Unix only: the runs are on the fake Pi (testdata/fake-pi), a Python script started by its `#!` line.
#[cfg(unix)]
mod engineering;
mod fixtures;
// Unix only for the tests that start a run or an agent on the fake Pi; the rest run everywhere.
mod packages;
// Unix only: the chats run on the fake Pi (testdata/fake-pi), a Python script started by its `#!` line.
#[cfg(unix)]
mod proposals;
mod reading;
mod registry;
mod skill_files;
