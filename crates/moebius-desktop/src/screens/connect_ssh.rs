//! The connect screen's way over SSH: the hosts of the SSH configuration and the servers connected to before, adding a
//! host, the steps of the setup with the log the person can open, the question of `ssh` for a password, and the offer to
//! install Moebius on a host that has none. The state is plain data; the app feeds it the events of `ssh::remote`.

use iced::font::Weight;
use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

use crate::app::Message as AppMessage;
use crate::screens::connect::input_style;
use crate::settings;
use crate::ssh::config::{self, AddError};
use crate::ssh::remote::{Event, Install, Step};
use crate::ssh::tunnel::TunnelEvent;
use crate::theme;
use crate::widgets::page::{self, Tone};

/// The lines of the log that are kept.
const LOG_LINES: usize = 500;

/// A host in the list: its name and what the configuration says of where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRow {
    pub name: String,
    pub detail: String,
}

/// Where the connection is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    /// Nothing runs; the list of hosts shows.
    #[default]
    Idle,
    Running {
        host: String,
        step: Step,
    },
    /// `moebius` is not on the host.
    Offer {
        host: String,
        system: String,
        url: String,
    },
    Failed {
        host: String,
        step: Step,
        error: String,
    },
}

/// A question of `ssh` and what is typed for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub text: String,
    pub answer: String,
}

impl Prompt {
    /// Whether the answer is a secret; the question of an unknown host, answered `yes`, is not.
    pub fn secret(&self) -> bool {
        !self.text.contains("(yes/no")
    }
}

#[derive(Debug, Clone, Default)]
pub struct SshConnect {
    /// Whether the SSH way is shown instead of the address and token.
    pub panel: bool,
    pub hosts: Vec<HostRow>,
    pub adding: bool,
    pub typed: String,
    pub add_error: Option<String>,
    pub phase: Phase,
    pub log: Vec<String>,
    pub show_log: bool,
    pub prompt: Option<Prompt>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// "Connect over SSH": the list of hosts opens.
    Open,
    /// Back to the address and token.
    Close,
    Pick(String),
    Adding,
    Typed(String),
    Add,
    Install(Install),
    /// Starts the setup of the host again.
    Retry(String),
    Cancel,
    ToggleLog,
    AnswerEdited(String),
    Answer,
    CancelAnswer,
}

impl SshConnect {
    /// The hosts of the configuration files and the servers connected to before, each once.
    pub fn list_hosts(&mut self, files: &[std::path::PathBuf], remembered: &[settings::Server]) {
        let mut rows: Vec<HostRow> = Vec::new();
        for file in files {
            for host in config::hosts(file) {
                if !rows.iter().any(|row| row.name == host.name) {
                    rows.push(HostRow {
                        detail: host.detail(),
                        name: host.name,
                    });
                }
            }
        }
        for host in remembered.iter().filter_map(|server| server.host.as_ref()) {
            if !rows.iter().any(|row| &row.name == host) {
                rows.push(HostRow {
                    name: host.clone(),
                    detail: String::new(),
                });
            }
        }
        self.hosts = rows;
    }

    /// Starts a connection to `host`: an empty log and the first step.
    pub fn begin(&mut self, host: &str) {
        self.log.clear();
        self.prompt = None;
        self.phase = Phase::Running {
            host: host.to_string(),
            step: Step::Connect,
        };
    }

    /// Takes the host just written to the configuration; the error is what to tell.
    pub fn added(&mut self, result: Result<String, AddError>) -> Option<String> {
        match result {
            Ok(name) => {
                self.typed.clear();
                self.adding = false;
                self.add_error = None;
                Some(name)
            }
            Err(e) => {
                self.add_error = Some(e.to_string());
                None
            }
        }
    }

    fn push_log(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > LOG_LINES {
            self.log.remove(0);
        }
    }

    pub fn host(&self) -> Option<&str> {
        match &self.phase {
            Phase::Idle => None,
            Phase::Running { host, .. }
            | Phase::Offer { host, .. }
            | Phase::Failed { host, .. } => Some(host),
        }
    }

