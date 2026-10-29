//! How a message changes a project's Files and Editor tabs: a directory listed, a file read, opened, edited, saved
//! or closed.

use iced::Task;
use iced::widget::text_editor;

use crate::app::Message as AppMessage;
use crate::client::{Client, FileDetail};
use crate::screens::live::say;
use crate::settings::Settings;

use super::{Body, Listing, Message, OpenFile, Project, Tab};

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
            Message::Tab(tab) => {
                self.tab = tab;
                if tab == Tab::Files {
                    return self.list(repository, "", settings);
                }
            }
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
            Message::Open(path) => {
                self.tab = Tab::Editor;
                self.current = Some(path.clone());
                if self.file(&path).is_none() {
                    self.files.push(OpenFile {
                        path: path.clone(),
                        size: 0,
                        body: Body::Reading,
                        error: None,
                        saving: false,
                        committed: false,
                    });
                    return Self::read(repository, &path, settings);
                }
            }
            Message::Read(path, result) => {
                let Some(file) = self.file_mut(&path) else {
                    return Task::none();
                };
                file.body = match result {
                    // The text shown already: the cursor stays where it is.
                    Ok(FileDetail {
                        text: Some(text), ..
                    }) if matches!(&file.body, Body::Text { content, .. } if content.text() == text) =>
                    {
                        if let Body::Text { saved, .. } = &mut file.body {
                            *saved = text;
                        }
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
            }
            Message::Show(path) => {
                self.current = Some(path);
                self.closing = None;
            }
            Message::Close(path) => {
                if self.file(&path).is_some_and(OpenFile::unsaved) {
                    self.current = Some(path.clone());
                    self.closing = Some(path);
                } else {
                    self.close(&path);
                }
            }
            Message::Discard(path) => self.close(&path),
            Message::Keep => self.closing = None,
            Message::Edit(action) => {
                let editing = action.is_edit();
                let current = self.current.clone().unwrap_or_default();
                if let Some(file) = self.file_mut(&current)
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
                let current = self.current.clone().unwrap_or_default();
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

    pub(super) fn close(&mut self, path: &str) {
        let Some(index) = self.files.iter().position(|file| file.path == path) else {
            return;
        };
        self.files.remove(index);
        if self.closing.as_deref() == Some(path) {
            self.closing = None;
        }
        if self.current.as_deref() == Some(path) {
            let next = index.min(self.files.len().saturating_sub(1));
            self.current = self.files.get(next).map(|file| file.path.clone());
        }
    }
}
