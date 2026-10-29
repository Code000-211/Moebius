//! The harness configuration repository: agents and rules come from a git repository, a run uses them, an edit through
//! the API is a commit to it, and a commit someone else pushes arrives at the next refresh.//!
//! One module per part: `reading` the agents, rules, skills and MCP servers read from it and refreshed, `edits` the edits
//! through the API, `skill_files` a skill's other files read and edited, `proposals` the chats' branches offered and merged; `fixtures` holds what they share.

mod edits;
mod fixtures;
mod proposals;
mod reading;
mod skill_files;
