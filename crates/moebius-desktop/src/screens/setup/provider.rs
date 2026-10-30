//! "Log in to a provider" on the Models page: the providers the server has built in (Anthropic, OpenAI) and "Other"
//! with the fields of an `oauth` table. Choosing one adds the upstream through `PUT /v1/settings` (live, no restart),
//! then the row of the new upstream asks for the confirmation of the warning and logs in (see `login`).

use iced::widget::{column, container, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length};
use serde_json::{Value, json};

use crate::app::Message as AppMessage;
use crate::client::{Client, Provider};
use crate::screens::connect;
use crate::screens::live::say;
use crate::settings::Settings;
use crate::theme;
use crate::widgets::form::{PROTOCOLS, labelled, menu_style, notice_line, pick_style};
use crate::widgets::page;

use super::{Message, Setup};

/// What "Other" in the choice of providers is called in a message.
pub const OTHER: &str = "other";

/// The flows an `oauth` table can have.
const FLOWS: [&str; 2] = ["code", "device"];

/// Where "Log in to a provider" stands.
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub step: Step,
    /// Why the last step did not work.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// The providers to choose from.
    Choosing,
    /// The fields of an `oauth` table.
    Other(Other),
    /// The server was asked to add the upstream.
    Adding,
}

/// The fields of an `oauth` table, as typed.
#[derive(Debug, Clone, PartialEq)]
pub struct Other {
    pub protocol: String,
    pub base_url: String,
    pub flow: String,
    pub client_id: String,
    /// The authorization URL of a `code` login, or the device URL of a `device` login.
    pub url: String,
    pub token_url: String,
    /// The scopes, separated by spaces.
    pub scopes: String,
}

impl Default for Other {
    fn default() -> Self {
        Self {
            protocol: PROTOCOLS[0].to_string(),
            base_url: String::new(),
            flow: FLOWS[0].to_string(),
            client_id: String::new(),
            url: String::new(),
            token_url: String::new(),
            scopes: String::new(),
        }
    }
}

/// A field of [`Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtherField {
    BaseUrl,
    ClientId,
    Url,
    TokenUrl,
    Scopes,
}

impl Other {
    /// The upstream to add, or what is missing. The refresh token is kept under a name made from the time, so that
    /// the login of an upstream that was removed is not taken for this one's.
    fn upstream(&self, secret_suffix: u64) -> Result<Value, String> {
        let needed = [
            (&self.base_url, "Base URL"),
            (&self.client_id, "Client ID"),
            (
                &self.url,
                if self.flow == "device" {
                    "Device URL"
                } else {
                    "Authorize URL"
                },
            ),
            (&self.token_url, "Token URL"),
        ];
        if let Some((_, name)) = needed.iter().find(|(value, _)| value.trim().is_empty()) {
            return Err(format!("{name} is empty."));
        }
        let mut oauth = json!({
            "flow": self.flow,
            "client_id": self.client_id.trim(),
            "token_url": self.token_url.trim(),
            "secret": format!("login-{secret_suffix}"),
        });
        let url_key = if self.flow == "device" {
            "device_url"
        } else {
            "authorize_url"
        };
        oauth[url_key] = json!(self.url.trim());
        let scopes: Vec<&str> = self.scopes.split_whitespace().collect();
        if !scopes.is_empty() {
            oauth["scopes"] = json!(scopes);
        }
        Ok(json!({
            "protocol": self.protocol,
            "base_url": self.base_url.trim(),
            "oauth": oauth,
        }))
    }
}

