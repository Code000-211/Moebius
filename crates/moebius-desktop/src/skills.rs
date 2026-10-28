//! The Skills page: the skills an agent can name, and an editor for the one chosen. An edit is `PUT /v1/skills/{name}`,
//! which the server makes a commit in the harness configuration repository. The skill Moebius brings is shown and
//! cannot be changed. A server without a harness configuration repository lists no skills, and the page then offers to
//! set one up (see `harness_setup`).

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, row, scrollable, space, text, text_editor, text_input,
};
use iced::{Alignment, Border, Element, Length, Task};

use crate::agents::{field, notice_line};
use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Skill, SkillDetail};
use crate::harness_setup::{self, HarnessSetup};
use crate::import::{self, Import};
use crate::live::say;
use crate::page::{self, Tone};
use crate::settings::Settings;
use crate::{connect, theme};

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Vec<Skill>, ClientError>),
    /// A skill of the list was chosen.
    Select(String),
    /// The skill with this name was read.
    Opened(String, Result<SkillDetail, ClientError>),
    NewSkill,
    NameEdited(String),
    DescriptionEdited(String),
    Text(text_editor::Action),
    Save,
    /// The server answered the edit: the skill as committed, or why not.
    Saved(Result<SkillDetail, ClientError>),
    AskDelete,
    Keep,
    Delete,
    Deleted(Result<(), ClientError>),
    /// A message of the import panel.
    Import(import::Message),
}

enum Notice {
    /// The edit is a commit with this short id; none when the edit changed nothing.
    Saved(Option<String>),
    Failed(String),
}

struct Editor {
    name: String,
    /// Whether the name is still being typed, as for a new skill.
    naming: bool,
    /// Whether the skill is in the repository, so that it can be deleted.
    exists: bool,
    built_in: bool,
    reading: bool,
    description: String,
    text: text_editor::Content,
    /// The skill's other files, which are listed and not edited.
    files: Vec<String>,
    notice: Option<Notice>,
    /// Whether "Delete" asks to be confirmed.
    confirming: bool,
}

impl Editor {
    fn blank(name: &str) -> Self {
        Editor {
            name: name.to_string(),
            naming: false,
            exists: false,
            built_in: false,
            reading: false,
            description: String::new(),
            text: text_editor::Content::new(),
            files: vec![],
            notice: None,
            confirming: false,
        }
    }

    /// The skill's name, description and text as a `PUT` sends them, or what is wrong with the fields.
    fn request(&self) -> Result<(String, String, String), String> {
        let name = self.name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("A name is made of letters, digits, - and _.".into());
        }
        let description = self.description.trim();
        if description.is_empty() {
            return Err("Enter the description.".into());
        }
        let mut text = self.text.text();
        // The editor keeps a final line break of its own; the file is not given a second one.
        if text.ends_with('\n') {
            text.pop();
        }
        Ok((name.to_string(), description.to_string(), text))
    }
}

pub struct Skills {
    /// The skills, once read. A server with a harness configuration repository always lists the built-in one, so an
    /// empty list means there is none and nothing can be edited.
    list: Option<Vec<Skill>>,
    editor: Option<Editor>,
    import: Import,
}

impl Default for Skills {
    fn default() -> Self {
        Skills {
            list: None,
            editor: None,
            import: Import::new(import::Kind::Skills),
        }
    }
}

impl Skills {
    fn editable(&self) -> bool {
        self.list.as_ref().is_some_and(|list| !list.is_empty())
    }

