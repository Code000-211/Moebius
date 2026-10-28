//! A project: the page of one repository, with the tabs Board, Files and Editor. The board is `board.rs`; this module
//! holds the other two. Files lists the repository's default branch as a tree whose directories are read when they
//! are first opened (`GET /v1/repositories/{id}/tree`); a click on a file opens it in the Editor, which keeps the open
//! files as tabs, highlights them by their extension and saves one with `PUT /v1/repositories/{id}/files/{path}`, a
//! commit on the default branch. Where the cursor is and what is selected tell the floating chat what "this" is.

use std::collections::{BTreeMap, BTreeSet};

use iced::advanced::text::highlighter::Format;
use iced::font::Weight;
use iced::highlighter;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, text, text_editor};
use iced::{Alignment, Border, Element, Length, Padding, Task, keyboard};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, FileDetail, Repository, Tree, TreeEntry};
use crate::icons::Icon;
use crate::live::say;
use crate::page;
use crate::settings::Settings;
use crate::{theme, transcript};

/// The tabs at the top of a project's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Board,
    Files,
    Editor,
}

impl Tab {
    pub fn title(self) -> &'static str {
        match self {
            Tab::Board => "Board",
            Tab::Files => "Files",
            Tab::Editor => "Editor",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Tab(Tab),
    /// A directory of the tree was clicked: it opens, read the first time, or closes.
    Toggle(String),
    /// The server listed a directory, or said why not.
    Listed(String, Result<Tree, ClientError>),
    /// A file of the tree was clicked: it opens in the Editor.
    Open(String),
    /// The server answered reading a file.
    Read(String, Result<FileDetail, ClientError>),
    /// The tab of an open file was clicked.
    Show(String),
    /// The cross of a file's tab: an unsaved file asks first.
    Close(String),
    /// Closing an unsaved file was confirmed; its changes are dropped.
    Discard(String),
    /// Closing an unsaved file was called off.
    Keep,
    Edit(text_editor::Action),
    /// Save the file shown, from its button or Ctrl+S.
    Save,
    /// The server answered saving the file at the path with the text sent.
    Saved(String, String, Result<FileDetail, ClientError>),
}

/// A directory of the tree as far as it is known.
#[derive(Debug)]
enum Listing {
    Reading,
    Read(Vec<TreeEntry>),
    Failed(String),
}

/// What the Editor holds of an open file.
enum Body {
    Reading,
    /// The text being edited, and the text as the default branch has it, which tells whether it is saved.
    Text {
        content: text_editor::Content,
        saved: String,
    },
    Binary,
    TooLarge,
    Failed(String),
}

struct OpenFile {
    path: String,
    size: u64,
    body: Body,
    /// The refusal or failure of the last save.
    error: Option<String>,
    saving: bool,
    /// Whether the last save went through and nothing has changed since.
    committed: bool,
}

impl OpenFile {
    fn unsaved(&self) -> bool {
        matches!(&self.body, Body::Text { content, saved } if content.text() != *saved)
    }

    fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// The largest file the Editor edits, as the server reads it.
const TEXT_LIMIT: &str = "1 MB";

/// How many lines around the cursor, and how many characters in all, the floating chat is told of the file shown.
const CONTEXT_LINES: usize = 40;
const CONTEXT_CHARS: usize = 3_000;
/// How many characters of the selection the floating chat is told.
const SELECTION_CHARS: usize = 600;

/// The Files and Editor tabs of one repository, and the tab shown.
#[derive(Default)]
pub struct Project {
    tab: Tab,
    dirs: BTreeMap<String, Listing>,
    open_dirs: BTreeSet<String>,
    files: Vec<OpenFile>,
    /// The path of the file shown in the Editor.
    current: Option<String>,
    /// The unsaved file whose closing waits for a confirmation.
    closing: Option<String>,
}

/// `text` cut to `limit` characters, saying so when it was cut.
fn cut(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_string();
    }
    let start: String = text.chars().take(limit).collect();
    format!("{start}\n[cut: the first {limit} of {count} characters]")
}

/// The language the highlighter is given for a file: its extension, or its name when it has none.
fn token(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name
        .rsplit_once('.')
        .map_or(name, |(_, extension)| extension);
    transcript::language(extension).to_string()
}

/// Ctrl+S (Cmd+S on macOS) saves the file the Editor shows; the app ignores it elsewhere.
pub fn key(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<AppMessage> {
    match event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
            if modifiers.command() && key.as_ref() == keyboard::Key::Character("s") =>
        {
            Some(AppMessage::SaveFile)
        }
        _ => None,
    }
}

impl Project {
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// Whether a file open in the Editor has changes that are not saved.
    pub fn unsaved(&self) -> bool {
        self.files.iter().any(OpenFile::unsaved)
    }

