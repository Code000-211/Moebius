//! The client against a real server (`moebius-core` as a dev-dependency): it reads the snapshot with the server's
//! token, receives an event on the stream, and is told when the token is wrong or the server cannot be reached.
//!
//! One module per area: `client` (the connection itself), `live` (runs, shells and the Fleet), `inbox`, `chat`,
//! `harness` (the harness configuration repository), `import` (skills imported from a chosen zip), `improve` (usage, Retro, research and the decision model),
//! `setup` (models, plugins and settings) and `project` (repositories, scripts and files). `fixtures` holds the server
//! on a free port, the repositories to add and the waiting on the stream they share.
#![cfg(unix)]

mod chat;
mod client;
mod fixtures;
mod harness;
mod import;
mod improve;
mod inbox;
mod live;
mod project;
mod setup;
