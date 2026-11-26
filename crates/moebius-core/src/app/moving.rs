//! A person moving a card to another column of its board, as a column script moves it: the new column's script takes
//! it up and a `lane = true` column queues the lane. The move is a fresh start, as a retry is, so a card whose run was
//! stopped or whose note was wrong can go on from where the person puts it.

use std::collections::BTreeMap;

use super::{App, AppError, Result};
use crate::app::columns::ProblemKind;
use crate::decide::Actor;
use crate::store::Card;

/// What a person's move does to the card's note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteChange {
    /// The note stays as it is.
    Keep,
    /// The note becomes this text.
    Set(String),
    /// The card has no note any more.
    Clear,
}

impl NoteChange {
    /// The word the decision records for it: `kept`, `replaced` or `cleared`.
    fn word(&self) -> &'static str {
        match self {
            NoteChange::Keep => "kept",
            NoteChange::Set(_) => "replaced",
            NoteChange::Clear => "cleared",
        }
    }
}

impl App {
    /// A person moves the card to `column` of its board, keeping, replacing or clearing its note. The decision is
    /// recorded (kind `move_card`); the runs so far no longer count toward the card's limits, a stopped run no longer
    /// holds it and its `stuck` item is resolved. Refused for an agent or a script, for a column the board does not
    /// have, for the column the card is in, while a run of the card is going, and when the board's `kanban.toml`
    /// cannot be read.
    pub fn move_card_by(
        &self,
        by: &Actor,
        card_id: i64,
        column: &str,
        note: NoteChange,
        reason: &str,
    ) -> Result<Card> {
        if !by.is_person() {
            return Err(AppError::Forbidden("only a person moves a card".into()));
        }
        let card = self.card(card_id)?;
        let columns = self.project_columns_of(card.project_id)?;
        if let Some((ProblemKind::Invalid, problem)) = &columns.problem {
            return Err(AppError::Invalid(format!(
                "the board cannot be read: {problem}"
            )));
        }
        if columns.column(column).is_none() {
            return Err(AppError::Invalid(format!(
                "the board of card {card_id} has no column {column}"
            )));
        }
        if card.column == column {
            return Err(AppError::Invalid(format!(
                "card {card_id} is in {column} already"
            )));
        }
        if self
            .store
            .runs_for_card(card_id)?
            .iter()
            .any(|run| run.ended_at.is_none())
        {
            return Err(AppError::Conflict(format!(
                "a run of card {card_id} is going; stop it first"
            )));
        }
        let context = BTreeMap::from([
            ("title".into(), card.title.clone()),
            ("from".into(), card.column.clone()),
            ("to".into(), column.to_string()),
            ("note".into(), note.word().to_string()),
        ]);
        self.record_action(by, "move_card", Some(card_id), None, context, reason)?;
        match &note {
            NoteChange::Keep => {}
            NoteChange::Set(text) => self.store.set_card_note(card_id, text)?,
            NoteChange::Clear => self.store.clear_card_note(card_id)?,
        }
        self.store.set_card_retried(card_id)?;
        self.store.resolve_inbox_kind_for_card(card_id, "stuck")?;
        self.move_card_from(card_id, &card.column, column, None, Some(by))?
            .ok_or_else(|| {
                AppError::Conflict(format!("card {card_id} was moved by something else first"))
            })
    }
}
