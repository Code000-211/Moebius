//! Automations: recurring work that runs on its own. An automation is a workflow whose trigger is `schedule` and whose
//! one node is a `chat` node: its name, cron schedule, agent, prompt, place and on/off flag are the workflow's.
//!
//! The workflow schedule is what fires an automation ([`App::fire_automation_definition`]): the run it starts reaches
//! the `chat` node, which starts the chat (see `app::workflows::chat`), and the firing names that workflow run. An
//! automation whose row has not been written to a workflow yet, and one on a server with no harness configuration
//! repository, is fired from its row instead, so neither stops on the way to a file. A firing due
//! while the chat of the one before still runs is recorded as skipped. A run never waits for a person: once an
//! automation's chat has answered and waits for a message, the schedule ends it (`firing::on_schedule`), so that what it
//! changed is offered as a change and lands in the inbox, as when a person ends a chat. Each firing is recorded, with
//! the run and its chat, and published as `automation_fired`.
//!
//! The automations API and CLI are a shortcut over these workflows: they read and save the file behind each one, and
//! rows in the store only keep an automation's id and its firings.

pub mod cron;

mod definition;
mod firing;
mod migration;
mod view;

pub use definition::is_automation;
pub use firing::on_schedule;
pub use view::AutomationView;

/// The origin of a chat an automation started (see `Run::origin`).
pub const AUTOMATION_ORIGIN: &str = "automation";
