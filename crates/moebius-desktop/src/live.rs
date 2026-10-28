//! The live view of a run (`docs/design/live-view.png`): its conversation (the agent's text, tool calls with the
//! decision of each check, the person's steers, errors), the steer field, stopping, and the panel of what the server
//! knows of the run, and the shell in its worktree. The state follows one run: a load of its events, then the stream's `run_event`s, which
//! `Conversation` puts together without gaps or repeats.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Border, Element, Length, Padding, Subscription, Task, keyboard};

use crate::app::Message as AppMessage;
use crate::client::{
    self, Board, Card, Client, ClientError, Judgement, Run, RunEvent, TerminalEvent,
};
use crate::conversation::{Conversation, Entry, Said};
use crate::page::{self, Tone, tag};
use crate::settings::Settings;
use crate::terminal::{PaneEvent, Shell};
use crate::{theme, transcript};

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

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Loaded(Ok(loaded)) => {
                self.run = loaded.run;
                self.now = loaded.now;
                self.notice = None;
                for event in loaded.events {
                    self.conversation.push(event);
                }
                self.set_judgements(loaded.judgements);
            }
            Message::Loaded(Err(e)) => self.notice = Some(say(&e)),
            Message::Judgements(Ok(judgements)) => self.set_judgements(judgements),
            Message::Judgements(Err(_)) => {}
            Message::SteerEdited(text) => self.steer = text,
            Message::Steer => {
                if let Some(settings) = settings.cloned()
                    && self.running()
                    && !self.steer.trim().is_empty()
                {
                    let (run, message) = (self.run.id, self.steer.clone());
                    return Task::perform(
                        async move { Client::new(settings).steer(run, &message).await },
                        |run| AppMessage::Live(Message::Steered(run)),
                    );
                }
            }
            Message::Steered(Ok(_)) => {
                self.steer.clear();
                self.notice = None;
            }
            Message::Steered(Err(e)) | Message::Stopped(Err(e)) => self.notice = Some(say(&e)),
            Message::AskStop => self.confirming = true,
            Message::KeepGoing => self.confirming = false,
            Message::Stop => {
                self.confirming = false;
                if let Some(settings) = settings.cloned() {
                    let run = self.run.id;
                    return Task::perform(
                        async move { Client::new(settings).stop(run).await },
                        |run| AppMessage::Live(Message::Stopped(run)),
                    );
                }
            }
            Message::Stopped(Ok(_)) => self.notice = None,
            Message::Tick(now) => self.now = now,
            Message::ToggleShell => {
                self.shell = match self.shell {
                    Some(_) => None,
                    None if self.has_worktree() => {
                        self.shells += 1;
                        Some(Shell::new(self.shells))
                    }
                    None => None,
                };
            }
            Message::Shell(event) => {
                if let Some(shell) = &mut self.shell {
                    shell.on_event(event);
                }
            }
            Message::ShellPane(event) => {
                if let Some(shell) = &mut self.shell {
                    match event {
                        PaneEvent::Resized(size) => shell.resized(size),
                        PaneEvent::Focus(focused) => shell.set_focus(focused),
                    }
                }
            }
            Message::Key(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) => {
                if let Some(shell) = &mut self.shell {
                    shell.key(&key, modifiers, text.as_deref());
                }
            }
            Message::Key(_) => {}
        }
        Task::none()
    }

    /// The shell's connection and the keys that go to it, while its pane is open. Closing the pane drops both,
    /// which closes the socket.
    pub fn subscription(&self, settings: &Settings) -> Subscription<AppMessage> {
        let Some(shell) = &self.shell else {
            return Subscription::none();
        };
        let connection = Subscription::run_with(
            (settings.clone(), self.run.id, shell.generation),
            |(settings, run, _)| client::terminal(settings.clone(), *run),
        )
        .map(|event| AppMessage::Live(Message::Shell(event)));
        let keys = keyboard::listen().map(|event| AppMessage::Live(Message::Key(event)));
        Subscription::batch([connection, keys])
    }

    /// The page: header, conversation with the steer field, and the panel of the run.
    pub fn view<'a>(&'a self, board: &'a Board, card: &'a Card) -> Element<'a, AppMessage> {
        let refused = self.conversation.refusals();
        let live = self.live();
        let round = match (card.rounds, board.round_limit) {
            (0, _) => None,
            (round, 0) => Some(format!("round {round}")),
            (round, limit) => Some(format!("round {round} of {limit}")),
        };
        let status = if live {
            "Live".to_string()
        } else {
            let mut status = self.run.status.clone();
            status[..1].make_ascii_uppercase();
            status
        };
        let chip = [Some(status), round]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let chip = tag(
            chip.join(" · "),
            if live { Tone::Teal } else { Tone::Plain },
        );
        let mut context = row![
            text(format!(
                "{} on {} · started {}",
                self.run.agent,
                harness_name(&self.run.harness),
                page::ago(self.now, self.run.started_at)
            ))
            .size(theme::CONTEXT)
            .color(theme::TEXT_SECONDARY)
        ];
        if refused > 0 {
            context = context.push(
                text(" · ")
                    .size(theme::CONTEXT)
                    .color(theme::TEXT_SECONDARY),
            );
            context = context.push(
                text(count_rules(refused))
                    .size(theme::CONTEXT)
                    .color(theme::RED_TEXT),
            );
        }
        let header = page::header_with(
            Some(page::card_breadcrumb(
                &board.repository.name,
                card.number,
                AppMessage::CloseCard,
            )),
            &card.title,
            Some(chip),
            context.into(),
            self.actions(),
        );
        let mut stream = column![
            scrollable(
                container(self.conversation_view())
                    .padding(Padding::from([16, 16]))
                    .width(Length::Fill)
            )
            .anchor_bottom()
            .height(Length::Fill),
        ]
        .width(Length::Fill);
        if let Some(shell) = &self.shell {
            stream = stream.push(
                container(shell.view(|event| AppMessage::Live(Message::ShellPane(event))))
                    .padding([0, 16])
                    .width(Length::Fill),
            );
        }
        let stream = container(stream.push(self.steer_view()))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| page::box_style());
        column![
            header,
            row![stream, self.panel(board, card, refused)]
                .spacing(14)
                .height(Length::Fill),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }

    /// "Open shell in worktree" and "Stop run" (or the question that confirms it), on the right of the header.
    fn actions(&self) -> Element<'_, AppMessage> {
        let mut actions = row![].spacing(10).align_y(Alignment::Center);
        if self.has_worktree() {
            let label = match self.shell {
                Some(_) => "Close shell",
                None => "Open shell in worktree",
            };
            actions = actions.push(page::white_button(
                label,
                Some(AppMessage::Live(Message::ToggleShell)),
            ));
        }
        if self.running() && self.confirming {
            actions = actions
                .push(
                    text("Stop this run?")
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY),
                )
                .push(page::white_button(
                    "Keep going",
                    Some(AppMessage::Live(Message::KeepGoing)),
                ))
                .push(page::white_button(
                    "Stop run",
                    Some(AppMessage::Live(Message::Stop)),
                ));
        } else if self.running() {
            actions = actions.push(page::white_button(
                "Stop run",
                Some(AppMessage::Live(Message::AskStop)),
            ));
        }
        actions.into()
    }

    fn conversation_view(&self) -> Element<'_, AppMessage> {
        if self.conversation.entries().is_empty() {
            return page::body("Nothing has happened in this run yet.");
        }
        column(self.conversation.entries().iter().map(entry_view))
            .spacing(12)
            .width(Length::Fill)
            .into()
    }

    /// The steer field and its button, under a dashed line, and the short message when something went wrong.
    fn steer_view(&self) -> Element<'_, AppMessage> {
        let running = self.running();
        let can_send = running && !self.steer.trim().is_empty();
        let field = text_input(
            "Steer this run, for example: also add a test for depth 0",
            &self.steer,
        )
        .on_input_maybe(running.then_some(|text| AppMessage::Live(Message::SteerEdited(text))))
        .on_submit_maybe(can_send.then_some(AppMessage::Live(Message::Steer)))
        .size(theme::BODY)
        .padding([10, 12])
        .style(|_, _| text_input::Style {
            background: theme::SURFACE.into(),
            border: Border {
                color: theme::LINE,
                width: 1.0,
                radius: theme::RADIUS_CONTROL.into(),
            },
            icon: theme::TEXT_MUTED,
            placeholder: theme::TEXT_MUTED,
            value: theme::TEXT,
            selection: theme::TEAL_FILL,
        });
        let send = button(
            text("Steer")
                .size(theme::BODY)
                .font(theme::manrope(Weight::Semibold)),
        )
        .padding([10, 18])
        .on_press_maybe(can_send.then_some(AppMessage::Live(Message::Steer)))
        .style(|_, status| {
            let (fill, ink) = match status {
                button::Status::Disabled => (theme::TRACK, theme::TEXT_MUTED),
                _ => (theme::AMBER, theme::ON_FILL),
            };
            button::Style {
                background: Some(fill.into()),
                text_color: ink,
                border: Border {
                    radius: theme::RADIUS_CONTROL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        });
        let hint = match (&self.notice, running) {
            (Some(notice), _) => notice.as_str(),
            (None, true) => "The run keeps going while you type. Steering is saved as a judgement.",
            (None, false) => "This run is not running, so it cannot be steered.",
        };
        column![
            page::rule(),
            column![
                row![field, send].spacing(10).align_y(Alignment::Center),
                text(hint).size(theme::LABEL).color(theme::TEXT_MUTED),
            ]
            .spacing(8)
            .padding([12, 16]),
        ]
        .into()
    }

    /// "This run": what the server knows of it.
    fn panel<'a>(
        &'a self,
        board: &'a Board,
        card: &'a Card,
        refused: usize,
    ) -> Element<'a, AppMessage> {
        let mut items: Vec<Element<'a, AppMessage>> = Vec::new();
        if card.rounds > 0 && board.round_limit > 0 {
            let line = format!("{} of {}", card.rounds, board.round_limit);
            items.push(item(
                "Round",
                Some(round_bar(card.rounds, board.round_limit)),
                text(line).size(theme::BODY).into(),
            ));
        }
        if card.tokens > 0 {
            items.push(item(
                "Tokens on this card",
                None,
                text(tokens(card.tokens)).size(theme::BODY).into(),
            ));
        }
        items.push(item(
            "Agent",
            None,
            row![
                text(self.run.agent.as_str())
                    .size(theme::BODY)
                    .color(theme::TEAL),
                text(format!(" · {}", harness_name(&self.run.harness))).size(theme::BODY),
            ]
            .into(),
        ));
        let mut judgements =
            row![text(format!("{} saved", self.judgements.len())).size(theme::BODY)];
        if refused > 0 {
            judgements = judgements.push(text(" · ").size(theme::BODY)).push(
                text(count_rules(refused))
                    .size(theme::BODY)
                    .font(theme::manrope(Weight::Semibold))
                    .color(theme::RED_TEXT),
            );
        }
        items.push(item("Judgements", None, judgements.into()));
        if let Some(change) = board.changes.iter().find(|c| c.card_id == Some(card.id)) {
            items.push(item(
                "Branch",
                None,
                text(change.branch.as_str())
                    .font(theme::MONO)
                    .size(theme::CODE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .into(),
            ));
        }
        let mut panel = column![
            text("This run")
                .size(theme::BOX_HEADING)
                .font(theme::manrope(Weight::Bold))
        ]
        .spacing(2);
        for (index, item) in items.into_iter().enumerate() {
            if index > 0 {
                panel = panel.push(page::rule());
            }
            panel = panel.push(item);
        }
        container(panel)
            .padding([14, 18])
            .width(300)
            .style(|_| page::box_style())
            .into()
    }
}

