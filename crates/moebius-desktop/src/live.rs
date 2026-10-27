//! The live view of a run (`docs/design/live-view.png`): its conversation (the agent's text, tool calls with the
//! decision of each check, the person's steers, errors), the steer field, stopping, and the panel of what the server
//! knows of the run. The state follows one run: a load of its events, then the stream's `run_event`s, which
//! `Conversation` puts together without gaps or repeats.

use iced::font::Weight;
use iced::highlighter::{self, Stream};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, rich_text, row, scrollable, space, span, text, text_input,
};
use iced::{Alignment, Border, Color, Element, Font, Length, Padding, Task};

use crate::app::Message as AppMessage;
use crate::client::{Board, Card, Client, ClientError, Judgement, Run, RunEvent};
use crate::conversation::{Conversation, Entry, Execution, Input, Said, ToolCall};
use crate::page::{self, Tone, tag};
use crate::settings::Settings;
use crate::theme;

/// The most lines of a tool's output, or of a diff or a file, drawn; the rest is said to be left out.
const MAX_LINES: usize = 200;

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
}

/// Unix seconds now.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

fn say(error: &ClientError) -> String {
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
        }
        Task::none()
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
        let stream = container(
            column![
                scrollable(
                    container(self.conversation_view())
                        .padding(Padding::from([16, 16]))
                        .width(Length::Fill)
                )
                .anchor_bottom()
                .height(Length::Fill),
                self.steer_view(),
            ]
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| boxed());
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

    /// "Stop run", or the question that confirms it, on the right of the header.
    fn actions(&self) -> Element<'_, AppMessage> {
        if !self.running() {
            return space().into();
        }
        if self.confirming {
            return row![
                text("Stop this run?")
                    .size(theme::CONTEXT)
                    .color(theme::TEXT_SECONDARY),
                white_button("Keep going", AppMessage::Live(Message::KeepGoing)),
                white_button("Stop run", AppMessage::Live(Message::Stop)),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into();
        }
        white_button("Stop run", AppMessage::Live(Message::AskStop))
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
            rule(),
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
                panel = panel.push(rule());
            }
            panel = panel.push(item);
        }
        container(panel)
            .padding([14, 18])
            .width(300)
            .style(|_| boxed())
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

/// A box of the page: white, with a line and round corners.
fn boxed() -> container::Style {
    container::Style {
        background: Some(theme::SURFACE.into()),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_BOX.into(),
        },
        ..container::Style::default()
    }
}

/// A line between the parts of a box.
fn rule<'a>() -> Element<'a, AppMessage> {
    container(space())
        .width(Length::Fill)
        .height(1)
        .style(|_| container::Style {
            background: Some(theme::TRACK.into()),
            ..container::Style::default()
        })
        .into()
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

/// A white button with a line, for everything that is not the page's one primary button.
fn white_button<'a>(label: &'a str, message: AppMessage) -> Element<'a, AppMessage> {
    button(
        text(label)
            .size(theme::CONTEXT)
            .font(theme::manrope(Weight::Semibold)),
    )
    .padding([8, 14])
    .on_press(message)
    .style(|_, _| button::Style {
        background: Some(theme::SURFACE.into()),
        text_color: theme::TEXT,
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..button::Style::default()
    })
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
        Entry::Tool(call) if call.refused() => refused_view(call),
        Entry::Tool(call) => tool_view(call),
        Entry::Person(said, message) => note(
            match said {
                Said::Steered => "You steered the run",
                Said::Said => "You said",
            },
            message,
            theme::LANE,
            theme::LINE,
            theme::TEXT_SECONDARY,
        ),
        Entry::Error(message) => note(
            "The run failed",
            message,
            theme::RED_FILL,
            theme::RED_BORDER,
            theme::RED_TEXT,
        ),
    }
}

