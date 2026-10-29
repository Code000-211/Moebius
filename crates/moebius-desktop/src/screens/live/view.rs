//! How the live view is drawn: the header with the run's status and the buttons of the shell and of stopping, the
//! conversation, and the steer field under it.

use iced::widget::{column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{Board, Card};
use crate::screens::connect;
use crate::screens::ports::Scope;
use crate::widgets::conversation::{Entry, Said};
use crate::widgets::page::{self, Tone, tag};
use crate::{theme, widgets::transcript};

use super::{Live, Message};

impl Live {
    /// The page: header, conversation with the steer field, and the panel of the run.
    /// `edit` is the editor of the card's title and body, under the header while it is open.
    pub fn view<'a>(
        &'a self,
        board: &'a Board,
        card: &'a Card,
        edit: Option<Element<'a, AppMessage>>,
    ) -> Element<'a, AppMessage> {
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
            .font(theme::MONO)
            .color(theme::TEXT_SECONDARY)
        ];
        if refused > 0 {
            context = context.push(
                text(" · ")
                    .size(theme::CONTEXT)
                    .font(theme::MONO)
                    .color(theme::TEXT_SECONDARY),
            );
            context = context.push(
                text(count_rules(refused))
                    .size(theme::CONTEXT)
                    .font(theme::MONO)
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
            Some(
                row![chip, crate::screens::history::card_pencil(card)]
                    .spacing(theme::SPACE_8)
                    .align_y(Alignment::Center)
                    .into(),
            ),
            context.into(),
            self.actions(),
        );
        let mut stream = column![
            scrollable(
                container(self.conversation_view())
                    .padding(theme::SPACE_16)
                    .width(Length::Fill)
            )
            .anchor_bottom()
            .height(Length::Fill),
        ]
        .width(Length::Fill);
        if let Some(shell) = &self.shell {
            stream = stream.push(
                container(shell.view(|event| AppMessage::Live(Message::ShellPane(event))))
                    .padding([0.0, theme::SPACE_16])
                    .width(Length::Fill),
            );
        }
        let stream = container(stream.push(self.steer_view()))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| page::box_style());
        let mut page = column![header].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(edit) = edit {
            page = page.push(edit);
        }
        page.push(
            row![stream, self.panel(board, card, refused)]
                .spacing(theme::PAGE_GAP)
                .height(Length::Fill),
        )
        .into()
    }

    /// "Open shell in worktree" and "Stop run" (or the question that confirms it), on the right of the header.
    fn actions(&self) -> Element<'_, AppMessage> {
        let mut actions = row![].spacing(theme::SPACE_10).align_y(Alignment::Center);
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
        if self.has_worktree() {
            actions = actions.push(crate::screens::ports::button(Scope::Run(self.run_id())));
        }
        if self.running() && self.confirming {
            actions = actions
                .push(
                    text("Stop this run?")
                        .size(theme::CONTEXT)
                        .font(theme::MONO)
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
            return page::body("Nothing has happened yet.");
        }
        column(self.conversation.entries().iter().map(entry_view))
            .spacing(theme::SPACE_12)
            .width(Length::Fill)
            .into()
    }

    /// The steer field and its button, under a dashed line, and the short message when something went wrong.
    fn steer_view(&self) -> Element<'_, AppMessage> {
        let running = self.running();
        let can_send = running && !self.steer.trim().is_empty();
        let field = text_input(
            "Steer this run, e.g. also add a test for depth 0",
            &self.steer,
        )
        .on_input_maybe(running.then_some(|text| AppMessage::Live(Message::SteerEdited(text))))
        .on_submit_maybe(can_send.then_some(AppMessage::Live(Message::Steer)))
        .size(theme::BODY)
        .padding(theme::FIELD_PADDING)
        .style(connect::input_style);
        let send = page::primary_button(
            "Steer",
            can_send.then_some(AppMessage::Live(Message::Steer)),
        );
        let hint = match (&self.notice, running) {
            (Some(notice), _) => notice.as_str(),
            (None, true) => "The run keeps going while you type. A steer is saved as a judgement.",
            (None, false) => "Not running, so it cannot be steered.",
        };
        column![
            page::rule(),
            column![
                row![field, send]
                    .spacing(theme::SPACE_10)
                    .align_y(Alignment::Center),
                text(hint)
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
            ]
            .spacing(theme::SPACE_8)
            .padding([theme::SPACE_12, theme::SPACE_16]),
        ]
        .into()
    }
}

pub fn count_rules(n: usize) -> String {
    match n {
        1 => "1 rule broken".to_string(),
        n => format!("{n} rules broken"),
    }
}

pub fn harness_name(harness: &str) -> &str {
    match harness {
        "pi-bolt" => "Pi-Bolt",
        "claude-code" => "Claude Code",
        "" => "an unknown harness",
        other => other,
    }
}

fn entry_view(entry: &Entry) -> Element<'_, AppMessage> {
    match entry {
        Entry::Text(paragraph) => container(
            text(paragraph.as_str())
                .size(theme::BODY)
                .line_height(theme::PROSE_LINE_HEIGHT)
                .color(theme::TEXT),
        )
        .max_width(theme::PROSE_WIDTH)
        .into(),
        Entry::Thinking(thought) => text(thought.as_str())
            .size(theme::CONTEXT)
            .color(theme::TEXT_MUTED)
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
        Entry::McpError { server, message } => transcript::mcp_error(server, message),
    }
}