fn count_rules(n: usize) -> String {
    match n {
        1 => "1 rule broken".to_string(),
        n => format!("{n} rules broken"),
    }
}

fn harness_name(harness: &str) -> &str {
    match harness {
        "pi-bolt" => "Pi-Bolt",
        "" => "an unknown harness",
        other => other,
    }
}

/// A number of tokens: `412k`, `1.5M`.
fn tokens(n: i64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M tokens", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{}k tokens", n / 1_000),
        n => format!("{n} tokens"),
    }
}

/// A label over its value, in the panel; `bar` goes between them.
fn item<'a>(
    label: &'a str,
    bar: Option<Element<'a, AppMessage>>,
    value: Element<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    let mut item = column![
        text(label)
            .size(theme::LABEL)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::SIDEBAR_GROUP)
    ]
    .spacing(4);
    if let Some(bar) = bar {
        item = item.push(bar);
    }
    container(item.push(value))
        .padding(Padding::from([8, 0]))
        .into()
}

/// The segments of the rounds: one per round the card gets, the rounds so far filled.
fn round_bar<'a>(rounds: i64, limit: i64) -> Element<'a, AppMessage> {
    row((0..limit).map(|round| {
        let fill = if round < rounds {
            theme::TEAL
        } else {
            theme::TRACK
        };
        container(space())
            .width(Length::FillPortion(1))
            .height(4)
            .style(move |_| container::Style {
                background: Some(fill.into()),
                border: Border {
                    radius: theme::RADIUS_SEGMENT.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
    }))
    .spacing(3)
    .width(Length::Fill)
    .into()
}

fn entry_view(entry: &Entry) -> Element<'_, AppMessage> {
    match entry {
        Entry::Text(paragraph) => container(
            text(paragraph.as_str())
                .size(15)
                .line_height(1.55)
                .color(theme::TEXT),
        )
        .max_width(720)
        .into(),
        Entry::Tool(call) => transcript::tool_call(call),
        Entry::Person(said, message) => transcript::note(
            match said {
                Said::Steered => "You steered the run",
                Said::Said => "You said",
            },
            message,
            theme::LANE,
            theme::LINE,
            theme::TEXT_SECONDARY,
        ),
        Entry::Error(message) => transcript::error(message),
    }
}
