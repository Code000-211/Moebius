//! The Skills page: the skills an agent can name, and an editor for the one chosen: its description and the list of its
//! files, with the editor of the file open (`files`). An edit of `SKILL.md` is `PUT /v1/skills/{name}`, which the server
//! makes a commit in the harness configuration repository. The skills Moebius brings are shown and cannot be changed. A
//! server without a harness configuration repository lists no skills, and the page then offers to set one up (see
//! `harness_setup`).

mod files;

use iced::widget::{column, row, text_editor, text_input};
use iced::{Alignment, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Skill, SkillDetail};
use crate::screens::harness_setup::{self, HarnessSetup};
use crate::screens::live::say;
use crate::settings::Settings;
use crate::widgets::form::{aside, box_heading, committed_line, field, mono_editor, notice_line};
use crate::widgets::import::{self, Import};
use crate::widgets::list_editor::{self, Actions, list_card};
use crate::widgets::page::{self, Tone};
use crate::{screens::connect, theme};

// The white box, which the import panel took from here first.
pub(crate) use crate::widgets::list_editor::boxed;

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
    /// A message about the skill's files.
    File(files::Message),
}

pub use files::Message as FileMessage;

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
    /// The skill's other files.
    files: Vec<String>,
    /// The other file open in the editor; none while `SKILL.md` is.
    file: Option<files::FileEditor>,
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
            file: None,
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

    /// What saving the file open would send: its path and text, or what is wrong.
    pub fn save_file_request(&self) -> Result<(String, String), String> {
        match self.editor.as_ref().and_then(|editor| editor.file.as_ref()) {
            Some(file) => file.request(),
            None => Err("No file is open.".into()),
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
            Message::File(message) => {
                if let Some(editor) = &mut self.editor {
                    return files::update(editor, message, settings);
                }
            }
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
                skill
                    .built_in
                    .then_some(("Built in", Tone::Plain))
                    .into_iter()
                    .collect(),
                skill.description.as_str(),
                false,
                chosen == Some(skill.name.as_str()),
                Some(AppMessage::Skills(Message::Select(skill.name.clone()))),
            )
        }))
        .spacing(8)
        .padding(iced::Padding::default().right(12));
        let mut detail = column![].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(editor) = &self.editor {
            detail = detail.push(editor_view(editor));
        }
        list_editor::layout(
            header,
            self.import.is_open().then(|| self.import.view()),
            cards,
            detail,
        )
    }
}

fn text_action(action: text_editor::Action) -> AppMessage {
    AppMessage::Skills(Message::Text(action))
}

fn editor_view(editor: &Editor) -> Element<'_, AppMessage> {
    let message = AppMessage::Skills;
    let actions = if editor.built_in {
        row![page::tag("Built in · read only".to_string(), Tone::Plain)]
            .spacing(10)
            .align_y(Alignment::Center)
    } else {
        list_editor::actions(
            &editor.name,
            editor.exists,
            editor.confirming,
            "Delete skill",
            Actions {
                ask_delete: message(Message::AskDelete),
                keep: message(Message::Keep),
                delete: message(Message::Delete),
                save: (!editor.reading).then_some(message(Message::Save)),
            },
        )
    };
    let head = list_editor::head(
        editor.naming,
        &editor.name,
        "Name of the new skill",
        move |name| message(Message::NameEdited(name)),
        actions,
    );
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
        Some(Notice::Saved(commit)) => top = top.push(committed_line(commit.as_deref())),
        Some(Notice::Failed(error)) => top = top.push(notice_line(error, true)),
        None => {}
    }
    let mut boxes = column![boxed(top)].spacing(theme::PAGE_GAP);
    if editor.reading {
        return boxes.into();
    }
    boxes = boxes.push(files::list(editor));
    if let Some(file) = &editor.file {
        boxes = boxes.push(files::editor_view(file, editor.built_in));
    } else {
        boxes = boxes.push(boxed(
            column![
                box_heading(
                    "SKILL.md",
                    aside("Markdown · the text after the frontmatter")
                ),
                mono_editor(
                    &editor.text,
                    "What the skill teaches the agent, in Markdown.",
                    (!editor.built_in).then_some(text_action as fn(_) -> _),
                    360,
                ),
            ]
            .spacing(12),
        ));
    }
    boxes.into()
}