    /// What the setup of the host tells.
    pub fn on_setup(&mut self, event: Event) {
        let Some(host) = self.host().map(str::to_string) else {
            return;
        };
        match event {
            Event::Step(step) => {
                self.phase = Phase::Running { host, step };
                self.prompt = None;
            }
            Event::Log(line) => self.push_log(line),
            Event::Prompt(text) => {
                self.prompt = Some(Prompt {
                    text,
                    answer: String::new(),
                })
            }
            Event::Missing { system, url } => {
                self.prompt = None;
                self.phase = Phase::Offer { host, system, url };
            }
            Event::Ready { .. } => {
                self.prompt = None;
                self.phase = Phase::Running {
                    host,
                    step: Step::Tunnel,
                };
            }
            Event::Failed(error) => {
                let step = match self.phase {
                    Phase::Running { step, .. } => step,
                    _ => Step::Connect,
                };
                self.push_log(error.clone());
                self.prompt = None;
                self.phase = Phase::Failed { host, step, error };
            }
        }
    }

    /// What the tunnel tells while the connection is made; it only logs and asks.
    pub fn on_tunnel(&mut self, event: &TunnelEvent) {
        match event {
            TunnelEvent::Up => self.push_log("The tunnel is up.".to_string()),
            TunnelEvent::Down(reason) => self.push_log(format!("The tunnel is down: {reason}")),
            TunnelEvent::Prompt(text) => {
                self.prompt = Some(Prompt {
                    text: text.clone(),
                    answer: String::new(),
                })
            }
        }
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        match &self.phase {
            Phase::Idle => self.hosts_view(),
            Phase::Running { host, step } => self.steps_view(host, *step, None),
            Phase::Failed { host, step, error } => self.steps_view(host, *step, Some(error)),
            Phase::Offer { host, system, url } => self.offer_view(host, system, url),
        }
    }

