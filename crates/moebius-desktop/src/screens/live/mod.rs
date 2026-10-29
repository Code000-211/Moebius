//! The live view of a run (`docs/design/live-view.png`): its conversation (the agent's text, tool calls with the
//! decision of each check, the person's steers, errors), the steer field, stopping, and the panel of what the server
//! knows of the run, and the shell in its worktree. The state follows one run: a load of its events, then the stream's
//! `run_event`s, which `Conversation` puts together without gaps or repeats.
//!
//! `update` has how a message changes the view, `view` the header, the conversation and the steer field, and `panel`
//! the panel of the run.

mod panel;
mod update;
mod view;

use iced::{Subscription, Task, keyboard};

use crate::app::Message as AppMessage;
use crate::client::{self, Client, ClientError, Hint, Judgement, Run, RunEvent, TerminalEvent};
use crate::settings::Settings;
use crate::widgets::conversation::Conversation;
use crate::widgets::terminal::{PaneEvent, Shell};

pub use panel::tokens;
pub use view::harness_name;

/// The status of a run that can be steered and stopped.
const RUNNING: &str = "running";

/// What loading a run brings: the run, its events after the ones already held, and the card's judgements.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub run: Run,
    pub events: Vec<RunEvent>,
    pub judgements: Vec<Judgement>,
    /// The time of the load, Unix seconds.
    pub now: i64,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<Loaded, ClientError>),
    /// The judgements were read again, as a new check arrived.
    Judgements(Result<Vec<Judgement>, ClientError>),
    SteerEdited(String),
    Steer,
    Steered(Result<Run, ClientError>),
    AskStop,
    KeepGoing,
    Stop,
    Stopped(Result<Run, ClientError>),
    /// The clock, for how long ago the run started.
    Tick(i64),
    /// "Open shell in worktree" or "Close shell".
    ToggleShell,
    /// Something the shell's connection did.
    Shell(TerminalEvent),
    /// The shell's pane was laid out at a size, or a click gave it focus or took it away.
    ShellPane(PaneEvent),
    /// A key nothing else took, for the shell if it has focus.
    Key(keyboard::Event),
}

/// The run being watched.
pub struct Live {
    pub card: i64,
    run: Run,
    conversation: Conversation,
    /// The decisions of this run's tool calls, oldest first.
    judgements: Vec<Judgement>,
    /// What the card's latest code run was hinted, and how much of it the change touched.
    hint: Option<Hint>,
    steer: String,
    /// Whether "Stop run" asks to be confirmed.
    confirming: bool,
    /// The last thing that went wrong, as a short line.
    notice: Option<String>,
    now: i64,
    /// The shell in the worktree, while its pane is open.
    shell: Option<Shell>,
    /// How many shells were opened, which tells one's connection from the next one's.
    shells: u32,
}

/// Unix seconds now.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

/// A failure as one short line: the server's own message when it sent one.
pub fn say(error: &ClientError) -> String {
    match error {
        ClientError::Server { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

/// Reads the run, its events after `after` and the card's judgements.
pub fn load(settings: Settings, card: i64, run: i64, after: i64) -> Task<AppMessage> {
    Task::perform(
        async move {
            let client = Client::new(settings);
            Ok(Loaded {
                run: client.run(run).await?,
                events: client.run_events(run, after).await?,
                judgements: client.judgements(card).await?,
                now: now(),
            })
        },
        |loaded| AppMessage::Live(Message::Loaded(loaded)),
    )
}

impl Live {
    /// Watches `run`, the card's latest as the snapshot knows it, until it is loaded.
    pub fn new(card: i64, run: Run) -> Self {
        Live {
            card,
            run,
            conversation: Conversation::default(),
            judgements: Vec::new(),
            hint: None,
            steer: String::new(),
            confirming: false,
            notice: None,
            now: now(),
            shell: None,
            shells: 0,
        }
    }

    pub fn run_id(&self) -> i64 {
        self.run.id
    }

    /// The run watched: its agent and status.
    pub fn run(&self) -> &Run {
        &self.run
    }

    pub fn last_seq(&self) -> i64 {
        self.conversation.last_seq()
    }

    fn running(&self) -> bool {
        self.run.status == RUNNING
    }

    /// Whether the run has a worktree: from its start until it has ended.
    fn has_worktree(&self) -> bool {
        matches!(self.run.status.as_str(), "starting" | RUNNING | "waiting")
    }

    fn live(&self) -> bool {
        matches!(self.run.status.as_str(), "starting" | RUNNING)
    }

    /// Takes the judgements of this run's tool calls and names the deciders of the checks.
    fn set_judgements(&mut self, judgements: Vec<Judgement>) {
        self.hint = judgements.iter().rev().find_map(Judgement::hint);
        self.judgements = judgements
            .into_iter()
            .filter(|j| j.kind == "tool_call" && j.run == Some(self.run.id))
            .collect();
        self.conversation.name_deciders(&self.judgements);
    }

    /// A `run_event` of the stream. Whether the judgements are to be read again, as a check arrived.
    pub fn on_event(&mut self, event: RunEvent) -> bool {
        let checked = event.kind == "tool_check";
        self.conversation.push(event);
        self.conversation.name_deciders(&self.judgements);
        checked
    }

    /// The run ended, as the stream says.
    pub fn on_ended(&mut self, status: &str) {
        self.run.status = status.to_string();
        self.confirming = false;
    }

    /// Reads the judgements again.
    pub fn reload_judgements(&self, settings: Settings) -> Task<AppMessage> {
        let card = self.card;
        Task::perform(
            async move { Client::new(settings).judgements(card).await },
            |judgements| AppMessage::Live(Message::Judgements(judgements)),
        )
    }

    /// The shell's connection and the keys that go to it, while its pane is open. Closing the pane drops both,
    /// which closes the socket.
    pub fn subscription(&self, settings: &Settings) -> Subscription<AppMessage> {
        let Some(shell) = &self.shell else {
            return Subscription::none();
        };
        let connection = Subscription::run_with(
            (settings.clone(), self.run.id, shell.generation),
            |(settings, run, _)| {
                client::terminal(settings.clone(), format!("/v1/runs/{run}/terminal"))
            },
        )
        .map(|event| AppMessage::Live(Message::Shell(event)));
        let keys = keyboard::listen().map(|event| AppMessage::Live(Message::Key(event)));
        Subscription::batch([connection, keys])
    }
}