    /// Shows the tab without reading anything, as when a card of the board opens.
    pub fn show_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Listed(_, Err(ClientError::Unauthorized))
                | Message::Read(_, Err(ClientError::Unauthorized))
                | Message::Saved(_, _, Err(ClientError::Unauthorized))
        )
    }

    /// Whether `path` is open and has no changes of its own, so that a change on the server can be read again.
    pub fn follows(&self, path: &str) -> bool {
        self.files
            .iter()
            .any(|file| file.path == path && !file.unsaved() && !file.saving)
    }

    fn file(&self, path: &str) -> Option<&OpenFile> {
        self.files.iter().find(|file| file.path == path)
    }

    fn file_mut(&mut self, path: &str) -> Option<&mut OpenFile> {
        self.files.iter_mut().find(|file| file.path == path)
    }

    fn shown(&self) -> Option<&OpenFile> {
        self.current.as_deref().and_then(|path| self.file(path))
    }

    /// The line and column of the cursor in the file shown, from 1.
    fn cursor(&self) -> Option<(usize, usize)> {
        match &self.shown()?.body {
            Body::Text { content, .. } => {
                let position = content.cursor().position;
                Some((position.line + 1, position.column + 1))
            }
            _ => None,
        }
    }

    /// Reads a directory, unless it is read or being read.
    fn list(&mut self, repository: i64, dir: &str, settings: &Settings) -> Task<AppMessage> {
        if matches!(
            self.dirs.get(dir),
            Some(Listing::Reading | Listing::Read(_))
        ) {
            return Task::none();
        }
        self.dirs.insert(dir.to_string(), Listing::Reading);
        let (client, dir) = (Client::new(settings.clone()), dir.to_string());
        Task::perform(
            async move {
                let tree = client.tree(repository, &dir).await;
                (dir, tree)
            },
            move |(dir, tree)| AppMessage::Project(repository, Message::Listed(dir, tree)),
        )
    }

    /// Reads a file into its tab, also again when it changed on the default branch.
    pub fn read(repository: i64, path: &str, settings: &Settings) -> Task<AppMessage> {
        let (client, path) = (Client::new(settings.clone()), path.to_string());
        Task::perform(
            async move {
                let file = client.file(repository, &path).await;
                (path, file)
            },
            move |(path, file)| AppMessage::Project(repository, Message::Read(path, file)),
        )
    }

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

    fn close(&mut self, path: &str) {
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

    /// What the floating chat is told of the tab shown, below the repository's line.
    pub fn context(&self) -> Vec<String> {
        match self.tab {
            Tab::Board => Vec::new(),
            Tab::Files => {
                let open: Vec<&str> = self.open_dirs.iter().map(String::as_str).collect();
                vec![match open.is_empty() {
                    true => "Browsing the files of the default branch, at the root.".to_string(),
                    false => format!(
                        "Browsing the files of the default branch; open directories: {}",
                        open.join(", ")
                    ),
                }]
            }
            Tab::Editor => self.editor_context(),
        }
    }

    fn editor_context(&self) -> Vec<String> {
        let Some(file) = self.shown() else {
            return vec!["The Editor has no file open.".to_string()];
        };
        let mut lines = vec![format!(
            "Open file: {}{}",
            file.path,
            if file.unsaved() {
                " (unsaved changes)"
            } else {
                ""
            }
        )];
        let others: Vec<&str> = self
            .files
            .iter()
            .filter(|other| other.path != file.path)
            .map(|other| other.path.as_str())
            .collect();
        if !others.is_empty() {
            lines.push(format!("Also open: {}", others.join(", ")));
        }
        let content = match &file.body {
            Body::Text { content, .. } => content,
            Body::Reading => return lines,
            Body::Binary => {
                lines.push("It is a binary file and is not shown.".to_string());
                return lines;
            }
            Body::TooLarge => {
                lines.push(format!("It is larger than {TEXT_LIMIT} and is not shown."));
                return lines;
            }
            Body::Failed(error) => {
                lines.push(format!("It could not be read: {error}"));
                return lines;
            }
        };
        let cursor = content.cursor().position;
        lines.push(format!(
            "Cursor: line {}, column {}",
            cursor.line + 1,
            cursor.column + 1
        ));
        if let Some(selection) = content.selection().filter(|s| !s.is_empty()) {
            lines.push(format!(
                "Selected text:\n{}",
                cut(&selection, SELECTION_CHARS)
            ));
        }
        let first = cursor.line.saturating_sub(CONTEXT_LINES / 2);
        let last = (first + CONTEXT_LINES).min(content.line_count());
        let around: Vec<String> = (first..last)
            .filter_map(|index| {
                let line = content.line(index)?;
                Some(format!("{:>5} | {}", index + 1, line.text))
            })
            .collect();
        let used: usize = lines.iter().map(|line| line.chars().count() + 1).sum();
        lines.push(format!(
            "Lines {}-{} of {}:\n{}",
            first + 1,
            last,
            content.line_count(),
            cut(
                &around.join("\n"),
                CONTEXT_CHARS.saturating_sub(used).max(500)
            )
        ));
        lines
    }

    /// The Files or Editor tab of `repository`'s page, under the header and the tabs the app gives.
    pub fn view<'a>(
        &'a self,
        repository: &'a Repository,
        tabs: Element<'a, AppMessage>,
    ) -> Element<'a, AppMessage> {
        let id = repository.id;
        let (context, actions): (&str, Element<'a, AppMessage>) = match self.tab {
            Tab::Editor => {
                let can_save = self
                    .shown()
                    .is_some_and(|file| file.unsaved() && !file.saving);
                (
                    "A save is a commit on the default branch.",
                    page::primary_button(
                        "Save",
                        can_save.then_some(AppMessage::Project(id, Message::Save)),
                    ),
                )
            }
            _ => (
                "The default branch. A click on a file opens it in the Editor.",
                space().into(),
            ),
        };
        let header = page::header(
            "Projects",
            &repository.name,
            Some(match &repository.github {
                Some(github) => format!("GitHub · {github}"),
                None => "No forge".to_string(),
            }),
            page::context(context),
            actions,
        );
        let body = match self.tab {
            Tab::Editor => self.editor_view(id),
            _ => self.files_view(id),
        };
        column![header, tabs, body]
            .spacing(theme::PAGE_GAP)
            .width(Length::Fill)
            .into()
    }

    fn files_view(&self, repository: i64) -> Element<'_, AppMessage> {
        let rows: Vec<Element<'_, AppMessage>> = match self.dirs.get("") {
            None | Some(Listing::Reading) => {
                vec![note_row(
                    0,
                    "Reading the default branch.",
                    theme::TEXT_MUTED,
                )]
            }
            Some(Listing::Failed(error)) => vec![note_row(0, error.as_str(), theme::RED_TEXT)],
            Some(Listing::Read(entries)) if entries.is_empty() => {
                vec![note_row(
                    0,
                    "The repository has no files.",
                    theme::TEXT_MUTED,
                )]
            }
            Some(Listing::Read(entries)) => self.interleave(repository, entries, 0),
        };
        container(scrollable(column(rows).spacing(1)).height(Length::Fill))
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| page::box_style())
            .into()
    }

    /// The rows of `entries` at `depth`, each open directory followed by its own rows.
    fn interleave<'a>(
        &'a self,
        repository: i64,
        entries: &'a [TreeEntry],
        depth: usize,
    ) -> Vec<Element<'a, AppMessage>> {
        let mut rows = Vec::new();
        for entry in entries {
            let open = self.open_dirs.contains(&entry.path);
            rows.push(entry_row(repository, entry, depth, open));
            if entry.kind != "dir" || !open {
                continue;
            }
            match self.dirs.get(&entry.path) {
                None | Some(Listing::Reading) => {
                    rows.push(note_row(depth + 1, "Reading.", theme::TEXT_MUTED));
                }
                Some(Listing::Failed(error)) => {
                    rows.push(note_row(depth + 1, error.as_str(), theme::RED_TEXT));
                }
                Some(Listing::Read(children)) if children.is_empty() => {
                    rows.push(note_row(depth + 1, "Empty.", theme::TEXT_MUTED));
                }
                Some(Listing::Read(children)) => {
                    rows.extend(self.interleave(repository, children, depth + 1));
                }
            }
        }
        rows
    }

    fn editor_view(&self, repository: i64) -> Element<'_, AppMessage> {
        let Some(file) = self.shown() else {
            return container(page::body("No file is open. Open one from Files."))
                .padding(20)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| page::box_style())
                .into();
        };
        let tabs = row(self.files.iter().map(|open| {
            file_tab(
                repository,
                open,
                Some(open.path.as_str()) == self.current.as_deref(),
            )
        }))
        .spacing(6)
        .wrap();
        let label = text(transcript_label(&file.path))
            .size(theme::LABEL)
            .color(theme::TEXT_MUTED);
        let path_line = row![
            text(file.path.as_str())
                .size(theme::CODE)
                .font(theme::MONO)
                .color(theme::TEXT_SECONDARY),
            space::horizontal(),
            label,
        ]
        .align_y(Alignment::Center);
        let main: Element<'_, AppMessage> = match &file.body {
            Body::Text { content, .. } => code_editor(repository, &file.path, content),
            Body::Reading => page::body("Reading the file."),
            Body::Binary => page::body(format!(
                "{} is a binary file ({} bytes); it is not edited here.",
                file.name(),
                file.size
            )),
            Body::TooLarge => page::body(format!(
                "{} is larger than {TEXT_LIMIT} ({} bytes); it is not edited here.",
                file.name(),
                file.size
            )),
            Body::Failed(error) => text(error.as_str())
                .size(theme::CONTEXT)
                .color(theme::RED_TEXT)
                .into(),
        };
        let mut status = row![].spacing(14).align_y(Alignment::Center);
        if let Some((line, column)) = self.cursor() {
            status = status.push(
                text(format!("Ln {line}, Col {column}"))
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
            );
        }
        let notice = if self.closing.as_deref() == Some(file.path.as_str()) {
            status = status
                .push(
                    text(format!("Close {} without saving?", file.name()))
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY),
                )
                .push(page::white_button(
                    "Keep",
                    Some(AppMessage::Project(repository, Message::Keep)),
                ))
                .push(page::white_button(
                    "Close without saving",
                    Some(AppMessage::Project(
                        repository,
                        Message::Discard(file.path.clone()),
                    )),
                ));
            None
        } else if let Some(error) = &file.error {
            Some((error.as_str(), theme::RED_TEXT))
        } else if file.saving {
            Some(("Saving.", theme::TEXT_MUTED))
        } else if file.unsaved() {
            Some(("Unsaved changes.", theme::AMBER_TEXT))
        } else if file.committed {
            Some((
                "Saved as a commit on the default branch.",
                theme::ONLINE_TEXT,
            ))
        } else {
            None
        };
        if let Some((line, color)) = notice {
            status = status.push(
                text(line)
                    .size(theme::CONTEXT)
                    .font(theme::manrope(Weight::Semibold))
                    .color(color)
                    .wrapping(Wrapping::WordOrGlyph),
            );
        }
        column![
            tabs,
            container(column![path_line, main, status].spacing(12))
                .padding(20)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| page::box_style()),
        ]
        .spacing(10)
        .into()
    }
}

