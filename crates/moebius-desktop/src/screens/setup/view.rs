//! How the Models, Plugins and Settings pages are drawn: their boxes, the banner that asks for a restart, and the
//! tables and labelled values they show.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message as AppMessage;
use crate::screens::board::tokens;
use crate::settings::{self, Settings};
use crate::theme;
use crate::widgets::form::notice_line;
use crate::widgets::model_picker;
use crate::widgets::page;

use super::inputs::{field_view, optional_box, upstream_row};
use super::{Message, Notice, Owner, Setup};

/// The amber line that says the server has to restart for some keys to be used, with the button that restarts it.
fn banner<'a>(line: String, restart: Option<AppMessage>) -> Element<'a, AppMessage> {
    let mut content = row![
        text(line)
            .size(theme::CONTEXT)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::AMBER_TEXT)
            .width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if restart.is_some() {
        content = content.push(page::primary_button("Restart", restart));
    }
    container(content)
        .padding([12, 16])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::AMBER_FILL.into()),
            border: iced::Border {
                color: theme::AMBER_BORDER,
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// A page of the Setup group: the header with its `actions`, and `content` that scrolls.
fn opened<'a>(
    title: &'a str,
    context: &str,
    actions: Element<'a, AppMessage>,
    content: iced::widget::Column<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    column![
        page::header(
            "Setup",
            title,
            None,
            page::context(context.to_string()),
            actions,
        ),
        scrollable(container(content).padding(Padding::default().right(12))).height(Length::Fill),
    ]
    .spacing(theme::PAGE_GAP)
    .width(Length::Fill)
    .into()
}

/// A table with a row of headings; each of `rows` has one element for each column, of the portion of the heading.
pub(super) fn table<'a>(
    columns: &[(&'static str, u16)],
    rows: Vec<Vec<Element<'a, AppMessage>>>,
) -> Element<'a, AppMessage> {
    let heads = columns
        .iter()
        .fold(row![].spacing(10), |heads, (label, portion)| {
            heads.push(cell(
                *portion,
                text(*label)
                    .size(theme::LABEL)
                    .font(theme::manrope(Weight::Semibold))
                    .color(theme::TEXT_MUTED),
            ))
        });
    let mut table = column![heads, page::rule()].spacing(8);
    for cells in rows {
        let line = cells
            .into_iter()
            .zip(columns)
            .fold(row![].spacing(10), |line, (content, (_, portion))| {
                line.push(cell(*portion, content))
            });
        table = table.push(line).push(page::rule());
    }
    table.into()
}

/// Labelled values, one to a line.
fn facts<'a>(lines: Vec<(&'static str, Element<'a, AppMessage>)>) -> Element<'a, AppMessage> {
    let mut facts = column![].spacing(10);
    for (label, value) in lines {
        facts = facts.push(
            row![
                container(
                    text(label)
                        .size(theme::BODY)
                        .font(theme::manrope(Weight::Semibold))
                        .color(theme::TEXT_MUTED)
                )
                .width(Length::Fixed(170.0)),
                container(value).width(Length::Fill),
            ]
            .spacing(10),
        );
    }
    facts.into()
}

fn cell<'a>(portion: u16, content: impl Into<Element<'a, AppMessage>>) -> Element<'a, AppMessage> {
    container(content.into())
        .width(Length::FillPortion(portion))
        .into()
}

fn strong<'a>(line: &'a str) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::BODY)
        .font(theme::manrope(Weight::Semibold))
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

fn plain<'a>(line: String) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::BODY)
        .color(theme::TEXT_SECONDARY)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

fn mono<'a>(line: String) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::CODE_SMALL)
        .font(theme::MONO)
        .color(theme::TEXT)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

