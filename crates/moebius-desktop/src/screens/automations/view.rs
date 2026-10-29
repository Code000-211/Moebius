//! How the Automations page is drawn: the list, the editor's boxes (the schedule, the chat, the prompt) and the
//! firings.

use std::fmt;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, pick_list, row, space, text, text_input};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{Firing, Snapshot};
use crate::screens::connect;
use crate::theme;
use crate::widgets::form::{box_heading, field, menu_style, mono_editor, notice_line, pick_style};
use crate::widgets::list_editor::{self, Actions, boxed, list_card};
use crate::widgets::page::{self, Tone};

use super::{Automations, Editor, Message, Notice, PRESETS, Place, schedule_name, until};

/// The name of the harness configuration repository in the place picker.
const HARNESS_PLACE: &str = "Harness configuration";

/// A choice of the schedule picker: a preset by its name, or the expression typed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Preset {
    name: &'static str,
    cron: String,
}

impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// A choice of the place picker, with the name it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PlaceChoice(Place, String);

impl fmt::Display for PlaceChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.1)
    }
}

fn message(message: Message) -> AppMessage {
    AppMessage::Automations(message)
}

fn prompt_action(action: iced::widget::text_editor::Action) -> AppMessage {
    message(Message::Prompt(action))
}

impl Automations {
    /// The Automations page: the list, and the editor of the automation chosen.
    pub fn view<'a>(&'a self, snapshot: &'a Snapshot) -> Element<'a, AppMessage> {
        let header = page::header(
            "",
            "Automations",
            None,
            page::context(
                "Chats that start on their own, on a schedule in the server's local time. What they offer lands in the Inbox.",
            ),
            page::white_button("New automation", Some(message(Message::NewAutomation))),
        );
        let chosen = self.editor.as_ref().and_then(|editor| editor.id);
        let cards: Element<_> = if self.list.is_empty() {
            page::boxed(
                "Automations",
                page::body("No automation yet. One triages new issues every morning, say."),
            )
        } else {
            column(self.list.iter().map(|automation| {
                let failed = automation
                    .last_firing
                    .as_ref()
                    .filter(|firing| firing.status == "failed");
                let tag = match (&automation.last_firing, automation.enabled) {
                    (Some(firing), _) if firing.status == "failed" => ("Failed", Tone::Red),
                    (Some(firing), true) if firing.status == "skipped" => ("Skipped", Tone::Amber),
                    (_, true) => ("On", Tone::Teal),
                    (_, false) => ("Off", Tone::Plain),
                };
                let line = match (failed, automation.next_firing) {
                    (Some(firing), _) => firing.reason.clone(),
                    (None, Some(next)) => format!(
                        "{} · next {}",
                        schedule_name(&automation.schedule),
                        until(self.now, next)
                    ),
                    (None, None) => schedule_name(&automation.schedule).to_string(),
                };
                list_card(
                    &automation.name,
                    vec![tag],
                    line,
                    failed.is_some(),
                    chosen == Some(automation.id),
                    Some(message(Message::Select(automation.id))),
                )
            }))
            .spacing(8)
            .padding(iced::Padding::default().right(12))
            .into()
        };
        let detail: Element<_> = match &self.editor {
            Some(editor) => self.editor_view(editor, snapshot),
            None => column![].into(),
        };
        list_editor::layout(header, None, cards, detail)
    }