    /// Reads the skills again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        Task::perform(async move { client.skills().await }, |result| {
            AppMessage::Skills(Message::Listed(result))
        })
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(message, Message::Listed(Err(ClientError::Unauthorized)))
    }

    /// The import panel, which says what "Ask the agent" would send.
    pub fn import(&self) -> &Import {
        &self.import
    }

    /// What a save would send: the skill's name, description and text, or what is wrong with the editor.
    pub fn save_request(&self) -> Result<(String, String, String), String> {
        match &self.editor {
            Some(editor) => editor.request(),
            None => Err("No skill is open.".into()),
        }
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        match message {
            Message::Listed(Ok(list)) => self.list = Some(list),
            Message::Listed(Err(_)) => {}
            Message::Select(name) => {
                let mut editor = Editor::blank(&name);
                editor.reading = true;
                editor.built_in = self
                    .list
                    .iter()
                    .flatten()
                    .any(|skill| skill.name == name && skill.built_in);
                self.editor = Some(editor);
                let client = Client::new(settings.clone());
                return Task::perform(
                    {
                        let name = name.clone();
                        async move { client.skill(&name).await }
                    },
                    move |result| AppMessage::Skills(Message::Opened(name.clone(), result)),
                );
            }
            Message::Opened(name, result) => {
                if let Some(editor) = self.editor.as_mut().filter(|editor| editor.name == name) {
                    editor.reading = false;
                    match result {
                        Ok(detail) => {
                            editor.exists = !editor.built_in;
                            editor.description = detail.description;
                            editor.text = text_editor::Content::with_text(&detail.text);
                            editor.files = detail.files;
                        }
                        Err(e) => editor.notice = Some(Notice::Failed(say(&e))),
                    }
                }
            }
            Message::NewSkill => {
                self.editor = Some(Editor {
                    naming: true,
                    ..Editor::blank("")
                });
            }
            Message::Save => return self.save(settings),
            Message::Saved(Ok(detail)) => {
                if let Some(editor) = &mut self.editor
                    && editor.name == detail.name
                {
                    editor.naming = false;
                    editor.exists = true;
                    editor.files = detail.files;
                    editor.notice = Some(Notice::Saved(detail.commit));
                }
                return self.reload(settings);
            }
            Message::Delete => {
                if let Some(editor) = &self.editor {
                    let (client, name) = (Client::new(settings.clone()), editor.name.clone());
                    return Task::perform(async move { client.delete_skill(&name).await }, |r| {
                        AppMessage::Skills(Message::Deleted(r))
                    });
                }
            }
            Message::Deleted(Ok(())) => {
                self.editor = None;
                return self.reload(settings);
            }
            Message::Saved(Err(e)) | Message::Deleted(Err(e)) => {
                if let Some(editor) = &mut self.editor {
                    editor.confirming = false;
                    editor.notice = Some(Notice::Failed(say(&e)));
                }
            }
            Message::Import(message) => return self.import.update(message, settings),
            edit => self.edit(edit),
        }
        Task::none()
    }

    /// The changes of the editor's fields.
    fn edit(&mut self, message: Message) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        editor.notice = None;
        match message {
            Message::NameEdited(name) => editor.name = name,
            Message::DescriptionEdited(description) => editor.description = description,
            Message::Text(action) => editor.text.perform(action),
            Message::AskDelete => editor.confirming = true,
            Message::Keep => editor.confirming = false,
            _ => {}
        }
    }

    fn save(&mut self, settings: &Settings) -> Task<AppMessage> {
        let request = self.save_request();
        let Some(editor) = &mut self.editor else {
            return Task::none();
        };
        match request {
            Ok((name, description, text)) => {
                // A skill saved under a new name is open under it from now on.
                editor.name.clone_from(&name);
                editor.notice = None;
                let client = Client::new(settings.clone());
                Task::perform(
                    async move { client.save_skill(&name, &description, &text).await },
                    |result| AppMessage::Skills(Message::Saved(result)),
                )
            }
            Err(problem) => {
                editor.notice = Some(Notice::Failed(problem));
                Task::none()
            }
        }
    }

    /// The Skills page: the list, and the editor of the skill chosen. Without a harness configuration repository, the
    /// box that sets one up, and the buttons muted.
    pub fn view<'a>(&'a self, harness: &'a HarnessSetup) -> Element<'a, AppMessage> {
        let new_skill = self
            .editable()
            .then_some(AppMessage::Skills(Message::NewSkill));
        let buttons = [
            page::white_button(
                "Import",
                new_skill
                    .as_ref()
                    .map(|_| AppMessage::Skills(Message::Import(import::Message::Show(true)))),
            ),
            page::white_button("New skill", new_skill.clone()),
        ]
        .map(|button| {
            if self.editable() {
                button
            } else {
                harness_setup::muted(button)
            }
        });
        let header = page::header(
            "Build",
            "Skills",
            None,
            page::context(
                "What an agent can be given: the skills in skills/ of the harness configuration repository, and the one Moebius brings. An edit is a commit.",
            ),
            row(buttons).spacing(10).into(),
        );
        if !self.editable() {
            // Until the server has said, nothing is offered.
            let content = match self.list {
                Some(_) => harness.view(),
                None => page::body("Reading the skills…"),
            };
            return column![header, content]
                .spacing(theme::PAGE_GAP)
                .width(Length::Fill)
                .into();
        }
        let chosen = self.editor.as_ref().map(|editor| editor.name.as_str());
        let cards = column(self.list.iter().flatten().map(|skill| {
            list_card(
                &skill.name,
                skill.built_in.then_some("Built in"),
                &skill.description,
                false,
                chosen == Some(skill.name.as_str()),
                AppMessage::Skills(Message::Select(skill.name.clone())),
            )
        }))
        .spacing(8)
        .padding(iced::Padding::default().right(12));
        let mut detail = column![].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(editor) = &self.editor {
            detail = detail.push(editor_view(editor));
        }
        column![
            header,
            self.import.is_open().then(|| self.import.view()),
            row![
                scrollable(cards).width(280),
                scrollable(detail).width(Length::Fill)
            ]
            .spacing(14)
            .height(Length::Fill),
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }
}

