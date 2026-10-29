//! What the event stream tells the app: reading again what an event changed, and the steps of runs, which go to the
//! screens that show them.

use iced::Task;

use crate::client::{ClientError, RunEvent, StreamEvent};
use crate::screens::live;
use crate::screens::project::Project;

use super::message::Message;
use super::{App, Connection, Opened};

impl App {
    pub(super) fn on_stream(&mut self, event: StreamEvent) -> Task<Message> {
        match event {
            StreamEvent::Connected => {
                self.connection = Connection::Online;
                // Events are not replayed: the run shown reads what it missed.
                let tiles = match &self.settings {
                    Some(settings) => self.fleet.reload(settings),
                    None => Task::none(),
                };
                let (missed, chats) = match (&self.opened, &self.settings) {
                    (Some(Opened::Run(live)), Some(settings)) => (
                        live::load(settings.clone(), live.card, live.run_id(), live.last_seq()),
                        self.chats.reload(settings),
                    ),
                    (_, Some(settings)) => (Task::none(), self.chats.reload(settings)),
                    _ => (Task::none(), Task::none()),
                };
                let usage = match &self.settings {
                    Some(settings) => self.usage.reload(settings),
                    None => Task::none(),
                };
                // A server that restarted tells what it runs with now.
                let setup = match &self.settings {
                    Some(settings) if self.setup.restarting() => self.setup.reload(settings),
                    _ => Task::none(),
                };
                let start = match &self.settings {
                    Some(settings) if self.start.incomplete() => self.start.reload(settings),
                    _ => Task::none(),
                };
                // A server that restarted on a new harness configuration repository has skills, MCP servers and its
                // agents there now.
                let harness = match &self.settings {
                    Some(settings) if self.harness.restarting() => {
                        self.harness.restarted();
                        Task::batch([
                            self.skills.reload(settings),
                            self.mcp_servers.reload(settings),
                            self.agents.reload(settings),
                        ])
                    }
                    _ => Task::none(),
                };
                let floating = match &self.settings {
                    Some(settings) => self.float.reload(settings),
                    None => Task::none(),
                };
                // Ports that opened or closed while the stream was away.
                let ports = match &self.settings {
                    Some(settings) => self.ports.reload(settings),
                    None => Task::none(),
                };
                Task::batch([
                    self.reload(),
                    missed,
                    chats,
                    tiles,
                    usage,
                    setup,
                    start,
                    harness,
                    floating,
                    ports,
                ])
            }
            StreamEvent::Disconnected(_) => {
                self.connection = Connection::Offline;
                Task::none()
            }
            StreamEvent::Unauthorized => {
                self.show_connect_error(&ClientError::Unauthorized);
                Task::none()
            }
            StreamEvent::Event(event)
                if matches!(event.kind.as_str(), "port_opened" | "port_closed") =>
            {
                self.on_port_event(&event);
                Task::none()
            }
            StreamEvent::Event(event) if event.kind == "run_event" => {
                self.on_run_event(&event.data)
            }
            StreamEvent::Event(event) => {
                let mut tasks = vec![self.reload()];
                if matches!(event.kind.as_str(), "run_started" | "run_ended")
                    && let Some(settings) = &self.settings
                {
                    tasks.push(self.chats.reload(settings));
                }
                if matches!(
                    event.kind.as_str(),
                    "retro_done" | "trial_done" | "proposal_status"
                ) && let Some(settings) = &self.settings
                {
                    // The Retro on show, and the proposals the Agents screen offers, have changed.
                    tasks.push(self.retros.reload(settings));
                    tasks.push(self.agents.reload(settings));
                }
                if matches!(event.kind.as_str(), "takeover_proposed" | "takeover_status")
                    && let Some(settings) = &self.settings
                {
                    tasks.push(self.decision_model.reload(settings));
                }
                if matches!(event.kind.as_str(), "research_status" | "experiment_done")
                    && let Some(settings) = &self.settings
                {
                    tasks.push(self.experiments.reload(settings));
                }
                if matches!(event.kind.as_str(), "automation_fired" | "run_ended")
                    && self.page() == super::message::Page::Automations
                    && let Some(settings) = &self.settings
                {
                    // A firing, or the end of a chat one started, shows on the page.
                    tasks.push(self.automations.refresh(settings));
                }
                if event.kind == "replayed"
                    && let Some(settings) = &self.settings
                {
                    // The refusal on show has an outcome now.
                    tasks.push(self.chats.reload_judgements(settings));
                    if let Some(Opened::Run(live)) = &self.opened
                        && event.data["run"] == live.run_id()
                    {
                        tasks.push(live.reload_judgements(settings.clone()));
                    }
                }
                if event.kind == "file_changed"
                    && let (Some(settings), Some(repository), Some(path)) = (
                        &self.settings,
                        event.data["repository"].as_i64(),
                        event.data["path"].as_str(),
                    )
                    && let Some(project) = self.projects.get(&repository)
                    && project.follows(path)
                {
                    // A file open without changes of its own shows what was saved elsewhere, by the agent say.
                    tasks.push(Project::read(repository, path, settings));
                }
                if event.kind == "run_ended" {
                    // A run that ended has used tokens, which the meter of the sidebar shows.
                    if let Some(settings) = &self.settings {
                        tasks.push(self.usage.reload(settings));
                    }
                    let status = event.data["status"].as_str().unwrap_or_default();
                    if let Some(run) = event.data["run"].as_i64() {
                        self.chats.on_ended(run, status);
                        self.float.on_ended(run, status);
                    }
                    if let Some(Opened::Run(live)) = &mut self.opened
                        && event.data["run"] == live.run_id()
                    {
                        live.on_ended(status);
                    }
                }
                Task::batch(tasks)
            }
        }
    }

