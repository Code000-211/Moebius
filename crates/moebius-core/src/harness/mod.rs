//! The coding-agent harnesses and what they are given: Pi-Bolt runs and chats (`pibolt`), Claude Code runs and chats
//! (`claude`), the harness of each agent picked by its `harness` (`pick`), shells in a worktree and on the server
//! (`terminal`), the harness configuration repository with the agents, skills, MCP servers and rules (`repo`, `skill`,
//! `mcp`), the MCP gateway through which a run reaches its MCP servers (`gateway`), the chats that end in issues
//! (`grilling`), and the outputs of tools kept small where they are made (`output`), and the machines that run a card's harness over SSH (`remote`).

pub mod claude;
pub mod gateway;
pub mod grilling;
pub mod mcp;
pub mod output;
pub mod pibolt;
pub mod pick;
pub mod remote;
pub mod repo;
pub mod skill;
pub mod terminal;