fn text_action(action: text_editor::Action) -> AppMessage {
    AppMessage::Skills(Message::Text(action))
}

fn editor_view(editor: &Editor) -> Element<'_, AppMessage> {
    let message = AppMessage::Skills;
    let title: Element<_> = if editor.naming {
        text_input("Name of the new skill", &editor.name)
            .on_input(move |name| message(Message::NameEdited(name)))
            .size(theme::BODY)
            .padding([9, 12])
            .width(280)
            .style(connect::input_style)
            .into()
    } else {
        text(editor.name.as_str())
            .size(21)
            .font(theme::manrope(Weight::Bold))
            .into()
    };
    let mut actions = row![].spacing(10).align_y(Alignment::Center);
    if editor.built_in {
        actions = actions.push(page::tag("Built in · read only".to_string(), Tone::Plain));
    } else if editor.confirming {
        actions = actions
            .push(
                text(format!("Delete {}?", editor.name))
                    .size(theme::CONTEXT)
                    .color(theme::TEXT_SECONDARY),
            )
            .push(page::white_button("Keep", Some(message(Message::Keep))))
            .push(page::white_button(
                "Delete skill",
                Some(message(Message::Delete)),
            ));
    } else {
        if editor.exists {
            actions = actions.push(page::white_button(
                "Delete",
                Some(message(Message::AskDelete)),
            ));
        }
        actions = actions.push(page::primary_button(
            "Save",
            (!editor.reading).then_some(message(Message::Save)),
        ));
    }
    let head = row![title, space::horizontal(), actions].align_y(Alignment::Center);
    let mut top = column![head].spacing(12);
    if editor.reading {
        top = top.push(page::body("Reading the skill…"));
    } else {
        let description: Element<_> = if editor.built_in {
            page::body(editor.description.as_str())
        } else {
            text_input(
                "One line that says when to use the skill",
                &editor.description,
            )
            .on_input(move |text| message(Message::DescriptionEdited(text)))
            .size(theme::BODY)
            .padding([9, 12])
            .style(connect::input_style)
            .into()
        };
        top = top.push(field("Description", description));
    }
    match &editor.notice {
        Some(Notice::Saved(Some(commit))) => {
            top = top.push(notice_line(&format!("Saved as a commit: {commit}"), false));
        }
        Some(Notice::Saved(None)) => {
            top = top.push(notice_line(
                "Nothing changed, so nothing was committed.",
                false,
            ));
        }
        Some(Notice::Failed(error)) => top = top.push(notice_line(error, true)),
        None => {}
    }
    let mut boxes = column![boxed(top)].spacing(theme::PAGE_GAP);
    if !editor.reading {
        boxes = boxes.push(boxed(
            column![
                row![
                    text("SKILL.md")
                        .size(theme::BOX_HEADING)
                        .font(theme::manrope(Weight::Bold)),
                    space::horizontal(),
                    text("Markdown · the text after the frontmatter")
                        .size(theme::LABEL)
                        .color(theme::TEXT_MUTED),
                ]
                .align_y(Alignment::Center),
                mono_editor(
                    &editor.text,
                    "What the skill teaches the agent, in Markdown.",
                    (!editor.built_in).then_some(text_action as fn(_) -> _),
                    360,
                ),
            ]
            .spacing(12),
        ));
        boxes = boxes.push(files_box(&editor.files));
    }
    boxes.into()
}