    fn editor_view<'a>(
        &'a self,
        editor: &'a Editor,
        snapshot: &'a Snapshot,
    ) -> Element<'a, AppMessage> {
        let actions = list_editor::actions(
            &editor.name,
            editor.id.is_some(),
            editor.confirming,
            "Delete automation",
            Actions {
                ask_delete: message(Message::AskDelete),
                keep: message(Message::Keep),
                delete: message(Message::Delete),
                save: Some(message(Message::Save)),
            },
        );
        let actions = if editor.confirming {
            actions
        } else {
            let run_now = editor.id.map(|_| message(Message::RunNow));
            row![page::white_button("Run now", run_now)]
                .push(actions)
                .spacing(10)
                .align_y(Alignment::Center)
        };
        let head = list_editor::head(
            true,
            &editor.name,
            "Name of the automation",
            |name| message(Message::NameEdited(name)),
            actions,
        );
        let mut top = column![head].spacing(12);
        match &editor.notice {
            Some(Notice::Saved) => top = top.push(notice_line("Saved.", false)),
            Some(Notice::Ran(firing)) => {
                let (line, red) = match firing.status.as_str() {
                    "started" => (
                        "Started its chat; it is among the firings below.".to_string(),
                        false,
                    ),
                    _ => (format!("Not started: {}", firing.reason), true),
                };
                top = top.push(notice_line(&line, red));
            }
            Some(Notice::Failed(error)) => top = top.push(notice_line(error, true)),
            None => {}
        }
        column![
            boxed(top),
            self.schedule_box(editor),
            chat_box(editor, &self.agents, snapshot),
            self.firings_box(editor),
        ]
        .spacing(theme::PAGE_GAP)
        .into()
    }

    /// The schedule: the picker of common ones, the cron expression, when it fires next, and whether it is on.
    fn schedule_box<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let mut presets: Vec<Preset> = PRESETS
            .iter()
            .map(|(name, cron)| Preset {
                name,
                cron: cron.to_string(),
            })
            .collect();
        let chosen = presets
            .iter()
            .find(|preset| preset.cron == editor.schedule.trim())
            .cloned()
            .unwrap_or_else(|| Preset {
                name: "Custom",
                cron: editor.schedule.clone(),
            });
        if chosen.name == "Custom" {
            presets.push(chosen.clone());
        }
        let picker = pick_list(presets, Some(chosen), |preset| {
            message(Message::PresetChosen(preset.cron))
        })
        .text_size(theme::BODY)
        .padding([9, 12])
        .width(Length::Fill)
        .style(pick_style)
        .menu_style(menu_style);
        let cron = text_input("0 8 * * 1-5", &editor.schedule)
            .on_input(|schedule| message(Message::ScheduleEdited(schedule)))
            .font(theme::MONO)
            .size(theme::CODE)
            .padding([9, 12])
            .style(connect::input_style);
        let saved = self.saved();
        let next = match saved {
            Some(saved)
                if saved.schedule.trim() != editor.schedule.trim()
                    || saved.enabled != editor.enabled =>
            {
                "Save to see when it fires next.".to_string()
            }
            Some(saved) => match saved.next_firing {
                Some(next) => format!("Fires next {}.", until(self.now, next)),
                None if !saved.enabled => "Off: only \"Run now\" fires it.".to_string(),
                None => "This schedule never comes due.".to_string(),
            },
            None => "Fires once saved, when the schedule comes due.".to_string(),
        };
        let switch = row![
            page::chip(
                "On".to_string(),
                editor.enabled,
                false,
                message(Message::Switched(true))
            ),
            page::chip(
                "Off".to_string(),
                !editor.enabled,
                false,
                message(Message::Switched(false))
            ),
        ]
        .spacing(6);
        let content = column![
            box_heading("Schedule", switch),
            row![
                field("Repeats", picker),
                field("Cron, in the server's local time", cron)
            ]
            .spacing(12),
            text(next).size(theme::CONTEXT).color(theme::TEXT_SECONDARY),
        ]
        .spacing(12);
        boxed(content)
    }

    /// The past firings, newest first; a firing that started a chat opens it.
    fn firings_box<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let content: Element<_> = match &editor.firings {
            None if editor.id.is_some() => page::body("Reading the firings…"),
            Some(firings) if !firings.is_empty() => {
                column(firings.iter().map(|firing| firing_row(firing, self.now)))
                    .spacing(6)
                    .into()
            }
            _ => page::body("It has not fired yet."),
        };
        boxed(
            column![
                box_heading("Firings", page::body("Each opens its chat")),
                content
            ]
            .spacing(12),
        )
    }
}

/// The chat each firing starts: its agent, where it works and its prompt.
fn chat_box<'a>(
    editor: &'a Editor,
    agents: &[String],
    snapshot: &'a Snapshot,
) -> Element<'a, AppMessage> {
    let mut names = agents.to_vec();
    if !names.contains(&editor.agent) {
        names.push(editor.agent.clone());
    }
    let agent = pick_list(names, Some(editor.agent.clone()), |agent| {
        message(Message::AgentChosen(agent))
    })
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let mut places = vec![PlaceChoice(
        Place::Nowhere,
        "An empty directory".to_string(),
    )];
    places.extend(snapshot.boards.iter().map(|board| {
        PlaceChoice(
            Place::Repository(board.repository.id),
            board.repository.name.clone(),
        )
    }));
    places.push(PlaceChoice(Place::Harness, HARNESS_PLACE.to_string()));
    let chosen = places
        .iter()
        .find(|choice| choice.0 == editor.place)
        .cloned();
    let place = pick_list(places, chosen, |choice| {
        message(Message::PlaceChosen(choice.0))
    })
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let content = column![
        box_heading("Chat", page::body("Ends once the agent has answered")),
        row![field("Agent", agent), field("Works in", place)].spacing(12),
        field(
            "Prompt",
            mono_editor(
                &editor.prompt,
                "Triage the issues opened since yesterday: label them and say which are ready.",
                Some(prompt_action),
                140,
            ),
        ),
    ]
    .spacing(12);
    boxed(content)
}

/// A firing: when, what came of it, who fired it, and its chat, which a click opens.
fn firing_row(firing: &Firing, now: i64) -> Element<'_, AppMessage> {
    let tone = match firing.status.as_str() {
        "started" => Tone::Teal,
        "skipped" => Tone::Amber,
        _ => Tone::Red,
    };
    let by = if firing.manual { "Run now" } else { "Schedule" };
    let what = match (firing.run_id, &firing.chat_status) {
        (Some(run), Some(status)) => format!("Chat {run} · {status}"),
        (Some(run), None) => format!("Chat {run}"),
        (None, _) => firing.reason.clone(),
    };
    let line = row![
        text(page::ago(now, firing.at))
            .size(theme::CONTEXT)
            .color(theme::TEXT_SECONDARY)
            .width(90),
        page::tag(firing.status.clone(), tone),
        text(by)
            .size(theme::CONTEXT)
            .color(theme::TEXT_MUTED)
            .width(80),
        text(what)
            .size(theme::CONTEXT)
            .font(theme::manrope(Weight::Medium))
            .wrapping(Wrapping::WordOrGlyph),
        space::horizontal(),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    button(line)
        .width(Length::Fill)
        .padding([8, 12])
        .on_press_maybe(firing.run_id.map(AppMessage::OpenChat))
        .style(|_, status| button::Style {
            background: Some(
                if status == button::Status::Hovered {
                    theme::LANE
                } else {
                    theme::SURFACE
                }
                .into(),
            ),
            text_color: theme::TEXT,
            border: Border {
                color: theme::LINE,
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            ..button::Style::default()
        })
        .into()
}
