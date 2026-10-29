//! How the Ports page is drawn: a list with one row for each port of the server that is open or forwarded, and the offer
//! to open a port that stands over a page.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, space, text, text_input};
use iced::{Alignment, Element, Length, Padding};

use crate::app::{Message as AppMessage, Page};
use crate::client::Port;
use crate::screens::connect;
use crate::theme;
use crate::widgets::form::notice_line;
use crate::widgets::page::{self, Tone};

use super::{Forward, Message, Ports, State};

/// What a port belongs to, in a few words.
pub fn owner(port: &Port) -> String {
    match (port.kind.as_str(), port.run) {
        ("run", Some(run)) => format!("run {run}"),
        ("chat", Some(run)) => format!("chat {run}"),
        ("terminal", Some(run)) => format!("terminal of run {run}"),
        ("terminal", None) => "server terminal".to_string(),
        (kind, _) => kind.to_string(),
    }
}

/// One row of the list: a port of the server, open, forwarded, or both.
pub(super) struct Row<'a> {
    pub(super) remote: u16,
    port: Option<&'a Port>,
    forward: Option<&'a Forward>,
}

fn ports_message(message: Message) -> AppMessage {
    AppMessage::Ports(message)
}

impl Ports {
    /// The rows to show: every open port and every forward, by port, narrowed to the scope.
    pub(super) fn rows(&self) -> Vec<Row<'_>> {
        let mut remotes: Vec<u16> = self
            .detected
            .iter()
            .map(|port| port.port)
            .chain(self.forwards.iter().map(|forward| forward.remote))
            .collect();
        remotes.sort_unstable();
        remotes.dedup();
        remotes
            .into_iter()
            .map(|remote| Row {
                remote,
                port: self.detected.iter().find(|port| port.port == remote),
                forward: self
                    .forwards
                    .iter()
                    .find(|forward| forward.remote == remote),
            })
            .filter(|row| match (self.scope, row.port) {
                (Some(scope), Some(port)) => scope.holds(port),
                (Some(_), None) => false,
                (None, _) => true,
            })
            .collect()
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        let rows = self.rows();
        let forwarded = rows.iter().filter(|row| row.forward.is_some()).count();
        let header = page::header(
            "Setup",
            Page::Ports.title(),
            None,
            page::context(format!("{} open, {forwarded} forwarded", rows.len())),
            page::white_button(
                "Forward a port\u{2026}",
                Some(ports_message(Message::OpenForm)),
            ),
        );
        let mut content = column![header]
            .spacing(theme::PAGE_GAP)
            .width(Length::Fill)
            .height(Length::Fill);
        if let Some(scope) = self.scope {
            content = content.push(
                row![
                    page::context(format!("Ports of {}", scope.label())),
                    page::white_button("Show all", Some(ports_message(Message::ShowAll))),
                ]
                .spacing(theme::SPACE_10)
                .align_y(Alignment::Center),
            );
        }
        if let Some(typed) = &self.form {
            content = content.push(self.form_view(typed));
        }
        if self.form.is_none()
            && let Some(note) = &self.note
        {
            content = content.push(notice_line(note, true));
        }
        let list: Element<'_, AppMessage> = if rows.is_empty() {
            page::body("No ports are open.")
        } else {
            let mut list = column![].width(Length::Fill);
            for (index, row) in rows.iter().enumerate() {
                if index > 0 {
                    list = list.push(page::rule());
                }
                list = list.push(self.row_view(row));
            }
            scrollable(container(list).padding(Padding::default().right(theme::SCROLL_GUTTER)))
                .height(Length::Shrink)
                .into()
        };
        content = content.push(
            container(list)
                .padding(theme::BOX_PADDING)
                .width(Length::Fill)
                .style(|_| page::box_style()),
        );
        content = content.push(
            row![
                page::chip(
                    "Restore forwarded ports when reconnecting".to_string(),
                    self.restore,
                    false,
                    ports_message(Message::ToggleRestore),
                ),
                page::context(if self.restore { "On" } else { "Off" }),
            ]
            .spacing(theme::SPACE_10)
            .align_y(Alignment::Center),
        );
        content.into()
    }

    /// The field that asks for a port of the server to forward.
    fn form_view<'a>(&'a self, typed: &'a str) -> Element<'a, AppMessage> {
        let valid = typed.trim().parse::<u16>().is_ok_and(|port| port != 0);
        let mut form = column![
            row![
                text_input("Port on the server, e.g. 3000", typed)
                    .on_input(|typed| ports_message(Message::FormEdited(typed)))
                    .on_submit(ports_message(Message::FormSubmit))
                    .font(theme::MONO)
                    .size(theme::CODE)
                    .padding(theme::FIELD_PADDING)
                    .width(theme::FIELD_WIDTH)
                    .style(connect::input_style),
                page::primary_button("Forward", valid.then(|| ports_message(Message::FormSubmit))),
                page::white_button("Cancel", Some(ports_message(Message::FormCancel))),
            ]
            .spacing(theme::SPACE_8)
            .align_y(Alignment::Center),
        ]
        .spacing(theme::SPACE_6);
        if let Some(note) = &self.note {
            form = form.push(notice_line(note, true));
        }
        form.into()
    }

    fn row_view<'a>(&'a self, row_: &Row<'a>) -> Element<'a, AppMessage> {
        let remote = row_.remote;
        let belongs = match row_.port {
            Some(port) if port.process.is_empty() => owner(port),
            Some(port) => format!("{} \u{b7} {}", port.process, owner(port)),
            None => "by hand".to_string(),
        };
        let port = column![
            text(remote.to_string())
                .size(theme::BOX_HEADING)
                .font(theme::MONO),
            text(belongs)
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED)
                .wrapping(Wrapping::WordOrGlyph),
        ]
        .spacing(theme::SPACE_2)
        .width(Length::FillPortion(2));
        let (address, actions) = match row_.forward {
            Some(forward) => (self.address_view(forward), self.actions_view(forward)),
            None => (
                text("Not forwarded")
                    .size(theme::CONTEXT)
                    .color(theme::TEXT_MUTED)
                    .into(),
                page::white_button("Forward", Some(ports_message(Message::Start(remote)))),
            ),
        };
        row![
            port,
            container(address).width(Length::FillPortion(3)),
            actions
        ]
        .spacing(theme::PAGE_GAP)
        .align_y(Alignment::Center)
        .padding(
            Padding::default()
                .top(theme::SPACE_8)
                .bottom(theme::SPACE_8),
        )
        .into()
    }

    /// The local address of a forward and how it stands; the field for another local port while it is being changed.
    fn address_view<'a>(&'a self, forward: &'a Forward) -> Element<'a, AppMessage> {
        if let Some((remote, typed)) = &self.editing
            && *remote == forward.remote
        {
            let mut edit = column![
                row![
                    text("127.0.0.1:")
                        .size(theme::CODE)
                        .font(theme::MONO)
                        .color(theme::TEXT_SECONDARY),
                    text_input("Local port", typed)
                        .on_input(|typed| ports_message(Message::LocalEdited(typed)))
                        .on_submit(ports_message(Message::LocalSubmit))
                        .font(theme::MONO)
                        .size(theme::CODE)
                        .padding(theme::FIELD_PADDING)
                        .width(Length::Fill)
                        .style(connect::input_style),
                    page::primary_button("Save", Some(ports_message(Message::LocalSubmit))),
                    page::white_button("Cancel", Some(ports_message(Message::LocalCancel))),
                ]
                .spacing(theme::SPACE_8)
                .align_y(Alignment::Center),
            ]
            .spacing(theme::SPACE_4);
            if let Some(note) = &self.note {
                edit = edit.push(notice_line(note, true));
            }
            return edit.into();
        }
        let (label, tone) = match &forward.state {
            State::Starting => ("starting", Tone::Plain),
            State::Listening => ("forwarded", Tone::Teal),
            State::Failed(_) => ("failed", Tone::Red),
        };
        let mut cell = column![
            row![
                text(format!("127.0.0.1:{}", forward.local))
                    .size(theme::CODE)
                    .font(theme::MONO),
                page::tag(label.to_string(), tone),
            ]
            .spacing(theme::SPACE_8)
            .align_y(Alignment::Center),
        ]
        .spacing(theme::SPACE_2);
        let problem = match &forward.state {
            State::Failed(reason) => Some(reason.as_str()),
            _ => forward.refused.as_deref(),
        };
        if let Some(problem) = problem {
            cell = cell.push(
                text(problem)
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::RED_TEXT)
                    .wrapping(Wrapping::WordOrGlyph),
            );
        }
        cell.into()
    }

    fn actions_view<'a>(&'a self, forward: &Forward) -> Element<'a, AppMessage> {
        let remote = forward.remote;
        let listening = forward.state == State::Listening;
        row![
            page::white_button(
                "Open in browser",
                listening.then(|| ports_message(Message::Open(remote)))
            ),
            page::white_button(
                "Copy address",
                listening.then(|| ports_message(Message::Copy(remote)))
            ),
            page::white_button(
                "Change local port",
                Some(ports_message(Message::Change(remote)))
            ),
            page::white_button(
                "Stop forwarding",
                Some(ports_message(Message::Stop(remote)))
            ),
        ]
        .spacing(theme::SPACE_6)
        .into()
    }
}

