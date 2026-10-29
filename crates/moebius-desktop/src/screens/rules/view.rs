//! How the Rules page is drawn: the built-in patterns with their refusals, the server's rules in prose with the
//! request to the Moebius agent, and the rules of each repository, each box with what came of its last save.

use iced::widget::text::Wrapping;
use iced::widget::{Column, column, row, space, text, text_editor};
use iced::{Alignment, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{RepositoryRules, RulesView};
use crate::theme;
use crate::widgets::form::{aside, box_heading, committed_line, notice_line};
use crate::widgets::inline_edit::editor_style;
use crate::widgets::list_editor;
use crate::widgets::page::{self, Tone};

use super::{Message, Notice, Place, Rules};

impl Rules {
    /// The Rules page.
    pub fn view(&self) -> Element<'_, AppMessage> {
        let header = page::header(
            "Build",
            "Rules",
            None,
            page::context(
                "What a run must not do. Every tool call is checked against these first; a refusal names the rule.",
            ),
            space().into(),
        );
        let mut boxes = column![header].spacing(theme::PAGE_GAP).width(Length::Fill);
        match &self.view {
            None => boxes = boxes.push(page::boxed("Rules", page::body("Reading the rules…"))),
            Some(view) => {
                boxes = boxes
                    .push(self.patterns_box(view))
                    .push(self.prose_box(view));
                for repository in &view.repositories {
                    boxes = boxes.push(self.repository_box(repository, view));
                }
            }
        }
        iced::widget::scrollable(
            boxes.padding(iced::Padding::default().right(theme::SCROLL_GUTTER)),
        )
        .height(Length::Fill)
        .into()
    }

    fn patterns_box<'a>(&'a self, view: &'a RulesView) -> Element<'a, AppMessage> {
        let rows = view.patterns.iter().map(|pattern| {
            row![
                column![
                    text(pattern.name.as_str())
                        .size(theme::BODY)
                        .font(theme::MONO),
                    text(pattern.description.as_str())
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY)
                        .wrapping(Wrapping::Word),
                ]
                .spacing(theme::SPACE_4)
                .width(Length::Fill),
                refusals_tag(view, &pattern.name),
            ]
            .spacing(theme::SPACE_12)
            .align_y(Alignment::Center)
            .into()
        });
        boxed(
            "Built-in patterns",
            aside("Always on · checked first, no model asked"),
            column(rows).spacing(theme::SPACE_12).into(),
        )
    }

    fn prose_box<'a>(&'a self, view: &'a RulesView) -> Element<'a, AppMessage> {
        let prose = &view.prose;
        let where_from = match prose.source.as_deref() {
            Some("harness") => "rules.md of the harness configuration repository",
            Some(_) => "the rules_file of moebius.toml",
            None => "No rules file",
        };
        let mut heading_side = row![aside(where_from)]
            .spacing(theme::SPACE_12)
            .align_y(Alignment::Center);
        if prose.editable && !self.by_agent {
            heading_side = heading_side.push(page::white_button(
                "Let the Moebius agent write them",
                Some(AppMessage::Rules(Message::ByAgent(true))),
            ));
        }
        let mut content = column![].spacing(theme::SPACE_12);
        if prose.editable && self.by_agent {
            content = content.push(self.wish_view());
        }
        if let Some(error) = &prose.error {
            content = content.push(notice_line(error, true));
        }
        let place = prose.editable.then_some(Place::Server);
        content = content.push(self.rule_cards(
            place,
            &prose.rules,
            view,
            "No rule in prose. Only the built-in patterns apply.",
        ));
        content = self.with_notice(content, Place::Server);
        if !prose.editable {
            content = content.push(page::body("Read only: this server has no rules file."));
        }
        boxed("Rules in prose", heading_side.into(), content.into())
    }

    fn repository_box<'a>(
        &'a self,
        repository: &'a RepositoryRules,
        view: &'a RulesView,
    ) -> Element<'a, AppMessage> {
        let place = Place::Repository(repository.repository_id);
        let mut content = column![].spacing(theme::SPACE_12);
        if let Some(error) = &repository.error {
            content = content.push(notice_line(error, true));
        }
        content = content.push(self.rule_cards(
            Some(place),
            &repository.rules,
            view,
            "No rule of its own.",
        ));
        content = self.with_notice(content, place);
        content = content.push(page::body(
            "Saved as a commit on the default branch (.moebius/rules.md).",
        ));
        boxed(
            &repository.repository,
            aside("Added to the server's rules; cannot weaken them"),
            content.into(),
        )
    }

    /// `content` with what came of the last save of the file of `place`.
    fn with_notice<'a>(
        &'a self,
        content: Column<'a, AppMessage>,
        place: Place,
    ) -> Column<'a, AppMessage> {
        match &self.notice {
            Some((at, Notice::Saved(Some(commit)))) if *at == place => {
                content.push(committed_line(Some(commit)))
            }
            Some((at, Notice::Saved(None))) if *at == place => {
                content.push(notice_line("Saved.", false))
            }
            Some((at, Notice::Failed(error))) if *at == place => {
                content.push(notice_line(error, true))
            }
            _ => content,
        }
    }

    /// The request to the Moebius agent: what should be allowed or forbidden, "Ask the agent" and Cancel.
    fn wish_view(&self) -> Element<'_, AppMessage> {
        let message = AppMessage::Rules;
        let ask = (!self.asking && !self.wish.text().trim().is_empty())
            .then_some(message(Message::AskAgent));
        let wish = text_editor(&self.wish)
            .placeholder(
                "What should be allowed or forbidden. The Moebius agent changes the rules and saves them.",
            )
            .on_action(move |action| message(Message::WishEdited(action)))
            .size(theme::BODY)
            .padding(theme::EDITOR_PADDING)
            .height(theme::EDITOR_SHORT)
            .style(editor_style);
        column![
            wish,
            row![
                page::primary_button("Ask the agent", ask),
                page::white_button("Cancel", Some(message(Message::ByAgent(false)))),
            ]
            .spacing(theme::SPACE_8),
        ]
        .spacing(theme::SPACE_12)
        .into()
    }
}

/// How often the rule refused a call in the last 7 days; red when it did.
pub fn refusals_tag<'a>(view: &RulesView, name: &str) -> Element<'a, AppMessage> {
    match view.refusals.get(name).copied().unwrap_or(0) {
        0 => page::tag("No refusals this week".to_string(), Tone::Plain),
        1 => page::tag("1 refusal this week".to_string(), Tone::Red),
        n => page::tag(format!("{n} refusals this week"), Tone::Red),
    }
}

/// A box with a heading, what stands on its right and its content.
fn boxed<'a>(
    heading: &'a str,
    aside: Element<'a, AppMessage>,
    content: Element<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    list_editor::boxed(column![box_heading(heading, aside), content].spacing(theme::SPACE_12))
}
