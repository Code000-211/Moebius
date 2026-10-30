//! How a message changes a project's files: a directory listed, a file read, opened, edited or saved; the messages
//! about tabs go to `tabs` and `shell`.

use iced::Task;
use iced::widget::text_editor;

use crate::app::Message as AppMessage;
use crate::client::{Client, FileDetail};
use crate::screens::live::say;
use crate::settings::Settings;

/// How many opened files are remembered for ranking.
const RECENT_FILES: usize = 10;

use super::{Body, Listing, Message, OpenFile, Project, Tab};

impl OpenFile {
    /// Puts the cursor on the line asked for, once the text is there.
    fn go_to_line(&mut self) {
        let Body::Text { content, .. } = &mut self.body else {
            return;
        };
        let Some(line) = self.goto.take() else {
            return;
        };
        let line = line
            .saturating_sub(1)
            .min(content.line_count().saturating_sub(1));
        content.move_to(text_editor::Cursor {
            position: text_editor::Position { line, column: 0 },
            selection: None,
        });
    }
}

impl Project {
    pub fn update(
        &mut self,
        repository: i64,
        message: Message,
        settings: Option<&Settings>,
    ) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        match message {
            Message::Tab(_)
            | Message::Close(_)
            | Message::Discard(_)
            | Message::Keep
            | Message::ToggleFiles
            | Message::Menu
            | Message::NewEditor
            | Message::NewTerminal
            | Message::NewChat => return self.tabs_update(repository, message, settings),
            Message::Shell(..) | Message::Pane(..) | Message::Key(_) => self.shell_update(message),
            Message::Toggle(dir) => {
                if !self.open_dirs.remove(&dir) {
                    self.open_dirs.insert(dir.clone());
                    return self.list(repository, &dir, settings);
                }
            }
            Message::Listed(dir, Ok(tree)) => {
                self.dirs.insert(dir, Listing::Read(tree.entries));
            }
            Message::Listed(dir, Err(e)) => {
                self.dirs.insert(dir, Listing::Failed(say(&e)));
            }
            Message::Open(path) => return self.open(repository, &path, None, settings),
            Message::Find(message) => return self.find_update(repository, message, settings),
            Message::Read(path, result) => {
                let Some(file) = self.file_mut(&path) else {
                    return Task::none();
                };
                let goto = file.goto.take();
                file.body = match result {
                    // The text shown already: the cursor stays where it is.
                    Ok(FileDetail {
                        text: Some(text), ..
                    }) if matches!(&file.body, Body::Text { content, .. } if content.text() == text) =>
                    {
                        if let Body::Text { saved, .. } = &mut file.body {
                            *saved = text;
                        }
                        file.goto = goto;
                        file.go_to_line();
                        return Task::none();
                    }
                    Ok(detail) => {
                        file.size = detail.size;
                        match detail.text {
                            Some(text) => Body::Text {
                                content: text_editor::Content::with_text(&text),
                                saved: text,
                            },
                            None if detail.too_large => Body::TooLarge,
                            None => Body::Binary,
                        }
                    }
                    Err(e) => Body::Failed(say(&e)),
                };
                file.goto = goto;
                file.go_to_line();
            }
            Message::Edit(action) => {
                let editing = action.is_edit();
                let current = self.tab.clone();
                if let Tab::Editor(path) = current
                    && let Some(file) = self.file_mut(&path)
                    && let Body::Text { content, .. } = &mut file.body
                {
                    content.perform(action);
                    if editing {
                        file.committed = false;
                        file.error = None;
                    }
                }
            }
            Message::Save => {
                let Tab::Editor(current) = self.tab.clone() else {
                    return Task::none();
                };
                let Some(file) = self.file_mut(&current) else {
                    return Task::none();
                };
                let Body::Text { content, .. } = &file.body else {
                    return Task::none();
                };
                if file.saving {
                    return Task::none();
                }
                file.saving = true;
                file.error = None;
                let (client, path, text) = (
                    Client::new(settings.clone()),
                    file.path.clone(),
                    content.text(),
                );
                return Task::perform(
                    async move {
                        let saved = client.save_file(repository, &path, &text).await;
                        (path, text, saved)
                    },
                    move |(path, text, saved)| {
                        AppMessage::Project(repository, Message::Saved(path, text, saved))
                    },
                );
            }
            Message::Saved(path, sent, result) => {
                let Some(file) = self.file_mut(&path) else {
                    return Task::none();
                };
                file.saving = false;
                match result {
                    Ok(detail) => {
                        file.size = detail.size;
                        file.committed = true;
                        if let Body::Text { saved, .. } = &mut file.body {
                            *saved = sent;
                        }
                    }
                    Err(e) => file.error = Some(say(&e)),
                }
            }
        }
        Task::none()
    }

    /// Opens `path` in an editor tab, with the cursor on `line` (from 1) when it is given, reading the file if it is
    /// not open yet.
    pub(super) fn open(
        &mut self,
        repository: i64,
        path: &str,
        line: Option<usize>,
        settings: &Settings,
    ) -> Task<AppMessage> {
        self.push_tab(Tab::Editor(path.to_string()));
        self.origin = None;
        self.recent.retain(|other| other != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(RECENT_FILES);
        if let Some(file) = self.file_mut(path) {
            file.goto = line;
            file.go_to_line();
            return Task::none();
        }
        self.files.push(OpenFile {
            path: path.to_string(),
            size: 0,
            body: Body::Reading,
            error: None,
            saving: false,
            committed: false,
            goto: line,
        });
        Self::read(repository, path, settings)
    }
}
