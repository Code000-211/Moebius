//! The files of the skill open on the Skills page: the list of them, `SKILL.md` first, and the editor of one of the
//! others. Reading a file is `GET /v1/skills/{name}/files/{path}`; saving and deleting it are `PUT` and `DELETE` of the
//! same route, each a commit in the harness configuration repository. A built-in skill's files are read only, and a
//! file that is not text is listed with its size and can only be deleted.

use iced::widget::{button, column, container, row, space, text, text_editor, text_input};
use iced::{Alignment, Border, Element, Length, Task};

use super::{Editor, Notice};
use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, FileDetail, SkillDetail};
use crate::screens::live::say;
use crate::settings::Settings;
use crate::widgets::form::{aside, box_heading, committed_line, mono_editor, notice_line};
use crate::widgets::icons::Icon;
use crate::widgets::list_editor::boxed;
use crate::widgets::page;
use crate::{screens::connect, theme};

#[derive(Debug, Clone)]
pub enum Message {
    /// `SKILL.md` (none) or another file of the list was chosen.
    Open(Option<String>),
    /// The file at this path was read.
    Opened(String, Result<FileDetail, ClientError>),
    NewFile,
    PathEdited(String),
    Text(text_editor::Action),
    Save,
    AskDelete,
    Keep,
    Delete,
    /// The server answered a save: the skill with its files, or why not.
    Saved(Result<SkillDetail, ClientError>),
    /// The server answered a delete, the same way.
    Deleted(Result<SkillDetail, ClientError>),
}

/// One of the skill's other files, open in the editor.
pub(super) struct FileEditor {
    path: String,
    /// Whether the path is still being typed, as for a new file.
    naming: bool,
    reading: bool,
    /// The text; none for a file that is not text or is too large to edit here.
    text: Option<text_editor::Content>,
    size: u64,
    notice: Option<Notice>,
    /// Whether "Delete file" asks to be confirmed.
    confirming: bool,
}

impl FileEditor {
    fn new(path: &str, naming: bool) -> Self {
        FileEditor {
            path: path.to_string(),
            naming,
            reading: !naming,
            text: naming.then(text_editor::Content::new),
            size: 0,
            notice: None,
            confirming: false,
        }
    }

    /// The path and text a save sends, or what is wrong with them.
    pub(super) fn request(&self) -> Result<(String, String), String> {
        let path = self.path.trim().trim_matches('/');
        if path.is_empty() || path.split('/').any(|part| part.is_empty() || part == "..") {
            return Err("Enter the file's path inside the skill, such as scripts/fill.py.".into());
        }
        if path == "SKILL.md" {
            return Err("SKILL.md is saved with the skill.".into());
        }
        let Some(content) = &self.text else {
            return Err("This file is not text and cannot be edited here.".into());
        };
        Ok((path.to_string(), content.text()))
    }
}

fn wrap(message: Message) -> AppMessage {
    AppMessage::Skills(super::Message::File(message))
}

/// What a message about the files does to the skill open in `editor`.
pub(super) fn update(
    editor: &mut Editor,
    message: Message,
    settings: &Settings,
) -> Task<AppMessage> {
    let client = Client::new(settings.clone());
    let name = editor.name.clone();
    match message {
        Message::Open(None) => editor.file = None,
        Message::Open(Some(path)) => {
            editor.file = Some(FileEditor::new(&path, false));
            return Task::perform(
                {
                    let path = path.clone();
                    async move { client.skill_file(&name, &path).await }
                },
                move |result| wrap(Message::Opened(path.clone(), result)),
            );
        }
        Message::Opened(path, result) => {
            if let Some(file) = editor.file.as_mut().filter(|file| file.path == path) {
                file.reading = false;
                match result {
                    Ok(detail) => {
                        file.size = detail.size;
                        file.text = detail.text.as_deref().map(text_editor::Content::with_text);
                    }
                    Err(e) => file.notice = Some(Notice::Failed(say(&e))),
                }
            }
        }
        Message::NewFile => editor.file = Some(FileEditor::new("", true)),
        Message::Save => {
            let Some(file) = &mut editor.file else {
                return Task::none();
            };
            match file.request() {
                Ok((path, text)) => {
                    file.path.clone_from(&path);
                    file.notice = None;
                    return Task::perform(
                        async move { client.save_skill_file(&name, &path, &text).await },
                        |result| wrap(Message::Saved(result)),
                    );
                }
                Err(problem) => file.notice = Some(Notice::Failed(problem)),
            }
        }
        Message::Delete => {
            if let Some(file) = &editor.file {
                let path = file.path.clone();
                return Task::perform(
                    async move { client.delete_skill_file(&name, &path).await },
                    |result| wrap(Message::Deleted(result)),
                );
            }
        }
        Message::Saved(Ok(detail)) => {
            if let Some(file) = &mut editor.file {
                file.naming = false;
                file.notice = Some(Notice::Saved(detail.commit));
            }
            editor.files = detail.files;
        }
        Message::Deleted(Ok(detail)) => {
            editor.file = None;
            editor.files = detail.files;
            editor.notice = Some(Notice::Saved(detail.commit));
        }
        Message::Saved(Err(e)) | Message::Deleted(Err(e)) => {
            if let Some(file) = &mut editor.file {
                file.confirming = false;
                file.notice = Some(Notice::Failed(say(&e)));
            }
        }
        edit => {
            if let Some(file) = &mut editor.file {
                file.notice = None;
                match edit {
                    Message::PathEdited(path) => file.path = path,
                    Message::Text(action) => {
                        if let Some(text) = &mut file.text {
                            text.perform(action);
                        }
                    }
                    Message::AskDelete => file.confirming = true,
                    Message::Keep => file.confirming = false,
                    _ => {}
                }
            }
        }
    }
    Task::none()
}