/// What the language label of the Editor says of a file.
fn transcript_label(path: &str) -> String {
    match token(path).as_str() {
        "rs" => "Rust".to_string(),
        "lua" => "Lua".to_string(),
        "py" => "Python".to_string(),
        "js" => "JavaScript".to_string(),
        "md" => "Markdown".to_string(),
        "sh" => "Shell".to_string(),
        "ini" => "TOML".to_string(),
        other => other.to_uppercase(),
    }
}

/// A line of the tree that is not an entry: reading, failed or empty.
fn note_row<'a>(depth: usize, line: &'a str, color: iced::Color) -> Element<'a, AppMessage> {
    container(text(line).size(theme::CONTEXT).color(color))
        .padding(Padding {
            top: 6.0,
            bottom: 6.0,
            left: 10.0 + 18.0 * depth as f32 + 22.0,
            right: 10.0,
        })
        .into()
}

/// A directory or a file of the tree, indented by its depth.
fn entry_row<'a>(
    repository: i64,
    entry: &'a TreeEntry,
    depth: usize,
    open: bool,
) -> Element<'a, AppMessage> {
    let dir = entry.kind == "dir";
    let chevron: Element<'a, AppMessage> = if dir {
        let icon = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        icon.view(theme::TEXT_MUTED, 14.0).into()
    } else {
        space().width(14).into()
    };
    let icon = if dir { Icon::Folder } else { Icon::File };
    let mut line = row![
        chevron,
        icon.view(theme::TEXT_SECONDARY, 16.0),
        text(entry.name.as_str())
            .size(theme::CONTEXT)
            .font(if dir {
                theme::manrope(Weight::Semibold)
            } else {
                theme::MANROPE
            })
            .color(theme::TEXT),
        space::horizontal(),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !dir {
        line = line.push(
            text(size(entry.size))
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED),
        );
    }
    let message = if dir {
        Message::Toggle(entry.path.clone())
    } else {
        Message::Open(entry.path.clone())
    };
    container(
        button(line)
            .padding(Padding {
                top: 6.0,
                bottom: 6.0,
                left: 10.0 + 18.0 * depth as f32,
                right: 10.0,
            })
            .width(Length::Fill)
            .on_press(AppMessage::Project(repository, message))
            .style(|_, status| button::Style {
                background: (status == button::Status::Hovered).then(|| theme::LANE.into()),
                border: Border {
                    radius: theme::RADIUS_SMALL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    )
    .id(format!("tree-{}", entry.path))
    .into()
}

/// A size in bytes as the tree writes it: `512 B`, `4.2 KB`, `1.3 MB`.
fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1_000_000 => format!("{:.1} MB", b as f64 / 1_000_000.0),
        b if b >= 1_000 => format!("{:.1} KB", b as f64 / 1_000.0),
        b => format!("{b} B"),
    }
}