    /// A step of a run. The steps of a run come in great numbers and change nothing the shell shows, except in the
    /// live view of that run and in the chat screen.
    pub(super) fn on_run_event(&mut self, data: &serde_json::Value) -> Task<Message> {
        let Ok(event) = serde_json::from_value::<RunEvent>(data.clone()) else {
            return Task::none();
        };
        if let Some(run) = data["run"].as_i64() {
            self.fleet.on_event(run, event.clone());
        }
        let chat_checked = data["run"]
            .as_i64()
            .is_some_and(|run| self.chats.on_event(run, &event));
        let float_checked = data["run"]
            .as_i64()
            .is_some_and(|run| self.float.on_event(run, &event));
        // A chat's answer can change the files its worktree changes, which "Offer changes" counts.
        let listed = match (&self.settings, data["run"].as_i64()) {
            (Some(settings), Some(run))
                if event.kind == "agent_settled" && self.chats.lists(run) =>
            {
                self.chats.reload_list(settings)
            }
            _ => Task::none(),
        };
        let live = self.on_live_event(data, event);
        let Some(settings) = &self.settings else {
            return Task::batch([live, listed]);
        };
        let mut tasks = vec![live, listed];
        if chat_checked {
            tasks.push(self.chats.reload_judgements(settings));
        }
        if float_checked {
            tasks.push(self.float.reload_judgements(settings));
        }
        Task::batch(tasks)
    }

    /// A `run_event` of the run on the live view, if that is the one on show. The task reads its judgements again
    /// when a check arrived.
    pub(super) fn on_live_event(
        &mut self,
        data: &serde_json::Value,
        event: RunEvent,
    ) -> Task<Message> {
        let (Some(Opened::Run(live)), Some(settings)) = (&mut self.opened, &self.settings) else {
            return Task::none();
        };
        if data["run"] != live.run_id() {
            return Task::none();
        }
        if live.on_event(event) {
            live.reload_judgements(settings.clone())
        } else {
            Task::none()
        }
    }
}
