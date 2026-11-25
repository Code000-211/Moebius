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
    /// By card id; a card in a column that counts as Done has none.
    pub cards: HashMap<i64, CardFacts>,
    /// The tokens used since the last retry, by card id; a card that used none is missing.
    pub tokens: HashMap<i64, i64>,
    /// The attempts of each card, by card id, oldest first.
    pub attempts: HashMap<i64, Vec<Attempt>>,
    /// How many tool calls were refused, by the id of the run (the card's latest one).
    pub refusals: HashMap<i64, i64>,
}

/// The classes of columns for code that asks about many cards: each project's columns are read once.
pub(crate) struct ClassOf<'a> {
    app: &'a App,
    projects: HashMap<i64, Option<std::sync::Arc<columns::ProjectColumns>>>,
}

impl ClassOf<'_> {
    /// The class of the column `column` of the project `project`, as [`App::column_class`] answers it.
    pub(crate) fn class(&mut self, project: i64, column: &str) -> &'static str {
        let app = self.app;
        self.projects
            .entry(project)
            .or_insert_with(|| app.project_columns_of(project).ok())
            .as_ref()
            .and_then(|columns| columns.class_of(column))
            .or_else(|| columns::built_in(column))
            .unwrap_or(columns::BACKLOG)
    }

    /// Whether `card` is closed or in a column that counts as `Done`.
    pub(crate) fn is_done(&mut self, card: &Card) -> bool {
        card.closed || self.class(card.project_id, &card.column) == columns::DONE
    }
}

impl App {
    /// The classes of columns, for asking about many cards.
    pub(crate) fn class_of(&self) -> ClassOf<'_> {
        ClassOf {
            app: self,
            projects: HashMap::new(),
        }
    }

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
        let mut class_of = self.class_of();
        for card in cards
            .iter()
            .filter(|c| class_of.class(c.project_id, &c.column) != columns::DONE)
        {
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};
    use std::sync::Arc;
    use std::time::Duration as Wait;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::app::{AnalyticsQuery, Carry, CycleFields};
    use crate::decide::Actor;
    use crate::harness::repo::testing::{put, use_harness};
    use crate::store::{Repository, Store};

    /// The columns of the project `web` in these tests, each counting as the built-in column it is after.
    const BOARD: &str = r#"
[[column]]
name = "Parked"
after = "Backlog"

[[column]]
name = "Building"
after = "Not started"
counts_as = "In progress"

[[column]]
name = "Ready for testing"
after = "In review"

[[column]]
name = "Approved"
after = "In review"
counts_as = "Ready to merge"

