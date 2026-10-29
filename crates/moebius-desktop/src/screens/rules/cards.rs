//! The rules of a file as cards: each with its name, text, refusals and a pencil, the "New rule" card under them, and
//! the card being edited grown into the editor of its name and text, with Save, Cancel and Delete.

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text, text_editor, text_input};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{Rule, RulesView};
use crate::widgets::inline_edit::{self, editor_style};
use crate::widgets::page;
use crate::{screens::connect, theme};

use super::file::valid_name;
use super::view::refusals_tag;
use super::{Message, Place, Rules};

impl Rules {
    /// The rules of a file as cards, each with a pencil when the file of `place` can be edited, and then the "New
    /// rule" card; the card being edited is its editor.
    pub(super) fn rule_cards<'a>(
        &'a self,
        place: Option<Place>,
        rules: &'a [Rule],
        view: &'a RulesView,
        empty: &'a str,
    ) -> Element<'a, AppMessage> {
        let mut cards = column![].spacing(theme::SPACE_12);
        if rules.is_empty() && place.is_none() {
            return page::body(empty);
        }
        for rule in rules {
            if let Some(editor) = place.and_then(|place| self.editor_card(place, Some(&rule.name)))
            {
                cards = cards.push(editor);
                continue;
            }
            let mut name = row![text(rule.name.as_str()).size(theme::BODY).font(theme::MONO)]
                .spacing(theme::SPACE_6)
                .align_y(Alignment::Center);
            if let Some(place) = place {
                name = name.push(inline_edit::pencil(
                    pencil_id(place, &rule.name),
                    AppMessage::Rules(Message::EditRule(place, Some(rule.name.clone()))),
                ));
            }
            let content = row![
                column![
                    name,
                    text(rule.text.as_str())
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY)
                        .wrapping(Wrapping::Word),
                ]
                .spacing(theme::SPACE_4)
                .width(Length::Fill),
                refusals_tag(view, &rule.name),
            ]
            .spacing(theme::SPACE_12)
            .align_y(Alignment::Center);
            cards = cards.push(card(content.into(), CardLine::Plain));
        }
        let Some(place) = place else {
            return cards.into();
        };
        if let Some(editor) = self.editor_card(place, None) {
            return cards.push(editor).into();
        }
        let id = match place {
            Place::Server => "new-rule".to_string(),
            Place::Repository(repository) => format!("new-rule-{repository}"),
        };
        let new = row![
            text(if rules.is_empty() {
                format!("{empty} Add a rule")
            } else {
                "Add a rule".to_string()
            })
            .size(theme::BODY)
            .color(theme::TEXT_MUTED),
            inline_edit::pencil(id, AppMessage::Rules(Message::EditRule(place, None))),
        ]
        .spacing(theme::SPACE_6)
        .align_y(Alignment::Center);
        cards.push(card(new.into(), CardLine::Muted)).into()
    }

    /// The card of the rule `rule` (a new rule without one) of `place` grown into its editor, if that is the one
    /// being edited.
    fn editor_card(&self, place: Place, rule: Option<&str>) -> Option<Element<'_, AppMessage>> {
        let editing = self
            .editing
            .as_ref()
            .filter(|e| e.place == place && e.rule.as_deref() == rule)?;
        let message = AppMessage::Rules;
        let fields = column![
            text_input("name-without-spaces", &editing.name)
                .on_input(move |name| message(Message::NameEdited(name)))
                .font(theme::MONO)
                .size(theme::BODY)
                .padding(theme::CONTROL_PADDING)
                .style(connect::input_style),
            text_editor(&editing.text)
                .placeholder("What a run must not do.")
                .on_action(move |action| message(Message::TextEdited(action)))
                .size(theme::BODY)
                .padding(theme::EDITOR_PADDING)
                .height(theme::EDITOR_SHORT)
                .style(editor_style),
        ]
        .spacing(theme::SPACE_12);
        let save =
            (!editing.saving && valid_name(&editing.name)).then_some(message(Message::SaveRule));
        let delete: Option<Element<'_, AppMessage>> = editing.rule.as_ref().map(|_| {
            if editing.deleting {
                row![
                    text("Delete this rule?")
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY),
                    page::white_button("Keep it", Some(message(Message::KeepRule))),
                    page::white_button(
                        "Delete",
                        (!editing.saving).then_some(message(Message::DeleteRule))
                    ),
                ]
                .spacing(theme::SPACE_8)
                .align_y(Alignment::Center)
                .into()
            } else {
                page::white_button("Delete rule", Some(message(Message::AskDelete)))
            }
        });
        let editor = inline_edit::editor_with(
            fields.into(),
            save,
            message(Message::CancelRule),
            delete,
            editing.error.as_deref(),
        );
        Some(card(editor, CardLine::Editing))
    }
}

/// The id of the pencil of a rule, for a test to find it.
fn pencil_id(place: Place, rule: &str) -> String {
    match place {
        Place::Server => format!("edit-rule-{rule}"),
        Place::Repository(repository) => format!("edit-rule-{repository}-{rule}"),
    }
}

/// The line around a rule's card: a rule, the empty "New rule" card, or a card grown into its editor.
enum CardLine {
    Plain,
    Muted,
    Editing,
}

fn card(content: Element<'_, AppMessage>, line: CardLine) -> Element<'_, AppMessage> {
    let (color, width, fill) = match line {
        CardLine::Plain => (theme::LINE, theme::HAIRLINE, theme::SURFACE),
        CardLine::Muted => (theme::LINE, theme::HAIRLINE, theme::LANE),
        CardLine::Editing => (theme::TEXT, theme::BORDER_FOCUS, theme::SURFACE),
    };
    container(content)
        .padding([theme::SPACE_12, theme::BOX_PADDING])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(fill.into()),
            border: Border {
                color,
                width,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
        .into()
}