    fn hosts_view(&self) -> Element<'_, AppMessage> {
        let mut hosts = column![].spacing(theme::SPACE_4);
        for host in &self.hosts {
            hosts = hosts.push(host_row(host));
        }
        if self.hosts.is_empty() {
            hosts = hosts.push(
                text("No hosts in the SSH configuration yet.")
                    .size(theme::CONTEXT)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
            );
        }
        let mut add = column![page::white_button(
            "Add new SSH host",
            (!self.adding).then_some(ssh(Message::Adding))
        )]
        .spacing(theme::SPACE_6);
        if self.adding {
            add = add.push(
                text_input("user@host or ssh -p 22 user@host", &self.typed)
                    .on_input(|typed| ssh(Message::Typed(typed)))
                    .on_submit(ssh(Message::Add))
                    .padding(theme::CONTROL_PADDING)
                    .size(theme::BODY)
                    .style(input_style),
            );
            add = add.push(page::primary_button(
                "Add host",
                (!self.typed.trim().is_empty()).then_some(ssh(Message::Add)),
            ));
            if let Some(error) = &self.add_error {
                add = add.push(
                    text(error)
                        .size(theme::CONTEXT)
                        .font(theme::MONO)
                        .color(theme::RED_TEXT),
                );
            }
        }
        column![
            text("Connect over SSH")
                .size(theme::PAGE_TITLE)
                .font(theme::manrope(Weight::Bold)),
            page::context(
                "Pick a host. Moebius is set up there, and the connection is kept alive."
            ),
            container(scrollable(hosts)).max_height(theme::HOST_LIST_HEIGHT),
            add,
            page::white_button("Connect by address", Some(ssh(Message::Close))),
        ]
        .spacing(theme::SPACE_12)
        .into()
    }

    fn steps_view<'a>(
        &'a self,
        host: &'a str,
        step: Step,
        failed: Option<&'a String>,
    ) -> Element<'a, AppMessage> {
        // Installing is a step only for a host that needed it.
        let installing = step == Step::Install
            || self
                .log
                .iter()
                .any(|l| l.starts_with("$ download") || l.starts_with("$ upload"));
        let mut steps = column![].spacing(theme::SPACE_6);
        for listed in Step::ALL {
            if listed == Step::Install && !installing {
                continue;
            }
            let (label, tone) = if listed < step {
                ("done", Tone::Teal)
            } else if listed == step && failed.is_some() {
                ("failed", Tone::Red)
            } else if listed == step {
                ("running", Tone::Plain)
            } else {
                ("", Tone::Plain)
            };
            let tag: Element<'_, AppMessage> = if label.is_empty() {
                iced::widget::space().into()
            } else {
                page::tag(label.to_string(), tone)
            };
            steps = steps.push(
                row![
                    container(tag).width(theme::STEP_TAG_WIDTH),
                    text(listed.label())
                        .size(theme::BODY)
                        .color(if listed > step {
                            theme::TEXT_MUTED
                        } else {
                            theme::TEXT
                        }),
                ]
                .spacing(theme::SPACE_8)
                .align_y(iced::Center),
            );
        }
        let mut content = column![
            text(format!("Connecting to {host}"))
                .size(theme::PAGE_TITLE)
                .font(theme::manrope(Weight::Bold)),
            steps,
        ]
        .spacing(theme::SPACE_12);
        if let Some(error) = failed {
            content = content.push(
                text(error)
                    .size(theme::CONTEXT)
                    .font(theme::MONO)
                    .color(theme::RED_TEXT),
            );
        }
        if let Some(prompt) = &self.prompt {
            content = content.push(self.prompt_view(prompt));
        }
        content = content.push(self.log_view());
        let mut buttons = row![].spacing(theme::SPACE_8);
        if failed.is_some() {
            buttons = buttons.push(page::primary_button(
                "Try again",
                Some(ssh(Message::Retry(host.to_string()))),
            ));
        }
        buttons = buttons.push(page::white_button(
            if failed.is_some() { "Back" } else { "Cancel" },
            Some(ssh(Message::Cancel)),
        ));
        content.push(buttons).into()
    }

    fn prompt_view<'a>(&'a self, prompt: &'a Prompt) -> Element<'a, AppMessage> {
        column![
            text(&prompt.text)
                .size(theme::CONTEXT)
                .font(theme::MONO)
                .color(theme::TEXT_SECONDARY),
            text_input("", &prompt.answer)
                .on_input(|answer| ssh(Message::AnswerEdited(answer)))
                .on_submit(ssh(Message::Answer))
                .secure(prompt.secret())
                .padding(theme::CONTROL_PADDING)
                .size(theme::BODY)
                .style(input_style),
            row![
                page::primary_button("Answer", Some(ssh(Message::Answer))),
                page::white_button("Cancel", Some(ssh(Message::CancelAnswer))),
            ]
            .spacing(theme::SPACE_8),
        ]
        .spacing(theme::SPACE_6)
        .into()
    }

    fn log_view(&self) -> Element<'_, AppMessage> {
        let toggle = page::white_button(
            if self.show_log {
                "Hide log"
            } else {
                "Show log"
            },
            Some(ssh(Message::ToggleLog)),
        );
        if !self.show_log {
            return toggle;
        }
        let lines = column(self.log.iter().map(|line| {
            text(line)
                .size(theme::CODE_SMALL)
                .font(theme::MONO)
                .color(theme::TEXT_SECONDARY)
                .into()
        }));
        column![
            toggle,
            container(scrollable(lines).width(Length::Fill))
                .padding(theme::SPACE_8)
                .height(theme::TERMINAL_HEIGHT)
                .style(|_| container::Style {
                    background: Some(theme::LANE.into()),
                    ..page::box_style()
                }),
        ]
        .spacing(theme::SPACE_6)
        .into()
    }

    fn offer_view<'a>(
        &'a self,
        host: &'a str,
        system: &'a str,
        url: &'a str,
    ) -> Element<'a, AppMessage> {
        column![
            text(format!("Moebius is not installed on {host}"))
                .size(theme::PAGE_TITLE)
                .font(theme::manrope(Weight::Bold)),
            page::context(format!("{system}. Release:")),
            text(url)
                .size(theme::CODE_SMALL)
                .font(theme::MONO)
                .color(theme::TEXT_SECONDARY),
            page::primary_button(
                "Install on the host",
                Some(ssh(Message::Install(Install::Download)))
            ),
            page::white_button(
                "Download here and upload",
                Some(ssh(Message::Install(Install::Upload)))
            ),
            page::white_button("Cancel", Some(ssh(Message::Cancel))),
        ]
        .spacing(theme::SPACE_12)
        .into()
    }
}

fn ssh(message: Message) -> AppMessage {
    AppMessage::Ssh(message)
}

fn host_row(host: &HostRow) -> Element<'_, AppMessage> {
    iced::widget::button(
        row![
            text(&host.name)
                .size(theme::BODY)
                .font(theme::manrope(Weight::Semibold)),
            iced::widget::space::horizontal(),
            text(&host.detail)
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
        ]
        .align_y(iced::Center),
    )
    .width(Length::Fill)
    .padding(theme::CONTROL_PADDING)
    .on_press(ssh(Message::Pick(host.name.clone())))
    .style(|_, status| iced::widget::button::Style {
        background: Some(
            if status == iced::widget::button::Status::Hovered {
                theme::HEAD_PILL
            } else {
                theme::SURFACE
            }
            .into(),
        ),
        text_color: theme::TEXT,
        border: iced::Border {
            color: theme::LINE,
            width: theme::HAIRLINE,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..iced::widget::button::Style::default()
    })
    .into()
}
