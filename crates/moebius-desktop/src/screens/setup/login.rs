//! The OAuth login of an upstream on the Models page: the warning about the provider's terms, which the person
//! confirms before a login starts (`POST /v1/models/upstreams/{place}/login`), then the code to enter at the provider
//! or the provider's page opened in the browser, then whether the login holds, read again every second
//! (`GET /v1/models/upstreams/{place}/login`) while it waits. A login by the provider's page also takes the code, or
//! the address the browser ended on, pasted into a field (`POST /v1/models/upstreams/{place}/login/code`), for a
//! browser that cannot reach the server.

use std::time::Duration;

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Login, UpstreamOAuth};
use crate::screens::connect;
use crate::screens::live::say;
use crate::settings::Settings;
use crate::theme;
use crate::widgets::form::{labelled, notice_line};
use crate::widgets::page;

use super::{Message, Setup};

/// How long the page waits before it asks again how a login stands.
const POLL: Duration = Duration::from_secs(1);

/// Where the login of one upstream stands on the page.
#[derive(Debug, Clone, PartialEq)]
pub enum LoginStep {
    /// "Log in" was pressed: the warning waits for the person's confirmation.
    Confirming,
    /// The server was asked to start the login.
    Starting,
    /// What the server last said.
    Answered(Login),
    /// The server could not be asked, or refused.
    Failed(String),
}

/// What was pasted to finish a login, and why the server did not take it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Paste {
    pub text: String,
    pub problem: Option<String>,
}

/// Asks how the login of the upstream at `place` stands, after `delay`.
fn ask(settings: &Settings, place: usize, delay: Duration) -> Task<AppMessage> {
    let client = Client::new(settings.clone());
    Task::perform(
        async move {
            tokio::time::sleep(delay).await;
            client.login(place).await
        },
        move |result| AppMessage::Setup(Message::LoginAnswer(place, result)),
    )
}

impl Setup {
    /// The change a message of a login makes, and what it asks of the server.
    pub(super) fn update_login(
        &mut self,
        message: Message,
        settings: Option<&Settings>,
    ) -> Task<AppMessage> {
        match message {
            Message::LogIn(place) => {
                self.logins.insert(place, LoginStep::Confirming);
            }
            Message::CancelLogIn(place) => {
                self.logins.remove(&place);
                self.pastes.remove(&place);
            }
            Message::ConfirmLogIn(place) => {
                let Some(settings) = settings else {
                    return Task::none();
                };
                self.pastes.remove(&place);
                self.logins.insert(place, LoginStep::Starting);
                let client = Client::new(settings.clone());
                return Task::perform(
                    async move { client.start_login(place).await },
                    move |result| AppMessage::Setup(Message::LoginStarted(place, result)),
                );
            }
            Message::LoginStarted(place, Ok(login)) => {
                if let Some(url) = &login.authorize_url {
                    crate::widgets::browser::open(url);
                }
                return self.answered(place, Ok(login), settings);
            }
            Message::LoginStarted(place, Err(e)) | Message::LoginAnswer(place, Err(e)) => {
                return self.answered(place, Err(e), settings);
            }
            Message::LoginAnswer(place, Ok(login)) => {
                // An answer for a login the person cancelled is dropped.
                if self.logins.contains_key(&place) {
                    return self.answered(place, Ok(login), settings);
                }
            }
            Message::PasteEdit(place, text) => {
                let paste = self.pastes.entry(place).or_default();
                paste.text = text;
                paste.problem = None;
            }
            Message::FinishLogIn(place) => {
                let (Some(settings), Some(paste)) = (settings, self.pastes.get(&place)) else {
                    return Task::none();
                };
                if paste.text.trim().is_empty() {
                    return Task::none();
                }
                let (client, pasted) = (Client::new(settings.clone()), paste.text.clone());
                return Task::perform(
                    async move { client.paste_login(place, &pasted).await },
                    move |result| AppMessage::Setup(Message::LoginPasted(place, result)),
                );
            }
            Message::LoginPasted(place, Ok(login)) => {
                self.pastes.remove(&place);
                return self.answered(place, Ok(login), settings);
            }
            Message::LoginPasted(place, Err(e)) => {
                self.pastes.entry(place).or_default().problem = Some(say(&e));
            }
            Message::OpenPage(url) => crate::widgets::browser::open(&url),
            _ => {}
        }
        Task::none()
    }

    /// Keeps what the server said of a login; asks again while it waits, and reads the models once it ends.
    fn answered(
        &mut self,
        place: usize,
        result: Result<Login, ClientError>,
        settings: Option<&Settings>,
    ) -> Task<AppMessage> {
        let login = match result {
            Ok(login) => login,
            Err(e) => {
                self.logins.insert(place, LoginStep::Failed(say(&e)));
                return Task::none();
            }
        };
        let waiting = login.state == "waiting";
        self.logins.insert(place, LoginStep::Answered(login));
        let Some(settings) = settings else {
            return Task::none();
        };
        if waiting {
            return ask(settings, place, POLL);
        }
        // An upstream that logged in lists its models now, not when the kept list runs out.
        let client = Client::new(settings.clone());
        Task::batch([
            Task::perform(async move { client.models().await }, |result| {
                AppMessage::Setup(Message::Models(result))
            }),
            self.ask_available(settings, true),
        ])
    }
}