/// The tab of an open file: its name, a dot while it is unsaved, and the cross that closes it.
fn file_tab<'a>(repository: i64, file: &'a OpenFile, current: bool) -> Element<'a, AppMessage> {
    let mut label = row![
        text(file.name())
            .size(theme::BREADCRUMB)
            .font(theme::MONO)
            .color(if current {
                theme::TEXT
            } else {
                theme::TEXT_SECONDARY
            })
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if file.unsaved() {
        label = label.push(
            container(space().width(theme::DOT).height(theme::DOT)).style(|_| container::Style {
                background: Some(theme::AMBER.into()),
                border: Border {
                    radius: (theme::DOT / 2.0).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
        );
    }
    let close = container(
        button(Icon::Close.view(theme::TEXT_MUTED, 12.0))
            .padding(2)
            .on_press(AppMessage::Project(
                repository,
                Message::Close(file.path.clone()),
            ))
            .style(|_, status| button::Style {
                background: (status == button::Status::Hovered).then(|| theme::TRACK.into()),
                border: Border {
                    radius: theme::RADIUS_SMALL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    )
    .id(format!("close-{}", file.path));
    let open = button(label)
        .padding(0)
        .on_press(AppMessage::Project(
            repository,
            Message::Show(file.path.clone()),
        ))
        .style(|_, _| button::Style::default());
    container(row![open, close].spacing(8).align_y(Alignment::Center))
        .padding([6, 10])
        .style(move |_| container::Style {
            background: Some(if current { theme::SURFACE } else { theme::LANE }.into()),
            border: Border {
                color: if current {
                    theme::TEAL_BORDER
                } else {
                    theme::LINE
                },
                width: 1.0,
                radius: theme::RADIUS_CONTROL.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// The file in the code font, highlighted by its extension.
fn code_editor<'a>(
    repository: i64,
    path: &str,
    content: &'a text_editor::Content,
) -> Element<'a, AppMessage> {
    text_editor(content)
        .on_action(move |action| AppMessage::Project(repository, Message::Edit(action)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding(14)
        .height(Length::Fill)
        .highlight_with::<highlighter::Highlighter>(
            highlighter::Settings {
                theme: highlighter::Theme::InspiredGitHub,
                token: token(path),
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

/// The tabs Board, Files and Editor at the top of a project's page; the Editor's says how many files are open.
pub fn tabs<'a>(repository: i64, project: Option<&Project>) -> Element<'a, AppMessage> {
    let shown = project.map_or(Tab::Board, Project::tab);
    let open = project.map_or(0, |project| project.files.len());
    let tab = |tab: Tab| -> Element<'a, AppMessage> {
        let selected = tab == shown;
        let label = match (tab, open) {
            (Tab::Editor, n) if n > 0 => format!("Editor · {n}"),
            (tab, _) => tab.title().to_string(),
        };
        let underline = container(space())
            .width(Length::Fill)
            .height(2)
            .style(move |_| container::Style {
                background: selected.then(|| theme::TEAL.into()),
                ..container::Style::default()
            });
        container(
            button(
                column![
                    text(label)
                        .size(theme::CONTEXT)
                        .font(theme::manrope(if selected {
                            Weight::Bold
                        } else {
                            Weight::Semibold
                        }))
                        .color(if selected {
                            theme::TEXT
                        } else {
                            theme::TEXT_SECONDARY
                        }),
                    underline,
                ]
                .spacing(6)
                .width(Length::Shrink),
            )
            .padding(Padding {
                top: 2.0,
                bottom: 0.0,
                left: 2.0,
                right: 2.0,
            })
            .on_press(AppMessage::Project(repository, Message::Tab(tab)))
            .style(|_, _| button::Style::default()),
        )
        .id(format!("project-tab-{}", tab.title()))
        .into()
    };
    column![
        row![tab(Tab::Board), tab(Tab::Files), tab(Tab::Editor)].spacing(22),
        page::rule(),
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_highlighted_by_its_extension_or_its_name() {
        assert_eq!(token("src/main.rs"), "rs");
        assert_eq!(token(".moebius/ready.luau"), "lua");
        assert_eq!(token("Cargo.toml"), "ini");
        assert_eq!(token("Makefile"), "Makefile");
        assert_eq!(transcript_label("a/b.rs"), "Rust");
    }

    #[test]
    fn a_size_is_written_short() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(4_200), "4.2 KB");
        assert_eq!(size(1_300_000), "1.3 MB");
    }

    #[test]
    fn a_long_text_is_cut_and_says_so() {
        assert_eq!(cut("short", 10), "short");
        assert_eq!(cut("abcdef", 3), "abc\n[cut: the first 3 of 6 characters]");
    }
}