/// The offer to open the port that opened last in what is being watched, in a line over the page.
pub fn offer_banner(ports: &Ports) -> Option<Element<'_, AppMessage>> {
    let port = ports.offer()?;
    let address = ports
        .address(port.port)
        .unwrap_or_else(|| format!("127.0.0.1:{}", port.port));
    Some(
        container(
            row![
                text(format!("Port {} opened in {}", port.port, owner(port)))
                    .size(theme::CONTEXT)
                    .font(theme::manrope(Weight::Semibold)),
                text(address)
                    .size(theme::CONTEXT)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
                space::horizontal(),
                page::primary_button("Open in browser", Some(ports_message(Message::OpenOffer))),
                page::white_button("Show ports", Some(AppMessage::Visit(Page::Ports))),
                page::white_button("Dismiss", Some(ports_message(Message::DismissOffer))),
            ]
            .spacing(theme::SPACE_8)
            .align_y(Alignment::Center),
        )
        .padding(theme::CARD_PADDING)
        .width(Length::Fill)
        .style(|_| page::box_style())
        .into(),
    )
}

/// The id of the button of a page that opens the Ports page, which the sidebar's entry shares its label with.
pub const OPEN_PORTS: &str = "open-ports";

/// The button of a page that opens the Ports page on the ports of `scope`.
pub fn button<'a>(scope: super::Scope) -> Element<'a, AppMessage> {
    container(page::white_button(
        "Ports",
        Some(AppMessage::OpenPorts(Some(scope))),
    ))
    .id(OPEN_PORTS)
    .into()
}
