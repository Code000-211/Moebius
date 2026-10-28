//! The editor of a column's script, opened by the pencil in a column's header on the board. It shows the script the
//! column runs for the repository, the repository's own or the built-in one, and saves it with
//! `PUT /v1/repositories/{id}/scripts/{column}`, which checks it and commits it. "Use the built-in script" removes
//! the repository's own. The speech bubble starts a chat draft with the Moebius agent about the script.

use iced::advanced::text::highlighter::Format;
use iced::font::Weight;
use iced::highlighter;
use iced::widget::{button, column, container, row, space, text, text_editor};
use iced::{Alignment, Border, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, ScriptDetail};
use crate::icons::Icon;
use crate::live::say;
use crate::page::{self, Tone};
use crate::settings::Settings;
use crate::theme;

#[derive(Debug, Clone)]
pub enum Message {
    /// The script was read, or why not.
    Opened(Result<ScriptDetail, ClientError>),
    Text(text_editor::Action),
    Save,
    /// The server answered the save: the script as committed, or the check's refusal.
    Saved(Result<ScriptDetail, ClientError>),
    AskBuiltIn,
    Keep,
    UseBuiltIn,
    /// The server answered removing the repository's script: the built-in one, or why not.
    UsedBuiltIn(Result<ScriptDetail, ClientError>),
    /// The person wants to talk to the Moebius agent about this script; the app opens the chat.
    AskAgent,
}

/// The editor of one column's script of one repository.
pub struct ScriptEditor {
    repository: i64,
    repository_name: String,
    column: String,
    /// Where the repository keeps its own script, once the server has said.
    path: String,
    /// Whether the repository has a script of its own, once the server has said.
    own: Option<bool>,
    text: text_editor::Content,
    /// The refusal or failure of the last request.
    error: Option<String>,
    saved: bool,
    /// Whether the text was edited since the script was read or saved.
    edited: bool,
    /// Whether "Use the built-in script" asks to be confirmed.
    confirming: bool,
}

