//! How the Agents page is drawn: the list, the editor's head with its Save and Delete, and the A1 Bento Workbench
//! (Instructions on the left, Capabilities & Retro on the right).

use std::borrow::Cow;

use iced::widget::{column, row};
use iced::{Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{Agent, Proposal};
use crate::screens::harness_setup::HarnessSetup;
use crate::screens::live::harness_name;
use crate::widgets::form::{committed_line, notice_line};
use crate::widgets::list_editor::{self, Actions, boxed, list_card};
use crate::widgets::model_picker;
use crate::widgets::page::{self, Tone};
use crate::{screens::retro, theme};

use super::boxes::{
    built_in_instructions_box, built_in_skills_box, extensions_box, instructions_box,
    skill_paths_box,
};
use super::editor::Editor;
use super::{Agents, Files, Message, Notice};

fn agent_card(agent: &Agent, chosen: bool, editable: bool) -> Element<'_, AppMessage> {
    let mut tags = Vec::new();
    if agent.built_in {
        tags.push(("Built in", Tone::Plain));
    }
    let line: Cow<str> = match &agent.error {
        Some(error) => {
            tags.push(("Broken", Tone::Red));
            error.into()
        }
        None if agent.harness == "claude-code" => format!(
            "{} · {} · {}",
            agent.model,
            agent.protocol,
            harness_name(&agent.harness)
        )
        .into(),
        None => format!("{} · {}", agent.model, agent.protocol).into(),
    };
    list_card(
        &agent.name,
        tags,
        line,
        agent.error.is_some(),
        chosen,
        editable.then(|| AppMessage::Agents(Message::Select(agent.name.clone()))),
    )
}

impl Agents {
    /// The Agents page: the list, and the A1 Bento editor of the agent chosen.
    pub fn view<'a>(&'a self, harness: &'a HarnessSetup) -> Element<'a, AppMessage> {
        let editable = self.editable();
        let new_agent = editable.then_some(AppMessage::Agents(Message::NewAgent));
        let header = page::header(
            "",
            "Agents",
            None,
            page::context(if self.in_file() {
                "What each run works with: model, instructions, skills, extensions. Saved in moebius.toml."
            } else {
                "What each run works with: model, instructions, skills, MCP servers, extensions. An edit is a commit."
            }),
            page::white_button("New agent", new_agent),
        );
        let mut detail = column![].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(editor) = &self.editor {
            detail = detail.push(self.editor_view(editor));
        } else {
            detail = detail.push(self.proposals_box());
        }
        list_editor::layout(
            header,
            self.in_file().then(|| harness.view()),
            self.list_view(),
            detail,
        )
    }

    /// The Retro's proposals still to be decided whose branch changes the files of the agent in the editor.
    pub(super) fn proposals_box(&self) -> Element<'_, AppMessage> {
        let name = self.editor.as_ref().map(|editor| editor.name.as_str());
        let open: Vec<&Proposal> = self
            .proposals
            .iter()
            .filter(|p| {
                matches!(
                    p.status.as_str(),
                    "open" | "trying" | "positive" | "untried"
                ) && name.is_some_and(|name| p.agents.iter().any(|a| a == name))
            })
            .collect();
        if open.is_empty() {
            return page::boxed(
                "From the Retro",
                page::body("No proposal waiting for a decision changes this agent."),
            );
        }
        page::boxed(
            "From the Retro",
            column(open.into_iter().map(retro::proposal)).spacing(theme::SPACE_12),
        )
    }

    pub(super) fn list_view(&self) -> Element<'_, AppMessage> {
        if self.list.is_empty() {
            return page::boxed("Agents", page::body("No agent yet."));
        }
        let chosen = self.editor.as_ref().map(|editor| editor.name.as_str());
        column(self.list.iter().map(|agent| {
            agent_card(
                agent,
                self.editable() && chosen == Some(agent.name.as_str()),
                self.editable(),
            )
        }))
        .spacing(theme::SPACE_8)
        .padding(iced::Padding::default().right(theme::SCROLL_GUTTER))
        .into()
    }

    pub(super) fn editor_view<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let message = |message| AppMessage::Agents(message);
        let ready = matches!(editor.files, Files::Ready);
        let actions = list_editor::actions(
            &editor.name,
            editor.exists,
            editor.confirming,
            "Delete agent",
            Actions {
                ask_delete: message(Message::AskDelete),
                keep: message(Message::KeepAgent),
                delete: message(Message::Delete),
                save: ready.then_some(message(Message::Save)),
            },
        );
        let head = list_editor::head(
            editor.naming,
            &editor.name,
            "Name of the new agent",
            move |name| message(Message::NameEdited(name)),
            actions,
        );
        let mut content = column![head].spacing(theme::SPACE_12);
        match &editor.files {
            Files::Reading => content = content.push(page::body("Reading the agent…")),
            Files::Broken(error) => {
                content = content
                    .push(notice_line(error, true))
                    .push(page::body(
                        "Its files cannot be read, so it cannot run. Fix them in the repository or delete the agent.",
                    ));
                return self.with_boxes(editor, content, false);
            }
            Files::Ready => {}
        }
        if ready {
            content = content.push(self.settings_row(editor));
            if let Some(available) = &self.available {
                content = content.push(model_picker::note(
                    available,
                    AppMessage::Agents(Message::RefreshAvailable),
                ));
            }
        }
        if let Some(notice) = &editor.notice {
            content = content.push(match notice {
                Notice::Saved(commit) => committed_line(commit.as_deref()),
                Notice::Written => notice_line("Saved in moebius.toml.", false),
                Notice::Failed(error) => notice_line(error, true),
            });
        }
        self.with_boxes(editor, content, ready)
    }

    /// A1 Bento layout: Top settings box, and below it a 2-column Bento grid (Left: Instructions, Right: Capabilities & Retro).
    fn with_boxes<'a>(
        &'a self,
        editor: &'a Editor,
        head: iced::widget::Column<'a, AppMessage>,
        ready: bool,
    ) -> Element<'a, AppMessage> {
        let mut boxes = column![boxed(head)].spacing(theme::PAGE_GAP);
        if ready {
            let mut left_col = column![]
                .spacing(theme::PAGE_GAP)
                .width(Length::FillPortion(3));
            if let Some(built_in) = &editor.built_in {
                left_col = left_col
                    .push(built_in_instructions_box(built_in))
                    .push(built_in_skills_box(built_in));
            }
            left_col = left_col
                .push(instructions_box(editor))
                .push(self.proposals_box());

            let right_col = if self.in_file() {
                column![skill_paths_box(editor), extensions_box(editor)]
                    .spacing(theme::PAGE_GAP)
                    .width(Length::FillPortion(2))
            } else {
                column![
                    self.skills_box(editor),
                    self.mcp_box(editor),
                    extensions_box(editor),
                ]
                .spacing(theme::PAGE_GAP)
                .width(Length::FillPortion(2))
            };

            boxes = boxes.push(row![left_col, right_col].spacing(theme::PAGE_GAP));
        } else {
            boxes = boxes.push(self.proposals_box());
        }
        boxes.into()
    }
}