/// The amber box with the warning about the provider's terms.
fn warning<'a>(line: &'a str) -> Element<'a, AppMessage> {
    container(
        text(line)
            .size(theme::BODY)
            .color(theme::AMBER_TEXT)
            .wrapping(Wrapping::WordOrGlyph),
    )
    .padding([theme::SPACE_12, theme::BOX_PADDING])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(theme::AMBER_FILL.into()),
        border: iced::Border {
            color: theme::AMBER_BORDER,
            width: theme::HAIRLINE,
            radius: theme::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// Under the row of an upstream that logs in with OAuth (`place` in the file): the warning, whether it is logged in,
/// and the login as far as it has come.
pub(super) fn login_view<'a>(
    place: usize,
    oauth: &'a UpstreamOAuth,
    step: Option<&'a LoginStep>,
    paste: Option<&'a Paste>,
) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Setup(message);
    let how = match oauth.flow.as_str() {
        "device" => "Logs in with OAuth: a code you enter at the provider.",
        _ => "Logs in with OAuth: the provider's page in your browser.",
    };
    let state = if oauth.logged_in {
        "A login is kept."
    } else {
        "Not logged in."
    };
    let label = if oauth.logged_in {
        "Log in again"
    } else {
        "Log in"
    };
    let pressable = !matches!(step, Some(LoginStep::Confirming | LoginStep::Starting));
    let head = row![
        text(format!("{how} {state}"))
            .size(theme::BODY)
            .color(theme::TEXT_SECONDARY)
            .width(Length::Fill),
        page::white_button(label, pressable.then(|| message(Message::LogIn(place)))),
    ]
    .spacing(theme::SPACE_12)
    .align_y(Alignment::Center);
    let mut content = column![head, warning(&oauth.warning)].spacing(theme::SPACE_12);
    if let Some(needs) = &oauth.needs {
        content = content.push(notice_line(
            &format!("Requests to this upstream fail: they need {needs}, which is not served yet."),
            true,
        ));
    }
    match step {
        None => {}
        Some(LoginStep::Confirming) => {
            content = content.push(
                row![
                    page::primary_button(
                        "I understand, log in",
                        Some(message(Message::ConfirmLogIn(place)))
                    ),
                    page::white_button("Cancel", Some(message(Message::CancelLogIn(place)))),
                ]
                .spacing(theme::SPACE_12),
            );
        }
        Some(LoginStep::Starting) => content = content.push(page::body("Starting the login…")),
        Some(LoginStep::Failed(problem)) => content = content.push(notice_line(problem, true)),
        Some(LoginStep::Answered(login)) => content = content.push(answer(place, login, paste)),
    }
    container(content).width(Length::Fill).into()
}

/// The field for the code, or the address the browser ended on, with the button that finishes the login.
fn paste_view<'a>(place: usize, paste: Option<&'a Paste>) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Setup(message);
    let typed = paste.map_or("", |paste| paste.text.as_str());
    let input = text_input("Code, or http://localhost…/callback?code=…", typed)
        .on_input(move |text| message(Message::PasteEdit(place, text)))
        .on_submit(message(Message::FinishLogIn(place)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding(theme::CONTROL_PADDING)
        .style(connect::input_style);
    let mut paste_box = column![
        labelled("Paste the code or the address you ended on", input),
        page::body(
            "If the browser cannot reach this machine, copy the address from its address bar after you log in."
        ),
        page::primary_button(
            "Finish login",
            (!typed.trim().is_empty()).then(|| message(Message::FinishLogIn(place)))
        ),
    ]
    .spacing(theme::SPACE_8);
    if let Some(problem) = paste.and_then(|paste| paste.problem.as_deref()) {
        paste_box = paste_box.push(notice_line(problem, true));
    }
    paste_box.into()
}

/// What the server said of a login: the code and the page while it waits, then how it ended.
fn answer<'a>(place: usize, login: &'a Login, paste: Option<&'a Paste>) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Setup(message);
    match login.state.as_str() {
        "waiting" => {
            let mut waiting = column![].spacing(theme::SPACE_8);
            if let Some(code) = &login.user_code {
                let page_url = login
                    .verification_uri_complete
                    .clone()
                    .or_else(|| login.verification_uri.clone())
                    .unwrap_or_default();
                waiting = waiting.push(
                    row![
                        text(code)
                            .size(theme::PAGE_TITLE)
                            .font(theme::MONO)
                            .color(theme::TEXT),
                        text(format!(
                            "Enter this code at {}",
                            login.verification_uri.as_deref().unwrap_or_default()
                        ))
                        .size(theme::BODY)
                        .color(theme::TEXT_SECONDARY)
                        .wrapping(Wrapping::WordOrGlyph)
                        .width(Length::Fill),
                        page::white_button(
                            "Open the page",
                            Some(message(Message::OpenPage(page_url)))
                        ),
                    ]
                    .spacing(theme::SPACE_12)
                    .align_y(Alignment::Center),
                );
            } else if let Some(url) = &login.authorize_url {
                waiting = waiting.push(
                    row![
                        text("The provider's page opened in your browser. Log in there.")
                            .size(theme::BODY)
                            .color(theme::TEXT_SECONDARY)
                            .width(Length::Fill),
                        page::white_button(
                            "Open the page again",
                            Some(message(Message::OpenPage(url.clone())))
                        ),
                    ]
                    .spacing(theme::SPACE_12)
                    .align_y(Alignment::Center),
                );
                waiting = waiting.push(paste_view(place, paste));
            }
            waiting
                .push(
                    text("Waiting for the login…")
                        .size(theme::CONTEXT)
                        .font(theme::manrope(Weight::Semibold))
                        .color(theme::TEXT_MUTED),
                )
                .push(page::white_button(
                    "Cancel",
                    Some(message(Message::CancelLogIn(place))),
                ))
                .into()
        }
        "logged_in" => notice_line(&login.message, false),
        _ => notice_line(&login.message, true),
    }
}