impl ScriptEditor {
    /// The editor of `column` in the repository, and the task that reads its script.
    pub fn open(
        repository: i64,
        repository_name: &str,
        column: &str,
        settings: &Settings,
    ) -> (Self, Task<AppMessage>) {
        let editor = ScriptEditor {
            repository,
            repository_name: repository_name.to_string(),
            column: column.to_string(),
            path: String::new(),
            own: None,
            text: text_editor::Content::new(),
            error: None,
            saved: false,
            edited: false,
            confirming: false,
        };
        let (client, column) = (Client::new(settings.clone()), column.to_string());
        let task = Task::perform(
            async move { client.script(repository, &column).await },
            |result| AppMessage::Script(Message::Opened(result)),
        );
        (editor, task)
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Opened(Err(ClientError::Unauthorized))
                | Message::Saved(Err(ClientError::Unauthorized))
                | Message::UsedBuiltIn(Err(ClientError::Unauthorized))
        )
    }

    /// The message the chat draft starts with: what the person only has to finish.
    pub fn about(&self) -> String {
        let path = if self.path.is_empty() {
            "the script"
        } else {
            &self.path
        };
        format!("About the {} column's script ({path}): ", self.column)
    }

    /// The repository the script belongs to.
    pub fn repository(&self) -> i64 {
        self.repository
    }

    /// The column whose script this is.
    pub fn column(&self) -> &str {
        &self.column
    }

    /// Where the repository keeps its own script, once the server has said; empty until then.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Whether the text has changes that are not saved.
    pub fn unsaved(&self) -> bool {
        self.edited
    }

    fn adopt(&mut self, detail: ScriptDetail) {
        self.edited = false;
        self.own = Some(detail.source == "repository");
        self.path = detail.path;
        self.text = text_editor::Content::with_text(&detail.text);
    }

    /// The text a save sends. The editor keeps a final line break of its own; the file is not given a second one.
    pub fn request(&self) -> String {
        let mut text = self.text.text();
        if text.ends_with('\n') {
            text.pop();
        }
        text
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        let (repository, column) = (self.repository, self.column.clone());
        let client = Client::new(settings.clone());
        match message {
            Message::Opened(Ok(detail)) => self.adopt(detail),
            Message::Saved(Ok(detail)) => {
                self.adopt(detail);
                self.saved = true;
            }
            Message::UsedBuiltIn(Ok(detail)) => {
                self.adopt(detail);
                self.confirming = false;
                self.saved = true;
            }
            Message::Opened(Err(e)) | Message::Saved(Err(e)) | Message::UsedBuiltIn(Err(e)) => {
                self.confirming = false;
                self.error = Some(say(&e));
            }
            Message::Text(action) => {
                self.saved = false;
                self.edited |= action.is_edit();
                self.error = None;
                self.text.perform(action);
            }
            Message::Save => {
                self.error = None;
                let text = self.request();
                return Task::perform(
                    async move { client.save_script(repository, &column, &text).await },
                    |result| AppMessage::Script(Message::Saved(result)),
                );
            }
            Message::AskBuiltIn => self.confirming = true,
            Message::Keep => self.confirming = false,
            Message::UseBuiltIn => {
                self.error = None;
                return Task::perform(
                    async move { client.delete_script(repository, &column).await },
                    |result| AppMessage::Script(Message::UsedBuiltIn(result)),
                );
            }
            Message::AskAgent => {}
        }
        Task::none()
    }

    /// The page: the column's header and the script in an editor.
    pub fn view(&self) -> Element<'_, AppMessage> {
        let (tag, own) = match self.own {
            Some(true) => (Some(("Repository", Tone::Teal)), true),
            Some(false) => (Some(("Built in", Tone::Plain)), false),
            None => (None, false),
        };
        let breadcrumb = row![
            button(
                text(self.repository_name.as_str())
                    .size(theme::BREADCRUMB)
                    .color(theme::TEXT_SECONDARY)
            )
            .padding(0)
            .on_press(AppMessage::CloseCard)
            .style(|_, _| button::Style::default()),
            text(" / Column script")
                .size(theme::BREADCRUMB)
                .color(theme::TEXT_MUTED),
        ];
        let mut actions = row![icon_button(
            Icon::Chat,
            AppMessage::Script(Message::AskAgent),
            ASK_AGENT
        )]
        .spacing(8)
        .align_y(Alignment::Center);
        if self.confirming {
            actions = actions
                .push(
                    text("Remove the repository's script?")
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY),
                )
                .push(page::white_button(
                    "Keep",
                    Some(AppMessage::Script(Message::Keep)),
                ))
                .push(page::white_button(
                    "Use the built-in script",
                    Some(AppMessage::Script(Message::UseBuiltIn)),
                ));
        } else {
            if own {
                actions = actions.push(page::white_button(
                    "Use the built-in script",
                    Some(AppMessage::Script(Message::AskBuiltIn)),
                ));
            }
            actions = actions.push(page::primary_button(
                "Save",
                self.own
                    .is_some()
                    .then_some(AppMessage::Script(Message::Save)),
            ));
        }
        let context = match self.own {
            Some(true) => {
                "The repository's own script, from its default branch. A save is a commit."
            }
            Some(false) => {
                "The script built into Moebius. A save commits a script of the repository's own."
            }
            None => "Reading the script.",
        };
        let header = page::header_with(
            Some(breadcrumb.into()),
            &self.column,
            tag.map(|(label, tone)| page::tag(label.to_string(), tone)),
            page::context(context),
            actions.into(),
        );
        let mut content = column![
            row![
                text(if self.path.is_empty() {
                    "Script"
                } else {
                    self.path.as_str()
                })
                .size(theme::CODE)
                .font(theme::MONO)
                .color(theme::TEXT_SECONDARY),
                space::horizontal(),
                text("Luau · checked when saved")
                    .size(theme::LABEL)
                    .color(theme::TEXT_MUTED),
            ]
            .align_y(Alignment::Center),
            code_editor(&self.text),
        ]
        .spacing(12);
        if let Some(error) = &self.error {
            content = content.push(
                text(error.as_str())
                    .size(theme::CONTEXT)
                    .font(theme::manrope(Weight::Semibold))
                    .color(theme::RED_TEXT),
            );
        } else if self.saved {
            content = content.push(
                text("Saved as a commit on the default branch.")
                    .size(theme::CONTEXT)
                    .font(theme::manrope(Weight::Semibold))
                    .color(theme::ONLINE_TEXT),
            );
        }
        column![
            header,
            container(content)
                .padding(20)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| page::box_style()),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }
}

/// The script in the code font, highlighted as Lua, which Luau is a dialect of.
fn code_editor(content: &text_editor::Content) -> Element<'_, AppMessage> {
    text_editor(content)
        .on_action(|action| AppMessage::Script(Message::Text(action)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding(14)
        .height(Length::Fill)
        .highlight_with::<highlighter::Highlighter>(
            highlighter::Settings {
                theme: highlighter::Theme::InspiredGitHub,
                token: "lua".to_string(),
            },
            |highlight, _| Format {
                color: highlight.color(),
                font: None,
            },
        )
        .style(|_, status| text_editor::Style {
            background: theme::LANE.into(),
            border: Border {
                color: match status {
                    text_editor::Status::Focused { .. } => theme::TEAL,
                    _ => theme::LINE,
                },
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            placeholder: theme::TEXT_MUTED,
            value: theme::TEXT,
            selection: theme::TEAL_FILL,
        })
        .into()
}

/// The name of the speech bubble that asks the Moebius agent about the script, for a test to find it.
pub const ASK_AGENT: &str = "ask-the-moebius-agent";

/// A square button with a line icon, for what has no words, named `id`.
fn icon_button<'a>(icon: Icon, message: AppMessage, id: &'static str) -> Element<'a, AppMessage> {
    container(
        button(icon.view(theme::TEXT, 16.0))
            .padding([8, 10])
            .on_press(message)
            .style(|_, status| button::Style {
                background: Some(
                    if status == button::Status::Hovered {
                        theme::LANE
                    } else {
                        theme::SURFACE
                    }
                    .into(),
                ),
                border: Border {
                    color: theme::LINE,
                    width: 1.0,
                    radius: theme::RADIUS_CONTROL.into(),
                },
                ..button::Style::default()
            }),
    )
    .id(id)
    .into()
}