/// A message in a box with a small label above it.
fn note<'a>(
    label: &'a str,
    message: &'a str,
    fill: Color,
    border: Color,
    ink: Color,
) -> Element<'a, AppMessage> {
    container(
        column![
            text(label)
                .size(theme::LABEL)
                .font(theme::manrope(Weight::Bold))
                .color(ink),
            text(message).size(theme::CONTEXT).color(theme::TEXT),
        ]
        .spacing(4),
    )
    .padding([10, 12])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(fill.into()),
        border: Border {
            color: border,
            width: 1.0,
            radius: theme::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// The name of a tool in its pill.
fn tool_name<'a>(name: &'a str, fill: Color) -> Element<'a, AppMessage> {
    container(
        text(name)
            .font(theme::MONO)
            .size(theme::CODE)
            .color(theme::TEXT),
    )
    .padding([2, 8])
    .style(move |_| container::Style {
        background: Some(fill.into()),
        border: Border {
            radius: theme::RADIUS_ROUND.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// The first row of a tool call: its name, what it works on, and on the right what the check and the tool did.
fn head<'a>(call: &'a ToolCall, blocked: bool) -> Element<'a, AppMessage> {
    let fill = if blocked {
        theme::SURFACE
    } else {
        theme::HEAD_PILL
    };
    let arg = text(call.input.summary())
        .font(theme::MONO)
        .size(theme::CODE)
        .wrapping(Wrapping::None)
        .color(if blocked {
            theme::RED_TEXT
        } else {
            theme::TEXT_SECONDARY
        });
    let mut right = row![].spacing(8).align_y(Alignment::Center);
    match (&call.check, &call.execution) {
        (Some(check), _) if !check.allowed => {
            right = right.push(tag("Blocked".to_string(), Tone::Red))
        }
        (_, Some(Execution::Running)) => right = right.push(tag("Running".to_string(), Tone::Teal)),
        (Some(_), _) => right = right.push(tag("Allowed".to_string(), Tone::Teal)),
        _ => {}
    }
    let mut notes = Vec::new();
    if let Some(decider) = call.check.as_ref().and_then(|c| c.decider.as_deref()) {
        notes.push(capitalized(decider));
    }
    if !blocked
        && let Some(Execution::Finished {
            millis: Some(ms),
            failed,
        }) = &call.execution
    {
        notes.push(match failed {
            true => format!("failed after {}", duration(*ms)),
            false => format!("ran {}", duration(*ms)),
        });
    }
    if !notes.is_empty() {
        right = right.push(
            text(notes.join(" · "))
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED),
        );
    }
    row![
        tool_name(call.tool.as_str(), fill),
        container(arg).width(Length::Fill).clip(true),
        right,
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// A decider's name as the page writes it: `patterns` is the Pattern check.
fn capitalized(word: &str) -> String {
    let word = if word == "patterns" { "pattern" } else { word };
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A time in the unit a person reads: `12 ms`, `0.4 s`, `14 s`.
fn duration(ms: u64) -> String {
    match ms {
        0..1000 => format!("{ms} ms"),
        1000..10_000 => format!("{:.1} s", ms as f64 / 1000.0),
        _ => format!("{} s", ms / 1000),
    }
}

fn tool_view(call: &ToolCall) -> Element<'_, AppMessage> {
    let mut content = column![head(call, false)].spacing(8);
    match &call.input {
        Input::Edit { path, changes } => content = content.push(diff_view(path, changes)),
        Input::Write {
            path,
            content: file,
        } => {
            content = content.push(file_view(path, file));
        }
        _ if !call.output.is_empty() && !matches!(call.input, Input::Read(_)) => {
            content = content.push(output_view(&call.output));
        }
        _ => {}
    }
    container(content)
        .padding([10, 12])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::LANE.into()),
            border: Border {
                color: theme::TRACK,
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// A refused call: the red box with the rule broken and why. The call did not run.
fn refused_view(call: &ToolCall) -> Element<'_, AppMessage> {
    let check = call.check.as_ref().expect("a refused call has its check");
    let (title, why) = match check.rule() {
        Some((rule, why)) => (
            row![
                text("Rule broken: ")
                    .size(13.5)
                    .font(theme::manrope(Weight::Bold))
                    .color(theme::RED_TEXT),
                text(rule)
                    .font(theme::MONO)
                    .size(theme::CODE)
                    .color(theme::RED_TEXT),
            ],
            why,
        ),
        None => (
            row![
                text("Refused")
                    .size(13.5)
                    .font(theme::manrope(Weight::Bold))
                    .color(theme::RED_TEXT)
            ],
            check.reason.as_str(),
        ),
    };
    let dot = container(space().width(8).height(8)).style(|_| container::Style {
        background: Some(theme::RED.into()),
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    });
    container(
        column![
            row![dot, title].spacing(8).align_y(Alignment::Center),
            head(call, true),
            text(why).size(theme::CONTEXT).color(theme::TEXT),
        ]
        .spacing(8),
    )
    .padding([10, 12])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(theme::RED_FILL.into()),
        border: Border {
            color: theme::RED_BORDER,
            width: 1.5,
            radius: theme::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

/// The lines of the last `MAX_LINES`, with a note when earlier ones are left out.
fn tail<'a>(lines: &[&'a str]) -> (usize, Vec<&'a str>) {
    let skipped = lines.len().saturating_sub(MAX_LINES);
    (skipped, lines[skipped..].to_vec())
}

/// What a tool printed, in the dark block: the last lines, scrollable.
fn output_view(output: &str) -> Element<'_, AppMessage> {
    let (skipped, lines) = tail(&lines_of(output));
    let mut block = column![].spacing(0);
    if skipped > 0 {
        block = block.push(
            text(format!("{skipped} earlier lines left out"))
                .font(theme::MONO)
                .size(theme::CODE_SMALL)
                .color(theme::TERMINAL_DIM),
        );
    }
    block = block.push(
        text(lines.join("\n"))
            .font(theme::MONO)
            .size(theme::CODE_SMALL)
            .line_height(1.6)
            .color(theme::TERMINAL_TEXT),
    );
    container(scrollable(
        container(block).padding([10, 12]).width(Length::Fill),
    ))
    .max_height(240)
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(theme::TERMINAL.into()),
        border: Border {
            radius: theme::RADIUS_CONTROL.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// The highlighted spans of each line of `lines`, for the language of `path`'s extension.
fn highlighted<'a>(lines: &[&str], path: &str) -> Vec<Vec<iced::widget::text::Span<'a, (), Font>>> {
    let token = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt")
        .to_string();
    let mut stream = Stream::new(&highlighter::Settings {
        theme: highlighter::Theme::InspiredGitHub,
        token,
    });
    lines
        .iter()
        .map(|line| {
            let spans: Vec<_> = stream
                .highlight_line(line)
                .map(|(range, highlight)| {
                    let mut piece = span(line[range].to_string()).font(theme::MONO);
                    if let Some(color) = highlight.color() {
                        piece = piece.color(color);
                    }
                    if let Some(font) = highlight.font() {
                        piece = piece.font(Font {
                            family: theme::MONO.family,
                            ..font
                        });
                    }
                    piece
                })
                .collect();
            stream.commit();
            if spans.is_empty() {
                vec![span(" ".to_string()).font(theme::MONO)]
            } else {
                spans
            }
        })
        .collect()
}

/// One line of code: the marker (`+`, `-` or a space), the line, and the fill of the row.
fn code_row<'a>(
    marker: char,
    spans: Vec<iced::widget::text::Span<'a, (), Font>>,
    fill: Option<Color>,
) -> Element<'a, AppMessage> {
    let ink = match marker {
        '-' => theme::DIFF_REMOVED_MARK,
        '+' => theme::DIFF_ADDED_MARK,
        _ => theme::TEXT_MUTED,
    };
    container(
        row![
            text(marker.to_string())
                .font(theme::MONO)
                .size(theme::CODE)
                .color(ink)
                .width(24)
                .align_x(Alignment::Center),
            rich_text(spans)
                .size(theme::CODE)
                .line_height(1.6)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: fill.map(Into::into),
        ..container::Style::default()
    })
    .into()
}

/// Code in a white frame under a header line with the path on the left and `note` on the right. The rows are
/// scrollable, and `left_out` lines that were not drawn are said so at the end.
fn code_frame<'a>(
    path: &'a str,
    note: Option<String>,
    rows: Vec<Element<'a, AppMessage>>,
    left_out: usize,
) -> Element<'a, AppMessage> {
    let mut title = row![
        text(path)
            .font(theme::MONO)
            .size(theme::CODE_SMALL)
            .color(theme::TEXT_SECONDARY),
        space::horizontal(),
    ];
    if let Some(note) = note {
        title = title.push(
            text(note)
                .font(theme::MONO)
                .size(theme::CODE_SMALL)
                .color(theme::TEXT_SECONDARY),
        );
    }
    let mut body = column(rows);
    if left_out > 0 {
        body = body.push(
            container(
                text(format!("{left_out} more lines left out"))
                    .font(theme::MONO)
                    .size(theme::CODE_SMALL)
                    .color(theme::TEXT_MUTED),
            )
            .padding([4, 10]),
        );
    }
    container(column![
        container(title)
            .padding([6, 10])
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(theme::LANE.into()),
                ..container::Style::default()
            }),
        container(body).padding([4, 0]),
    ])
    .width(Length::Fill)
    .clip(true)
    .style(|_| container::Style {
        background: Some(theme::SURFACE.into()),
        border: Border {
            color: theme::TRACK,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// A file's first lines, highlighted for its language.
fn file_view<'a>(path: &'a str, file: &str) -> Element<'a, AppMessage> {
    let lines = lines_of(file);
    let rows = highlighted(&lines[..lines.len().min(MAX_LINES)], path)
        .into_iter()
        .map(|spans| code_row(' ', spans, None))
        .collect();
    code_frame(path, None, rows, lines.len().saturating_sub(MAX_LINES))
}

/// An edit's changes as removed and added lines, each side highlighted for the file's language.
fn diff_view<'a>(path: &'a str, changes: &[(String, String)]) -> Element<'a, AppMessage> {
    let mut rows: Vec<Element<'a, AppMessage>> = Vec::new();
    let (mut removed, mut added) = (0, 0);
    for (old, new) in changes {
        let (old, new) = (lines_of(old), lines_of(new));
        removed += old.len();
        added += new.len();
        // Past the cap the lines are counted but not highlighted or drawn.
        let room = MAX_LINES.saturating_sub(rows.len());
        for spans in highlighted(&old[..old.len().min(room)], path) {
            rows.push(code_row('-', spans, Some(theme::DIFF_REMOVED)));
        }
        let room = MAX_LINES.saturating_sub(rows.len());
        for spans in highlighted(&new[..new.len().min(room)], path) {
            rows.push(code_row('+', spans, Some(theme::DIFF_ADDED)));
        }
    }
    let left_out = removed + added - rows.len();
    code_frame(path, Some(format!("-{removed} +{added}")), rows, left_out)
}
