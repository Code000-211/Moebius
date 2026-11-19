//! What the analytics need of every card, read in a few passes instead of one query per card: when it was made, when it
//! was done, when work on it started, the column it sat in between its moves, its estimate and the tokens its runs
//! used. The sums and the narrowing to what a person may see are in `app::analytics`.

use std::collections::HashMap;

use rusqlite::params;

use super::{Result, Store, planning};

/// The column whose first entry starts a card that no code run has started.
const IN_PROGRESS: &str = "In progress";

/// A move of a card between columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub from: String,
    pub to: String,
    pub at: i64,
}

/// A card as far as the analytics need it. Archived cards are here too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsCard {
    pub id: i64,
    pub project_id: i64,
    pub repository_id: i64,
    pub created_at: i64,
    /// The column the card is in now.
    pub column: String,
    /// Whether its issue is closed or it is in `Done`.
    pub done: bool,
    /// When it left the board by being archived.
    pub archived_at: Option<i64>,
    /// When it was done: the last move into `Done`, else the last move; none while it is not done.
    pub done_at: Option<i64>,
    /// The local day of `done_at`, `YYYY-MM-DD`.
    pub done_day: Option<String>,
    /// The earlier of its first code run and its first move into `In progress`.
    pub started_at: Option<i64>,
    pub estimate: Option<u32>,
    /// The tokens of its runs, those of trials left out, and with an agent given, only that agent's.
    pub tokens: i64,
    /// Its moves, oldest first.
    pub moves: Vec<Moved>,
}

/// What the cards are narrowed to: those of one repository, and those an agent ran something on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardFilter {
    pub repository: Option<i64>,
    pub agent: Option<String>,
}

impl Store {
    /// The first moment of the local day that is `days - 1` days ago, as Unix seconds.
    pub fn window_start(&self, days: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            &format!("SELECT {}", super::usage::WINDOW_START),
            [days],
            |r| r.get(0),
        )?)
    }

    /// Every card in `filter`, archived ones too, with what the analytics need of each.
    pub fn analytics_cards(&self, filter: &CardFilter) -> Result<Vec<AnalyticsCard>> {
        let conn = self.conn();
        let agent = filter.agent.as_deref();
        let mut cards: Vec<AnalyticsCard> = {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.project_id, c.repository_id, c.created_at, c.column_name, \
                        (c.closed OR c.column_name = 'Done') AS done, \
                        CASE WHEN c.closed OR c.column_name = 'Done' THEN \
                          COALESCE((SELECT MAX(at) FROM card_moves WHERE card_id = c.id AND to_column = 'Done'), c.moved_at) \
                        END AS done_at, c.archived_at \
                 FROM cards c \
                 WHERE (?1 IS NULL OR c.repository_id = ?1) \
                   AND (?2 IS NULL OR EXISTS (SELECT 1 FROM runs r WHERE r.card_id = c.id AND r.agent = ?2)) \
                 ORDER BY c.id",
            )?;
            let rows = stmt.query_map(params![filter.repository, agent], |r| {
                Ok(AnalyticsCard {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    repository_id: r.get(2)?,
                    created_at: r.get(3)?,
                    column: r.get(4)?,
                    done: r.get(5)?,
                    done_at: r.get(6)?,
                    archived_at: r.get(7)?,
                    done_day: None,
                    started_at: None,
                    estimate: None,
                    tokens: 0,
                    moves: Vec::new(),
                })
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let at: HashMap<i64, usize> = cards.iter().enumerate().map(|(i, c)| (c.id, i)).collect();

        let mut stmt = conn.prepare(
            "SELECT card_id, from_column, to_column, at FROM card_moves ORDER BY card_id, id",
        )?;
        for moved in stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Moved {
                    from: r.get(1)?,
                    to: r.get(2)?,
                    at: r.get(3)?,
                },
            ))
        })? {
            let (card, moved) = moved?;
            if let Some(&i) = at.get(&card) {
                cards[i].moves.push(moved);
            }
        }

        let mut stmt = conn.prepare(
            "SELECT card_id, MIN(started_at) FROM runs WHERE purpose = 'code' AND card_id IS NOT NULL GROUP BY card_id",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (card, started) = row?;
            if let Some(&i) = at.get(&card) {
                cards[i].started_at = Some(started);
            }
        }
        for card in &mut cards {
            let entered = card
                .moves
                .iter()
                .find(|m| m.to == IN_PROGRESS)
                .map(|m| m.at);
            card.started_at = match (card.started_at, entered) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }

        let mut stmt = conn.prepare(
            "SELECT r.card_id, SUM(u.input_tokens + u.output_tokens) FROM usage u JOIN runs r ON r.id = u.run_id \
             WHERE r.card_id IS NOT NULL AND r.purpose <> 'trial' AND (?1 IS NULL OR r.agent = ?1) GROUP BY r.card_id",
        )?;
        for row in stmt.query_map(params![agent], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })? {
            let (card, tokens) = row?;
            if let Some(&i) = at.get(&card) {
                cards[i].tokens = tokens;
            }
        }

        let mut stmt =
            conn.prepare("SELECT card_id, label FROM card_labels WHERE label LIKE 'estimate:%'")?;
        let mut labels: HashMap<i64, Vec<String>> = HashMap::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (card, label) = row?;
            labels.entry(card).or_default().push(label);
        }
        for (card, labels) in labels {
            if let Some(&i) = at.get(&card) {
                cards[i].estimate = planning::estimate_of(&labels);
            }
        }

        let mut stmt = conn.prepare("SELECT strftime('%Y-%m-%d', ?1, 'unixepoch', 'localtime')")?;
        let mut days: HashMap<i64, String> = HashMap::new();
        for card in &mut cards {
            if let Some(done) = card.done_at {
                card.done_day = Some(match days.get(&done) {
                    Some(day) => day.clone(),
                    None => {
                        let day: String = stmt.query_row([done], |r| r.get(0))?;
                        days.insert(done, day.clone());
                        day
                    }
                });
            }
        }
        Ok(cards)
    }

    /// Every `(cycle, card, added_at)` of the cycles, closed ones too.
    pub fn cycle_members(&self) -> Result<Vec<(i64, i64, i64)>> {
        self.rows(
            "SELECT cycle_id, card_id, added_at FROM cycle_cards ORDER BY cycle_id, card_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
    }

    /// Every `(milestone, card)` of the milestones.
    pub fn milestone_members(&self) -> Result<Vec<(i64, i64)>> {
        self.rows(
            "SELECT milestone_id, card_id FROM milestone_cards ORDER BY milestone_id, card_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    /// Runs `sql` as it is, so that a test can write a history that no call of the store would.
    #[cfg(test)]
    pub(crate) fn exec_for_test(&self, sql: &str) -> Result<()> {
        Ok(self.conn().execute_batch(sql)?)
    }
}