fn files_box(files: &[String]) -> Element<'_, AppMessage> {
    let list: Element<_> = if files.is_empty() {
        page::body("No other file.")
    } else {
        column(files.iter().map(|file| {
            text(file.as_str())
                .size(theme::CODE_SMALL)
                .font(theme::MONO)
                .wrapping(Wrapping::WordOrGlyph)
                .into()
        }))
        .spacing(6)
        .into()
    };
    boxed(
        column![
            row![
                text("Other files")
                    .size(theme::BOX_HEADING)
                    .font(theme::manrope(Weight::Bold)),
                space::horizontal(),
                text("Beside SKILL.md · not edited here")
                    .size(theme::LABEL)
                    .color(theme::TEXT_MUTED),
            ]
            .align_y(Alignment::Center),
            list,
        ]
        .spacing(12),
    )
}

/// A white box around `content`.
pub(crate) fn boxed<'a>(content: impl Into<Element<'a, AppMessage>>) -> Element<'a, AppMessage> {
    container(content)
        .padding(20)
        .width(Length::Fill)
        .style(|_| page::box_style())
        .into()
}

/// The text editor of the pages, in the code font. Without `on_action` the text can be read and not changed.
pub(crate) fn mono_editor<'a>(
    content: &'a text_editor::Content,
    placeholder: &'a str,
    on_action: Option<fn(text_editor::Action) -> AppMessage>,
    height: u32,
) -> Element<'a, AppMessage> {
    let mut input = text_editor(content)
        .placeholder(placeholder)
        .font(theme::MONO)
        .size(theme::CODE)
        .padding(14)
        .height(height)
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
        });
    if let Some(on_action) = on_action {
        input = input.on_action(on_action);
    }
    input.into()
}

/// A card of a list on the left: a name, an optional tag, and a line that is red when something is wrong.
pub(crate) fn list_card<'a>(
    name: &'a str,
    tag: Option<&'a str>,
    line: &'a str,
    red: bool,
    chosen: bool,
    on_press: AppMessage,
) -> Element<'a, AppMessage> {
    let mut head = row![
        text(name)
            .size(theme::CARD_TITLE)
            .font(theme::manrope(Weight::Semibold)),
        space::horizontal(),
    ]
    .align_y(Alignment::Center);
    if let Some(tag) = tag {
        head = head.push(page::tag(tag.to_string(), Tone::Plain));
    }
    let mut content = column![head].spacing(4);
    if !line.is_empty() {
        content = content.push(
            text(line)
                .size(theme::LABEL)
                .color(if red {
                    theme::RED_TEXT
                } else {
                    theme::TEXT_SECONDARY
                })
                .wrapping(Wrapping::WordOrGlyph),
        );
    }
    button(content)
        .width(Length::Fill)
        .padding([12, 14])
        .on_press(on_press)
        .style(move |_, status| {
            let (fill, line) = match (red, chosen, status) {
                (true, _, _) => (theme::RED_FILL, theme::RED_BORDER),
                (false, true, _) => (theme::SURFACE, theme::TEAL),
                (false, false, button::Status::Hovered) => (theme::LANE, theme::LINE),
                (false, false, _) => (theme::SURFACE, theme::LINE),
            };
            button::Style {
                background: Some(fill.into()),
                text_color: theme::TEXT,
                border: Border {
                    color: line,
                    width: if chosen { 2.0 } else { 1.0 },
                    radius: theme::RADIUS_CARD.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}
