//! Draws the shell headlessly with a fixed snapshot (no server) and checks what is on the screen. The pictures are
//! kept in `tests/snapshots/` to be compared with `docs/design/board.png`; a later run must draw the same pixels.
//!
//! One module per screen or area: `sidebar` (the sidebar, connect, adding a repository and search), `board` (the
//! board, a new issue and a column's script), `live`, `inbox`, `chat` and `new_chat`, `agents`, `skills` (skills, the
//! import panel and MCP servers), `rules`, `setup` (Models, Plugins and Settings), `terminal`, `ports` (the forwarded ports and the offer to open one), the Improve pages
//! (`usage`, `improve` for the Retro and Experiments, `decision_model`), `fleet`, `automations`, `float_chat`,
//! `connect_ssh` (the way over SSH and the header of the servers), `project` (the tabs and the Files sidebar), `repositories` (the Projects overview and removing one), `edits` (editors in place) and `navigation` (going back). `fixtures` holds the
//! snapshots, the simulator, the drawing and the clicks they share.

mod agents;
mod automations;
mod board;
mod chat;
mod connect_ssh;
mod decision_model;
mod edits;
mod fixtures;
mod fleet;
mod float_chat;
mod improve;
mod inbox;
mod live;
mod navigation;
mod new_chat;
mod ports;
mod project;
mod repositories;
mod rules;
mod setup;
mod sidebar;
mod skills;
mod terminal;
mod usage;