/// The box that lists the skill's files, `SKILL.md` first, the one open marked; "New file" adds one to a skill of the
/// repository.
pub(super) fn list(editor: &Editor) -> Element<'_, AppMessage> {
    let open = editor.file.as_ref().map(|file| file.path.as_str());
    let mut rows = column![file_row(
        "SKILL.md",
        open.is_none(),
        wrap(Message::Open(None))
    )]
    .spacing(theme::SPACE_4);
    for path in &editor.files {
        rows = rows.push(file_row(
            path,
            open == Some(path.as_str()),
            wrap(Message::Open(Some(path.clone()))),
        ));
    }
    let new_file: Element<_> = if editor.built_in || !editor.exists {
        aside(if editor.built_in {
            "Read only"
        } else {
            "Save the skill to add files"
        })
    } else {
        page::white_button("New file", Some(wrap(Message::NewFile)))
    };
    boxed(column![box_heading("Files", new_file), rows].spacing(theme::SPACE_12))
}

/// A file of the list.
fn file_row<'a>(path: &'a str, open: bool, on_press: AppMessage) -> Element<'a, AppMessage> {
    let line = row![
        Icon::File.view(theme::TEXT_SECONDARY, theme::ICON),
        text(path)
            .size(theme::CODE_SMALL)
            .font(theme::MONO)
            .color(theme::TEXT),
        space::horizontal(),
    ]
    .spacing(theme::SPACE_8)
    .align_y(Alignment::Center);
    container(
        button(line)
            .padding(theme::CONTROL_PADDING)
            .width(Length::Fill)
            .on_press(on_press)
            .style(move |_, status| button::Style {
                background: (open || status == button::Status::Hovered).then(|| theme::LANE.into()),
                border: Border {
                    radius: theme::RADIUS_SMALL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    )
    .id(format!("skill-file-{path}"))
    .into()
}

fn text_action(action: text_editor::Action) -> AppMessage {
    wrap(Message::Text(action))
}

/// The editor of the file open: its path, its text and the buttons that save and delete it.
pub(super) fn editor_view(file: &FileEditor, built_in: bool) -> Element<'_, AppMessage> {
    let title: Element<_> = if file.naming {
        text_input("Path in the skill, such as scripts/fill.py", &file.path)
            .on_input(|path| wrap(Message::PathEdited(path)))
            .size(theme::BODY)
            .padding(theme::CONTROL_PADDING)
            .width(theme::NAME_INPUT_WIDTH)
            .style(connect::input_style)
            .into()
    } else {
        text(file.path.as_str())
            .size(theme::BOX_HEADING)
            .font(theme::MONO)
            .into()
    };
    let mut actions = row![].spacing(theme::SPACE_12).align_y(Alignment::Center);
    if built_in {
        actions = actions.push(aside("Read only"));
    } else if file.confirming {
        actions = actions
            .push(
                text(format!("Delete {}?", file.path))
                    .size(theme::CONTEXT)
                    .color(theme::TEXT_SECONDARY),
            )
            .push(page::white_button("Keep", Some(wrap(Message::Keep))))
            .push(page::white_button(
                "Delete the file",
                Some(wrap(Message::Delete)),
            ));
    } else {
        if !file.naming {
            actions = actions.push(page::white_button(
                "Delete file",
                Some(wrap(Message::AskDelete)),
            ));
        }
        let save = (!file.reading && file.text.is_some()).then(|| wrap(Message::Save));
        actions = actions.push(page::primary_button("Save file", save));
    }
    let mut content = column![row![title, space::horizontal(), actions].align_y(Alignment::Center)]
        .spacing(theme::SPACE_12);
    match &file.notice {
        Some(Notice::Saved(commit)) => content = content.push(committed_line(commit.as_deref())),
        Some(Notice::Failed(error)) => content = content.push(notice_line(error, true)),
        None => {}
    }
    content = match (&file.text, file.reading) {
        (_, true) => content.push(page::body("Reading the file…")),
        (Some(text), false) => content.push(mono_editor(
            text,
            "The file's text.",
            (!built_in).then_some(text_action as fn(_) -> _),
            theme::EDITOR_FULL,
        )),
        (None, false) => content.push(page::body(format!(
            "Not text, or too large to edit ({}).",
            crate::screens::project::size(file.size)
        ))),
    };
    boxed(content)
}