impl Setup {
    pub fn models_page(&self) -> Element<'_, AppMessage> {
        let (Some(models), Some(forms)) = (&self.models, &self.forms) else {
            return opened(
                "Models",
                "Reading the models of the server.",
                text("").into(),
                column![],
            );
        };
        let secrets = self.server.as_ref().map_or(&[][..], |s| &s.secrets[..]);
        let upstreams: Element<AppMessage> = if forms.upstreams.is_empty() {
            page::body("No upstream is configured; every model request fails.")
        } else {
            column(
                forms
                    .upstreams
                    .iter()
                    .enumerate()
                    .map(|(place, upstream)| upstream_row(place, upstream, secrets)),
            )
            .spacing(12)
            .into()
        };
        let upstreams = page::boxed(
            "Upstreams",
            column![
                upstreams,
                page::white_button(
                    "Add upstream",
                    Some(AppMessage::Setup(Message::AddUpstream))
                ),
            ]
            .spacing(12),
        );
        let agents: Element<AppMessage> = if models.agents.is_empty() {
            page::body("The server has no agent.")
        } else {
            table(
                &[
                    ("Agent", 3),
                    ("Model", 4),
                    ("Protocol", 2),
                    ("Thinking", 2),
                    ("Budget", 2),
                ],
                models
                    .agents
                    .iter()
                    .map(|agent| {
                        let (model, protocol) = match &agent.error {
                            Some(_) => ("Cannot run".to_string(), String::new()),
                            None => (agent.model.clone(), agent.protocol.clone()),
                        };
                        vec![
                            strong(&agent.name),
                            plain(model),
                            plain(protocol),
                            plain(agent.thinking.clone().unwrap_or_else(|| "–".into())),
                            plain(agent.token_budget.map_or_else(|| "–".into(), tokens)),
                        ]
                    })
                    .collect(),
            )
        };
        let agents = page::boxed(
            "Agents",
            column![agents, page::body("Agents are edited on the Agents page.")].spacing(10),
        );
        // What the upstreams said is shown once, under the first Model field on the page.
        let note = |first: bool| {
            let available = self.available.as_ref().filter(|_| first)?;
            Some(model_picker::note(
                available,
                AppMessage::Setup(Message::RefreshAvailable),
            ))
        };
        let teacher_on = forms.teacher.as_ref().is_some_and(|t| t.on);
        let teacher = forms.teacher.as_ref().map(|teacher| {
            optional_box(
                "Teacher",
                "The teacher model checks the rules written in prose.",
                "No teacher is configured; a call is refused while a rule in prose applies.",
                teacher,
                Message::ToggleTeacher,
                None,
                note(true),
            )
        });
        let decision_model = forms.decision_model.as_ref().map(|model| {
            optional_box(
                "Decision model",
                "The decision model shadows the teacher and can take kinds of decision over.",
                "No decision model is configured.",
                model,
                Message::ToggleDecisionModel,
                Some((Owner::DecisionModel, secrets)),
                note(!teacher_on),
            )
        });
        let mut content = column![self.status(), upstreams, agents].spacing(theme::PAGE_GAP);
        let mut boxes = row![].spacing(theme::PAGE_GAP).align_y(Alignment::Start);
        for element in [teacher, decision_model].into_iter().flatten() {
            boxes = boxes.push(container(element).width(Length::FillPortion(1)));
        }
        content = content.push(boxes);
        opened(
            "Models",
            "Where model requests go, and the models that check and decide. Keys are never shown.",
            page::primary_button("Save", Some(AppMessage::Setup(Message::SaveModels))),
            content,
        )
    }

    pub fn plugins_page(&self) -> Element<'_, AppMessage> {
        let Some(plugins) = &self.plugins else {
            return opened(
                "Plugins",
                "Reading the plugins of the server.",
                text("").into(),
                column![],
            );
        };
        let list: Element<AppMessage> = if plugins.plugins.is_empty() {
            page::body("No plugin is loaded.")
        } else {
            table(
                &[
                    ("Plugin", 3),
                    ("Version", 2),
                    ("Kinds", 2),
                    ("Time limit", 2),
                    ("Decides", 4),
                ],
                plugins
                    .plugins
                    .iter()
                    .map(|plugin| {
                        vec![
                            mono(plugin.name.clone()),
                            plain(plugin.version.clone()),
                            plain(plugin.kinds.join(", ")),
                            plain(format!("{} s", plugins.time_limit_seconds)),
                            if plugin.decides.is_empty() {
                                plain("No kind of decision".to_string())
                            } else {
                                mono(plugin.decides.join(", "))
                            },
                        ]
                    })
                    .collect(),
            )
        };
        opened(
            "Plugins",
            &format!(
                "{} plugins loaded; a call into one is refused after {} s.",
                plugins.plugins.len(),
                plugins.time_limit_seconds
            ),
            text("").into(),
            column![page::boxed("Loaded plugins", list)],
        )
    }

    /// The Settings page: every key of `moebius.toml` as an input, and where this app is connected, `saved` being the
    /// connection.
    pub fn settings_page(&self, saved: Option<&Settings>) -> Element<'_, AppMessage> {
        let (address, token) = match saved {
            Some(saved) => (saved.address.clone(), settings::mask(&saved.token)),
            None => ("none".to_string(), "none".to_string()),
        };
        let app = page::boxed(
            "This app",
            column![
                facts(vec![
                    ("Server address", mono(address)),
                    ("Token", mono(token))
                ]),
                page::white_button(
                    "Connect to another server",
                    Some(AppMessage::ChangeConnection)
                ),
            ]
            .spacing(14),
        );
        let (Some(server), Some(forms)) = (&self.server, &self.forms) else {
            return opened(
                "Settings",
                "Reading the settings of the server.",
                text("").into(),
                column![page::boxed(
                    "Server",
                    page::body("Reading the settings of the server.")
                )]
                .push(app)
                .spacing(theme::PAGE_GAP),
            );
        };
        let mut boxes = forms.settings.iter().map(|(title, form)| {
            page::boxed(
                title,
                column(form.fields.iter().map(|field| field_view(field, None))).spacing(12),
            )
        });
        let mut content = column![self.status()].spacing(theme::PAGE_GAP);
        while let Some(first) = boxes.next() {
            let mut line = row![container(first).width(Length::FillPortion(1))]
                .spacing(theme::PAGE_GAP)
                .align_y(Alignment::Start);
            if let Some(second) = boxes.next() {
                line = line.push(container(second).width(Length::FillPortion(1)));
            }
            content = content.push(line);
        }
        content = content.push(app);
        opened(
            "Settings",
            &format!(
                "The keys of {}. Save writes only what you changed.",
                server.file
            ),
            page::primary_button("Save", Some(AppMessage::Setup(Message::SaveSettings))),
            content,
        )
    }

    /// The banner that says a restart is needed, and what a save said.
    pub(super) fn status(&self) -> Element<'_, AppMessage> {
        let mut status = column![].spacing(10);
        if self.restarting {
            status = status.push(banner(
                "Restarting the server. The app connects again by itself.".to_string(),
                None,
            ));
        } else if let Some(server) = self.server.as_ref().filter(|s| s.restart_needed) {
            status = status.push(banner(
                format!(
                    "Restart the server to use: {}",
                    server.restart_keys.join(", ")
                ),
                Some(AppMessage::Setup(Message::Restart)),
            ));
        }
        match &self.notice {
            Some(Notice::Saved) => status = status.push(notice_line("Saved", false)),
            Some(Notice::NothingChanged) => {
                status = status.push(notice_line("Nothing changed, so nothing was saved.", false));
            }
            Some(Notice::Failed(problem)) => status = status.push(notice_line(problem, true)),
            None => {}
        }
        status.into()
    }
}
