//! A project: the page of one repository. Under its header stand the tabs: Board, then the ones the person opened,
//! each with a cross, and a `+` that offers an editor, a terminal in the repository's clone, an agent chat in the
//! project, and later a browser. The Files sidebar on the right shows the repository's default branch as a tree whose
//! directories are read when they are first opened (`GET /v1/repositories/{id}/tree`); a click on a file opens it in an
//! editor tab at once (or shows the tab it is open in). An editor highlights its file by the extension and saves it
//! with `PUT /v1/repositories/{id}/files/{path}`, a commit on the default branch. A terminal tab is a shell on the
//! server (`GET /v1/repositories/{id}/terminal`); a chat tab is a chat of the Chat page, shown inside the project.
//! The open tabs are kept for the project while the app runs. What is shown tells the floating chat what "this" is.
//!
//! `tabs` has how the tabs are opened, shown and closed, `update` how a message changes the files, `shell` the terminal
//! tabs, `strip` the tabs and the `+` menu, `view` the page and the sidebar, `context` what the floating chat is told,
//! and `editor` an editor tab. `find` is "Go to file" and "Search in files", which open over every tab and open a
//! file, at a line when a hit was chosen; `fuzzy` ranks the paths for the first.

mod context;
mod editor;
pub mod find;
mod fuzzy;
mod shell;
mod strip;
mod tabs;
mod update;
mod view;

use std::collections::{BTreeMap, BTreeSet};

use iced::widget::text_editor;
use iced::{Task, keyboard};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, FileDetail, TerminalEvent, Tree, TreeEntry};
use crate::settings::Settings;
use crate::widgets::terminal::PaneEvent;

pub use find::key as find_key;
pub use shell::terminal_key;
pub use strip::tabs;
pub use view::sidebar;
pub(crate) use view::size;

use shell::TerminalTab;

/// A tab of a project's page. Board is always there; the others are opened and closed by the person.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Board,
    /// The editor of the file with this path.
    Editor(String),
    /// The terminal with this number, which is the number of its opening.
    Terminal(u32),
    /// The chat tab with this number.
    Chat(u32),
}

#[derive(Debug, Clone)]
pub enum Message {
    /// A tab was clicked.
    Tab(Tab),
    /// The cross of a tab: a file with unsaved changes asks first.
    Close(Tab),
    /// Closing an unsaved file was confirmed; its changes are dropped.
    Discard(String),
    /// Closing an unsaved file was called off.
    Keep,
    /// The button at the right of the tabs: the Files sidebar folds away or comes back.
    ToggleFiles,
    /// The `+` beside the tabs: its menu opens or closes.
    Menu,
    /// Editor in the menu: "Go to file" opens, and a file picked opens as a tab.
    NewEditor,
    /// Terminal in the menu: a shell in the repository's clone opens as a tab.
    NewTerminal,
    /// Agent chat in the menu: a new chat in this project opens as a tab.
    NewChat,
    /// A directory of the tree was clicked: it opens, read the first time, or closes.
    Toggle(String),
    /// The server listed a directory, or said why not.
    Listed(String, Result<Tree, ClientError>),
    /// A file of the tree was clicked: it opens in an editor tab.
    Open(String),
    /// The server answered reading a file.
    Read(String, Result<FileDetail, ClientError>),
    Edit(text_editor::Action),
    /// Save the file shown, from its button or Ctrl+S.
    Save,
    /// The server answered saving the file at the path with the text sent.
    Saved(String, String, Result<FileDetail, ClientError>),
    /// Something of "Go to file" or "Search in files".
    Find(find::Message),
    /// Something the connection of the terminal with this number did.
    Shell(u32, TerminalEvent),
    /// The pane of the terminal with this number was laid out at a size, or a click gave it focus or took it away.
    Pane(u32, PaneEvent),
    /// A key nothing else took, for the terminal shown if its pane has focus.
    Key(keyboard::Event),
}

/// A directory of the tree as far as it is known.
#[derive(Debug)]
enum Listing {
    Reading,
    Read(Vec<TreeEntry>),
    Failed(String),
}

/// What an editor tab holds of an open file.
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
    /// The line, from 1, the cursor goes to once the text is read.
    goto: Option<usize>,
}

impl OpenFile {
    fn unsaved(&self) -> bool {
        matches!(&self.body, Body::Text { content, saved } if content.text() != *saved)
    }

    fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// The largest file an editor edits, as the server reads it.
const TEXT_LIMIT: &str = "1 MB";

/// The tabs of one repository's page and the files it has read.
#[derive(Default)]
pub struct Project {
    tab: Tab,
    /// The tabs after Board, in the order they were opened.
    opened: Vec<Tab>,
    /// Whether the Files sidebar is folded away.
    files_folded: bool,
    /// Whether the menu of the `+` is open.
    menu: bool,
    /// How many terminals and chat tabs were opened, which numbers the next.
    counter: u32,
    terminals: Vec<TerminalTab>,
    /// The chat of each chat tab by its number; none while the chat is not started.
    chats: Vec<(u32, Option<i64>)>,
    dirs: BTreeMap<String, Listing>,
    open_dirs: BTreeSet<String>,
    /// The files with an editor tab.
    files: Vec<OpenFile>,
    /// The unsaved file whose closing waits for a confirmation.
    closing: Option<String>,
    /// The panel of "Go to file" or "Search in files", when one is open.
    find: Option<find::Find>,
    /// Every path of the default branch, as the server last listed them for "Go to file".
    known_paths: Vec<String>,
    /// The files opened lately, the latest first, which "Go to file" ranks first.
    recent: Vec<String>,
    /// How the file shown was opened when it came from finding.
    origin: Option<find::Origin>,
}

/// Ctrl+S (Cmd+S on macOS) saves the file the editor shows; the app ignores it elsewhere.
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
    /// Whether a file open in an editor tab has changes that are not saved.
    pub fn unsaved(&self) -> bool {
        self.files.iter().any(OpenFile::unsaved)
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Listed(_, Err(ClientError::Unauthorized))
                | Message::Read(_, Err(ClientError::Unauthorized))
                | Message::Saved(_, _, Err(ClientError::Unauthorized))
                | Message::Find(
                    find::Message::Paths(Err(ClientError::Unauthorized))
                        | find::Message::Found(_, Err(ClientError::Unauthorized))
                )
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

    /// The path of the file of the editor tab shown.
    fn current_path(&self) -> Option<&str> {
        match &self.tab {
            Tab::Editor(path) => Some(path),
            _ => None,
        }
    }

    /// The file of the editor tab shown.
    fn shown(&self) -> Option<&OpenFile> {
        match &self.tab {
            Tab::Editor(path) => self.file(path),
            _ => None,
        }
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