[[column]]
name = "Shipped"
after = "Ready to merge"
counts_as = "Done"
"#;

    /// An app with a repository `web` whose project has the custom columns of [`BOARD`].
    fn custom_board() -> (tempfile::TempDir, App, Repository) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = App::new(
            store,
            Arc::new(Host::new(Wait::from_secs(1))),
            &dir.path().join("data"),
        );
        use_harness(&app, dir.path());
        let repository = app
            .store
            .add_repository("web", "/w", "none", "main")
            .unwrap();
        put(&app, "web", "kanban.toml", BOARD);
        // Reading the columns records their classes for the SQL that asks for them.
        app.project_columns_of(repository.home_project_id).unwrap();
        (dir, app, repository)
    }

    fn card_in(
        app: &App,
        repository: &Repository,
        title: &str,
        column: &str,
    ) -> crate::store::Card {
        app.store
            .add_card(repository.id, title, "", "person", column)
            .unwrap()
    }

    #[test]
    fn a_column_counts_as_its_class_for_one_card_and_for_many() {
        let (_dir, app, repository) = custom_board();
        let project = repository.home_project_id;
        for (column, class) in [
            ("Parked", "Backlog"),
            ("Building", "In progress"),
            ("Ready for testing", "In review"),
            ("Approved", "Ready to merge"),
            ("shipped", "Done"),
            ("Done", "Done"),
        ] {
            assert_eq!(app.column_class(project, column), class, "{column}");
        }
        let shipped = card_in(&app, &repository, "Shipped", "Shipped");
        // The card is read with the closed flag the store works out with SQL.
        assert!(app.card(shipped.id).unwrap().closed);
        let mut class_of = app.class_of();
        assert_eq!(class_of.class(project, "Approved"), "Ready to merge");
        assert!(class_of.is_done(&shipped));
        let building = card_in(&app, &repository, "Open", "Building");
        assert!(!class_of.is_done(&building));
    }

    #[test]
    fn a_blocker_in_a_column_that_counts_as_done_no_longer_blocks() {
        let (_dir, app, repository) = custom_board();
        let who = Actor::first_owner();
        let blocker = card_in(&app, &repository, "Blocker", "Building");
        let blocked = app
            .store
            .add_confirmed_card(
                repository.id,
                None,
                "Blocked",
                "",
                "person",
                "Backlog",
                &BTreeSet::from([blocker.number]),
            )
            .unwrap();
        let blocked_by = |app: &App| {
            let lookups = app
                .card_lookups(
                    std::slice::from_ref(&repository),
                    &HashMap::from([(repository.id, app.store.cards(repository.id).unwrap())]),
                    &HashMap::new(),
                )
                .unwrap();
            let card = app.card(blocked.id).unwrap();
            let sight = app.sight(&who).unwrap();
            let from_lookups = lookups.blocked_by_seen(&card, &sight);
            assert_eq!(app.blocked_by_seen(&card, &sight).unwrap(), from_lookups);
            assert_eq!(app.blocked_by(&card).unwrap(), from_lookups);
            from_lookups
        };
        assert_eq!(blocked_by(&app), [blocker.number]);
        app.store
            .move_card(blocker.id, "Ready for testing")
            .unwrap();
        assert_eq!(blocked_by(&app), [blocker.number]);
        app.store.move_card(blocker.id, "Shipped").unwrap();
        assert!(blocked_by(&app).is_empty());
        // As for the built-in column.
        app.store.move_card(blocker.id, "Backlog").unwrap();
        assert_eq!(blocked_by(&app), [blocker.number]);
        app.store.move_card(blocker.id, "Done").unwrap();
        assert!(blocked_by(&app).is_empty());
    }

    #[test]
    fn the_facts_of_a_board_leave_out_the_cards_in_a_column_that_counts_as_done() {
        let (_dir, app, repository) = custom_board();
        let open = card_in(&app, &repository, "Open", "Building");
        let shipped = card_in(&app, &repository, "Shipped", "Shipped");
        for card in [&open, &shipped] {
            app.store
                .add_run(Some(card.id), "coder", "fake", "code")
                .unwrap();
        }
        let cards = app.store.cards(repository.id).unwrap();
        let facts = app.board_facts(repository.id, &cards).unwrap();
        assert_eq!(facts.cards[&open.id].rounds, 1);
        assert!(!facts.cards.contains_key(&shipped.id));
        assert!(!facts.attempts.contains_key(&shipped.id));
        assert!(!facts.tokens.contains_key(&shipped.id));
    }

    #[test]
    fn the_open_count_and_the_milestone_follow_the_class() {
        let (_dir, app, repository) = custom_board();
        let who = Actor::first_owner();
        let project = repository.home_project_id;
        let shipped = card_in(&app, &repository, "Shipped", "Shipped");
        let testing = card_in(&app, &repository, "Testing", "Ready for testing");
        assert!(!app.card(testing.id).unwrap().closed);
        assert_eq!(app.store.open_cards(project).unwrap(), 1);
        let open = app.open_issue_numbers(repository.id).unwrap();
        assert!(open.contains(&testing.number) && !open.contains(&shipped.number));
        let milestone = app.store.add_milestone("Launch", "2999-01-01", "").unwrap();
        for card in [&shipped, &testing] {
            app.store
                .set_card_milestones(card.id, &[milestone.id])
                .unwrap();
        }
        let detail = app.milestone_detail(&who, milestone.id).unwrap();
        assert_eq!((detail.view.cards, detail.view.done), (2, 1));
        assert_eq!(detail.view.projects[0].done, 1);
    }

    #[test]
    fn a_cycle_counts_a_card_in_a_column_that_counts_as_done() {
        let (_dir, app, repository) = custom_board();
        let who = Actor::first_owner();
        let project = repository.home_project_id;
        let today = app.store.today().unwrap();
        let day = |offset: i64| {
            let format =
                time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap();
            (time::Date::parse(&today, &format).unwrap() + time::Duration::days(offset))
                .format(&format)
                .unwrap()
        };
        let fields = |name: &str, from: i64, to: i64| CycleFields {
            name: name.into(),
            starts_on: day(from),
            ends_on: day(to),
        };
        let cycle = app.add_cycle(&who, project, &fields("One", -1, 5)).unwrap();
        let shipped = card_in(&app, &repository, "Shipped", "Building");
        let building = card_in(&app, &repository, "Building", "Building");
        for card in [&shipped, &building] {
            app.set_card_cycle(&who, card.id, Some(cycle.id)).unwrap();
        }
        app.store.move_card(shipped.id, "Shipped").unwrap();
        let detail = app.cycle_detail(&who, cycle.id).unwrap();
        assert_eq!(detail.done, 1);
        let cards: Vec<_> = detail
            .cards
            .iter()
            .map(|c| (c.id, c.done, c.done_at.is_some()))
            .collect();
        assert_eq!(
            cards,
            [(shipped.id, true, true), (building.id, false, false)]
        );
        // Closing the cycle carries the card that is not done on and leaves the one that is.
        let next = app.add_cycle(&who, project, &fields("Two", 6, 12)).unwrap();
        let closed = app
            .close_cycle(&who, cycle.id, &Carry::Next { to: Some(next.id) })
            .unwrap();
        assert_eq!(closed.carried, [building.id]);
    }

    #[test]
    fn analytics_count_a_card_in_a_column_that_counts_as_done() {
        let (_dir, app, repository) = custom_board();
        let who = Actor::first_owner();
        let s = app.store.window_start(3).unwrap();
        let hour = 3600;
        let a = card_in(&app, &repository, "A", "Backlog");
        let b = card_in(&app, &repository, "B", "Backlog");
        let open = card_in(&app, &repository, "Open", "Ready for testing");
        app.store
            .exec_for_test(&format!(
                "UPDATE cards SET created_at = {} WHERE id IN ({}, {}, {});
                 INSERT INTO card_labels (card_id, label) VALUES ({}, 'estimate:3'), ({}, 'estimate:2');
                 INSERT INTO card_moves (card_id, from_column, to_column, at) VALUES
                    ({}, 'Backlog', 'Building', {}), ({}, 'Building', 'Shipped', {}), ({}, 'Backlog', 'Shipped', {});
                 UPDATE cards SET column_name = 'Shipped', moved_at = {} WHERE id IN ({}, {});",
                s - 1000,
                a.id,
                b.id,
                open.id,
                a.id,
                b.id,
                a.id,
                s + hour,
                a.id,
                s + 12 * hour,
                b.id,
                s + 20 * hour,
                s + 20 * hour,
                a.id,
                b.id,
            ))
            .unwrap();
        let query = AnalyticsQuery {
            days: 3,
            ..AnalyticsQuery::default()
        };
        let all = app.analytics(&who, &query).unwrap();
        assert_eq!(
            (all.totals.done, all.totals.points_done, all.totals.open),
            (2, 5, 1)
        );
        // A started when it entered a column that counts as In progress, and was done when it entered Shipped.
        let cycle_time = all.cycle_time.unwrap();
        assert_eq!(
            (cycle_time.cards, cycle_time.median_seconds),
            (1, 11 * hour)
        );
        // The time in Shipped is not counted, as the time in Done is not.
        assert!(all.columns.iter().all(|c| c.column != "Shipped"));
        assert!(all.columns.iter().any(|c| c.column == "Building"));
        // The burn-up of a milestone counts the card as done too.
        let milestone = app.store.add_milestone("Launch", "2999-01-01", "").unwrap();
        for card in [&a, &open] {
            app.store
                .set_card_milestones(card.id, &[milestone.id])
                .unwrap();
        }
        let all = app.analytics(&who, &query).unwrap();
        assert_eq!(all.milestones[0].done, 1);
        assert_eq!(all.milestones[0].points_done, 3);
    }

    #[test]
    fn a_change_waiting_in_a_column_that_counts_as_ready_to_merge_can_be_rejected() {
        let (_dir, app, repository) = custom_board();
        let card = card_in(&app, &repository, "Feature", "Approved");
        let change = app
            .store
            .add_change(repository.id, Some(card.id), "feature", None, "abc")
            .unwrap();
        app.reject_change(&Actor::first_owner(), change.id, "Add a test.")
            .unwrap();
        assert_eq!(app.card(card.id).unwrap().column, "In progress");
        // A card that is not waiting to be merged is refused, as in a built-in column.
        let other = card_in(&app, &repository, "Other", "Ready for testing");
        let change = app
            .store
            .add_change(repository.id, Some(other.id), "other", None, "def")
            .unwrap();
        assert!(
            app.reject_change(&Actor::first_owner(), change.id, "")
                .is_err()
        );
    }

    #[test]
    fn closing_and_reopening_an_issue_go_by_the_class_of_its_column() {
        let (_dir, app, repository) = custom_board();
        let shipped = card_in(&app, &repository, "Shipped", "Shipped");
        // Already in a column that counts as Done: closing it leaves it where it is.
        app.close_card(shipped.id).unwrap();
        assert_eq!(app.card(shipped.id).unwrap().column, "Shipped");
        // Reopening sends it back to the backlog, as it does a card in Done.
        app.reopened(shipped.id, None).unwrap();
        assert_eq!(app.card(shipped.id).unwrap().column, "Backlog");
        let testing = card_in(&app, &repository, "Testing", "Ready for testing");
        app.close_card(testing.id).unwrap();
        assert_eq!(app.card(testing.id).unwrap().column, "Done");
    }

    #[test]
    fn a_card_in_a_column_that_counts_as_backlog_is_approved_from_there() {
        let (_dir, app, repository) = custom_board();
        let who = Actor::first_owner();
        let parked = card_in(&app, &repository, "Parked", "Parked");
        let card = app.approve_issue(&who, parked.id, "fine").unwrap();
        assert_eq!(card.column, "Not started");
        let building = card_in(&app, &repository, "Building", "Building");
        assert!(app.approve_issue(&who, building.id, "fine").is_err());
    }

    #[test]
    fn a_closed_followed_request_sends_the_card_back_to_a_column_that_is_not_review_or_done() {
        let (_dir, app, repository) = custom_board();
        let closed_from = |column: &str| {
            let card = card_in(&app, &repository, column, "In review");
            let change = app
                .store
                .add_change(repository.id, Some(card.id), "followed", Some(7), "abc")
                .unwrap();
            app.store
                .set_change_outside(change.id, "outsider", column)
                .unwrap();
            app.followed_closed(&app.change(change.id).unwrap())
                .unwrap();
            app.card(card.id).unwrap().column
        };
        assert_eq!(closed_from("Building"), "Building");
        assert_eq!(closed_from("Ready for testing"), "Not started");
        assert_eq!(closed_from("Shipped"), "Not started");
    }
}