impl Setup {
    /// The change a message of "Log in to a provider" makes, and what it asks of the server.
    pub(super) fn update_provider(
        &mut self,
        message: Message,
        settings: Option<&Settings>,
    ) -> iced::Task<AppMessage> {
        match message {
            Message::Providers(Ok(providers)) => self.providers = providers,
            Message::Providers(Err(_)) => {}
            Message::ChooseProvider => {
                self.provider = Some(Panel {
                    step: Step::Choosing,
                    problem: None,
                });
            }
            Message::CancelProvider => self.provider = None,
            Message::PickProvider(name) if name == OTHER => {
                self.provider = Some(Panel {
                    step: Step::Other(Other::default()),
                    problem: None,
                });
            }
            Message::PickProvider(name) => {
                let Some(provider) = self.providers.iter().find(|p| p.name == name) else {
                    return iced::Task::none();
                };
                let upstream = json!({
                    "protocol": provider.protocol,
                    "base_url": provider.base_url,
                    "oauth": {"preset": provider.name},
                });
                return self.add_upstream(upstream, settings);
            }
            Message::OtherEdit(field, text) => {
                if let Some(Panel {
                    step: Step::Other(other),
                    ..
                }) = &mut self.provider
                {
                    match field {
                        OtherField::BaseUrl => other.base_url = text,
                        OtherField::ClientId => other.client_id = text,
                        OtherField::Url => other.url = text,
                        OtherField::TokenUrl => other.token_url = text,
                        OtherField::Scopes => other.scopes = text,
                    }
                }
            }
            Message::OtherProtocol(protocol) => {
                if let Some(Panel {
                    step: Step::Other(other),
                    ..
                }) = &mut self.provider
                {
                    other.protocol = protocol;
                }
            }
            Message::OtherFlow(flow) => {
                if let Some(Panel {
                    step: Step::Other(other),
                    ..
                }) = &mut self.provider
                {
                    other.flow = flow;
                }
            }
            Message::AddOther => {
                let Some(Panel {
                    step: Step::Other(other),
                    problem,
                }) = &mut self.provider
                else {
                    return iced::Task::none();
                };
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| since.as_secs());
                match other.upstream(now) {
                    Ok(upstream) => return self.add_upstream(upstream, settings),
                    Err(missing) => *problem = Some(missing),
                }
            }
            Message::ProviderAdded(place, Ok(_)) => {
                self.provider = None;
                // The row of the new upstream asks for the confirmation of the warning.
                self.logins.insert(place, super::LoginStep::Confirming);
                self.reset = true;
                if let Some(settings) = settings {
                    return self.reload(settings);
                }
            }
            Message::ProviderAdded(_, Err(e)) => {
                if let Some(panel) = &mut self.provider {
                    panel.step = Step::Choosing;
                    panel.problem = Some(say(&e));
                }
            }
            _ => {}
        }
        iced::Task::none()
    }

    /// Adds `upstream` at the end of the upstreams of the file, as the server has them.
    fn add_upstream(
        &mut self,
        upstream: Value,
        settings: Option<&Settings>,
    ) -> iced::Task<AppMessage> {
        let (Some(settings), Some(models)) = (settings, &self.models) else {
            return iced::Task::none();
        };
        let place = models.upstreams.len();
        if let Some(panel) = &mut self.provider {
            panel.step = Step::Adding;
            panel.problem = None;
        }
        let client = Client::new(settings.clone());
        let mut set = serde_json::Map::new();
        set.insert(format!("router.upstreams.{place}"), upstream);
        iced::Task::perform(
            async move { client.save_settings(set, Vec::new()).await },
            move |result| AppMessage::Setup(Message::ProviderAdded(place, result)),
        )
    }
}

fn caption<'a>(line: impl text::IntoFragment<'a>) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::CONTEXT)
        .color(theme::TEXT_MUTED)
        .into()
}

/// One provider to choose: its name, what to know of it, and the button.
fn choice<'a>(title: &'a str, about: String, pick: &'a str) -> Element<'a, AppMessage> {
    row![
        column![
            text(title)
                .size(theme::BODY)
                .font(theme::manrope(iced::font::Weight::Semibold)),
            caption(about),
        ]
        .spacing(theme::SPACE_2)
        .width(Length::Fill),
        page::white_button(
            "Log in",
            Some(AppMessage::Setup(Message::PickProvider(pick.to_string())))
        ),
    ]
    .spacing(theme::SPACE_12)
    .align_y(Alignment::Center)
    .into()
}

