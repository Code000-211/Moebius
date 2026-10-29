//! The Rules page: the built-in patterns and what each refused this week, the server's rules in prose and the rules
//! each repository adds. Every rule in prose is a card with its name, its text and a pencil; the pencil grows the card
//! in place into an editor of the rule's name and text with Save, Cancel and Delete (which asks first). Under the last
//! rule an empty "New rule" card with a pencil adds one. A save puts the rule's `## name` section back into its file
//! (or appends it, or drops it) and saves the whole file: the server's with `PUT /v1/rules`, which the server checks
//! and commits, a repository's `.moebius/rules.md` with `PUT /v1/repositories/{id}/files/.moebius/rules.md`, a commit
//! on its default branch that the server checks first. "Let the Moebius agent write them" asks the Moebius agent
//! instead, in the floating chat.
//!
//! `file` puts a rule back into its file's text, `update` has how a message changes the page, `view` the boxes of the
//! page and `cards` the rules in them as cards and the editor a card grows into.

mod cards;
mod file;
mod update;
mod view;

use iced::Task;
use iced::widget::text_editor;

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Rule, RulesView, Run};
use crate::settings::Settings;

use file::section;

pub use file::{append_rule, replace_rule};

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<RulesView, ClientError>),
    /// "Let the Moebius agent write them" (true) or its Cancel (false) was pressed.
    ByAgent(bool),
    WishEdited(text_editor::Action),
    /// "Ask the agent" was pressed.
    AskAgent,
    /// The server started the Moebius agent's chat that changes the rules, or said why not.
    Asked(Result<Run, ClientError>),
    /// A pencil: the rule with this name of the server or of a repository is edited in place; without a name, the
    /// "New rule" card is.
    EditRule(Place, Option<String>),
    NameEdited(String),
    TextEdited(text_editor::Action),
    SaveRule,
    CancelRule,
    /// "Delete rule" was pressed: the card asks first.
    AskDelete,
    /// The deletion was called off.
    KeepRule,
    /// The deletion was confirmed: the rule's section is dropped from the file, which is saved.
    DeleteRule,
    /// The server answered the save of a rules file: the short id of the commit of the server's rules (none when the
    /// text changed nothing, and for a repository's), or why not.
    RuleSaved(Result<Option<String>, ClientError>),
}

/// Whose rules file a rule belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Server,
    /// The repository with this id.
    Repository(i64),
}

/// A rule as it is being edited in its card.
struct Editing {
    place: Place,
    /// The rule's name as the file has it; none for a new rule.
    rule: Option<String>,
    name: String,
    text: text_editor::Content,
    /// Whether Delete was pressed and waits for a confirmation.
    deleting: bool,
    saving: bool,
    error: Option<String>,
}

enum Notice {
    /// The save is a commit with this short id; none when the server does not say, or the text changed nothing.
    Saved(Option<String>),
    Failed(String),
}

#[derive(Default)]
pub struct Rules {
    view: Option<RulesView>,
    /// What came of the last save, and whose file it was.
    notice: Option<(Place, Notice)>,
    /// Whether the request to the Moebius agent is open.
    by_agent: bool,
    wish: text_editor::Content,
    /// Whether the request to the agent is on its way.
    asking: bool,
    editing: Option<Editing>,
}

impl Rules {
    /// Reads the rules again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        Task::perform(async move { client.rules().await }, |result| {
            AppMessage::Rules(Message::Loaded(result))
        })
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Loaded(Err(ClientError::Unauthorized))
                | Message::Asked(Err(ClientError::Unauthorized))
                | Message::RuleSaved(Err(ClientError::Unauthorized))
        )
    }

    /// What the floating chat tells the Moebius agent of the page: the rule being edited, or the server's rules.
    pub fn context_text(&self) -> String {
        match &self.editing {
            Some(editing) => format!(
                "Editing {} in place:\n{}",
                match &editing.rule {
                    Some(rule) => format!("the rule {rule}"),
                    None => "a new rule".to_string(),
                },
                section(&editing.name, &editing.text.text())
            ),
            None => self
                .view
                .as_ref()
                .map_or(String::new(), |view| view.prose.text.clone()),
        }
    }

    /// Whether a rule is open in its editor or a request to the agent is typed, which Escape must not drop.
    pub fn unsaved(&self) -> bool {
        self.editing.is_some() || !self.wish.text().trim().is_empty()
    }

    /// The text of the rules file of `place`, as the server has it.
    fn file_of(&self, place: Place) -> Option<&str> {
        let view = self.view.as_ref()?;
        match place {
            Place::Server => Some(&view.prose.text),
            Place::Repository(id) => view
                .repositories
                .iter()
                .find(|repository| repository.repository_id == id)
                .map(|repository| repository.text.as_str()),
        }
    }

    /// The rules of `place`, as the server read them.
    fn rules_of(&self, place: Place) -> &[Rule] {
        let Some(view) = &self.view else {
            return &[];
        };
        match place {
            Place::Server => &view.prose.rules,
            Place::Repository(id) => view
                .repositories
                .iter()
                .find(|repository| repository.repository_id == id)
                .map_or(&[], |repository| &repository.rules),
        }
    }

    /// The whole file a save of the rule being edited sends: the rule put back where it was, appended when it is new,
    /// or dropped when `delete` says so.
    pub fn file_to_save(&self, delete: bool) -> Option<String> {
        let editing = self.editing.as_ref()?;
        let file = self.file_of(editing.place)?;
        let edited = if delete {
            String::new()
        } else {
            section(&editing.name, &editing.text.text())
        };
        match &editing.rule {
            Some(rule) => replace_rule(file, rule, &edited),
            None => Some(append_rule(file, &edited)),
        }
    }
}
