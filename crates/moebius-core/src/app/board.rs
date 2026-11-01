//! What a board shows of a card, worked out once: the `code` rounds since the card was retried, the failed rounds of
//! attempts, and the card's latest run. The board snapshot and the `card` table of column scripts both use it, so
//! that they cannot disagree; the snapshot reads for a whole board in a few queries instead of a few per card.

use std::collections::HashMap;

use super::{App, Result, columns, runs_since_retry};
use crate::store::{Attempt, Card, Run};

/// The facts about a card's runs that a board and its scripts share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFacts {
    /// Every run of the card, oldest first (trials aside).
    pub runs: Vec<Run>,
    /// The `code` runs since the last retry, and the rounds of attempts of which none passed.
    pub rounds: i64,
    /// The card's latest run that is not an attempt: an attempt that was not picked is shown by `attempts`.
    pub run: Option<Run>,
}

impl CardFacts {
    /// The facts of `card` from its `runs` and the number of its failed rounds of attempts.
    fn new(card: &Card, runs: Vec<Run>, failed_attempt_rounds: i64) -> Self {
        let code = runs_since_retry(&runs, card)
            .filter(|r| r.purpose == "code")
            .count() as i64;
        let run = runs.iter().rev().find(|r| r.purpose != "attempt").cloned();
        Self {
            runs,
            rounds: code + failed_attempt_rounds,
            run,
        }
    }
}

/// What a board reads of the cards that are not done, for the whole board at once.
#[derive(Debug, Default)]
pub struct BoardFacts {
    /// By card id; a card in Done has none.
    pub cards: HashMap<i64, CardFacts>,
    /// The tokens used since the last retry, by card id; a card that used none is missing.
    pub tokens: HashMap<i64, i64>,
    /// The attempts of each card, by card id, oldest first.
    pub attempts: HashMap<i64, Vec<Attempt>>,
    /// How many tool calls were refused, by the id of the run (the card's latest one).
    pub refusals: HashMap<i64, i64>,
}

impl App {
    /// The facts of one card.
    pub fn card_facts(&self, card: &Card) -> Result<CardFacts> {
        Ok(CardFacts::new(
            card,
            self.store.runs_for_card(card.id)?,
            self.store.failed_attempt_rounds(card.id)?,
        ))
    }

    /// The facts of the cards of the repository `repository_id` that are not done, in a handful of reads.
    pub fn board_facts(&self, repository_id: i64, cards: &[Card]) -> Result<BoardFacts> {
        let mut runs: HashMap<i64, Vec<Run>> = HashMap::new();
        for run in self.store.runs_for_board(repository_id, columns::DONE)? {
            if let Some(card_id) = run.card_id {
                runs.entry(card_id).or_default().push(run);
            }
        }
        let mut failed = self.store.failed_attempt_rounds_for_board(repository_id)?;
        let mut facts = BoardFacts {
            tokens: self.store.tokens_for_board(repository_id, columns::DONE)?,
            ..BoardFacts::default()
        };
        for attempt in self
            .store
            .attempts_for_board(repository_id, columns::DONE)?
        {
            facts
                .attempts
                .entry(attempt.card_id)
                .or_default()
                .push(attempt);
        }
        for card in cards.iter().filter(|c| c.column != columns::DONE) {
            let card_facts = CardFacts::new(
                card,
                runs.remove(&card.id).unwrap_or_default(),
                failed.remove(&card.id).unwrap_or_default(),
            );
            facts.cards.insert(card.id, card_facts);
        }
        let latest: Vec<i64> = facts
            .cards
            .values()
            .filter_map(|f| f.run.as_ref().map(|r| r.id))
            .collect();
        facts.refusals = self.store.refused_tool_calls_of(&latest)?;
        Ok(facts)
    }
}
