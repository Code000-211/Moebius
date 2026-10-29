//! The coding-agent harness and what it is given: Pi-Bolt runs and chats (`pibolt`), shells in a worktree and on the
//! server (`terminal`), the harness configuration repository with the agents, skills, MCP servers and rules
//! (`repo`, `skill`, `mcp`), and the chats that end in issues (`grilling`).

pub mod grilling;
pub mod mcp;
pub mod pibolt;
pub mod repo;
pub mod skill;
pub mod terminal;