fn input<'a>(
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, AppMessage> {
    text_input(placeholder, value)
        .on_input(move |text| AppMessage::Setup(on_input(text)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding(theme::CONTROL_PADDING)
        .style(connect::input_style)
        .into()
}

fn pick<'a>(
    options: &[&str],
    chosen: &str,
    on_pick: impl Fn(String) -> Message + 'a,
) -> Element<'a, AppMessage> {
    pick_list(
        options.iter().map(|o| o.to_string()).collect::<Vec<_>>(),
        Some(chosen.to_string()),
        move |chosen| AppMessage::Setup(on_pick(chosen)),
    )
    .text_size(theme::BODY)
    .padding(theme::CONTROL_PADDING)
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style)
    .into()
}

/// The fields of an `oauth` table.
fn other_view(other: &Other) -> Element<'_, AppMessage> {
    let url_label = if other.flow == "device" {
        "Device URL"
    } else {
        "Authorize URL"
    };
    column![
        row![
            container(labelled(
                "Protocol",
                pick(&PROTOCOLS, &other.protocol, Message::OtherProtocol)
            ))
            .width(Length::FillPortion(2)),
            container(labelled(
                "Base URL",
                input("https://api.example.com", &other.base_url, |t| {
                    Message::OtherEdit(OtherField::BaseUrl, t)
                })
            ))
            .width(Length::FillPortion(6)),
        ]
        .spacing(theme::SPACE_12),
        row![
            container(labelled(
                "Flow",
                pick(&FLOWS, &other.flow, Message::OtherFlow)
            ))
            .width(Length::FillPortion(2)),
            container(labelled(
                "Client ID",
                input("moebius", &other.client_id, |t| {
                    Message::OtherEdit(OtherField::ClientId, t)
                })
            ))
            .width(Length::FillPortion(6)),
        ]
        .spacing(theme::SPACE_12),
        row![
            container(labelled(
                url_label,
                input("https://login.example.com/authorize", &other.url, |t| {
                    Message::OtherEdit(OtherField::Url, t)
                })
            ))
            .width(Length::FillPortion(1)),
            container(labelled(
                "Token URL",
                input("https://login.example.com/token", &other.token_url, |t| {
                    Message::OtherEdit(OtherField::TokenUrl, t)
                })
            ))
            .width(Length::FillPortion(1)),
        ]
        .spacing(theme::SPACE_12),
        labelled(
            "Scopes, separated by spaces",
            input("models offline_access", &other.scopes, |t| {
                Message::OtherEdit(OtherField::Scopes, t)
            })
        ),
        row![
            page::primary_button(
                "Add and continue",
                Some(AppMessage::Setup(Message::AddOther))
            ),
            page::white_button("Cancel", Some(AppMessage::Setup(Message::CancelProvider))),
        ]
        .spacing(theme::SPACE_12),
    ]
    .spacing(theme::SPACE_12)
    .into()
}

/// The panel under the buttons of the Upstreams box.
pub(super) fn panel_view<'a>(
    providers: &'a [Provider],
    panel: &'a Panel,
) -> Element<'a, AppMessage> {
    let mut content = column![].spacing(theme::SPACE_12);
    match &panel.step {
        Step::Choosing => {
            content = content.push(page::body(
                "Log in with a subscription you already have. The provider's page opens in your browser; \
                 Moebius adds the upstream first.",
            ));
            for provider in providers {
                let about = match &provider.needs {
                    Some(needs) => {
                        format!("Logs in, but its requests need {needs}, which is not served yet.")
                    }
                    None => format!("{} {}", provider.protocol, provider.base_url),
                };
                content =
                    content
                        .push(page::rule())
                        .push(choice(&provider.title, about, &provider.name));
            }
            content = content.push(page::rule()).push(choice(
                "Other",
                "Any OAuth provider: its client id and addresses.".to_string(),
                OTHER,
            ));
            content = content.push(page::white_button(
                "Cancel",
                Some(AppMessage::Setup(Message::CancelProvider)),
            ));
        }
        Step::Other(other) => content = content.push(other_view(other)),
        Step::Adding => content = content.push(page::body("Adding the upstream…")),
    }
    if let Some(problem) = &panel.problem {
        content = content.push(notice_line(problem, true));
    }
    content.into()
}
