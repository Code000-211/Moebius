//! A project: the page of one repository, with the tabs Board, Files and Editor. The board is `board.rs`; this module
//! holds the other two. Files lists the repository's default branch as a tree whose directories are read when they
//! are first opened (`GET /v1/repositories/{id}/tree`); a click on a file opens it in the Editor, which keeps the open
//! files as tabs, highlights them by their extension and saves one with `PUT /v1/repositories/{id}/files/{path}`, a
//! commit on the default branch. Where the cursor is and what is selected tell the floating chat what "this" is.
//!
//! `update` has how a message changes the tabs, `context` what the floating chat is told of them, `view` the tabs and
//! the Files tab, and `editor` the Editor tab.

mod context;
mod editor;
mod update;
mod view;

use std::collections::{BTreeMap, BTreeSet};

use iced::widget::text_editor;
use iced::{Task, keyboard};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, FileDetail, Tree, TreeEntry};
use crate::settings::Settings;

pub(crate) use view::size;
pub use view::tabs;

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
}
