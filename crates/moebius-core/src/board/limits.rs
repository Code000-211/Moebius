//! The engine's limits, which scripts see as `moebius.run_limit`, `moebius.round_limit`, `moebius.agent_budget(name)`
//! and `moebius.day_budget`, and the memory and time a script call may take.

use std::collections::BTreeMap;
use std::time::Duration;

/// The engine's limits.
#[derive(Debug, Clone)]
pub struct Limits {
    pub run_limit: u32,
    pub round_limit: u32,
    /// Token budgets for a card by agent name, besides those the harness knows (`Runs::token_budget`), which win.
    pub agent_budgets: BTreeMap<String, i64>,
    /// The most tokens all runs may use in a day, if limited.
    pub day_budget: Option<i64>,
    /// How long one script call may run, not counting the calls it makes into Moebius.
    pub script_time: Duration,
    /// How much memory one script call may use.
    pub script_memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            run_limit: 3,
            round_limit: 5,
            agent_budgets: BTreeMap::new(),
            day_budget: None,
            script_time: Duration::from_secs(2),
            script_memory: 16 << 20,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use super::*;
    use crate::app::{App, OWNER_AUTHOR};
    use crate::board::testing::{FakeHarness, column, engine_with, setup, setup_with_harness};
    use crate::board::{Engine, Kanban};
    use crate::store::{Card, Run};

    /// Records `tokens` used by a run of the card.
    fn use_tokens(app: &App, run: i64, tokens: i64) {
        app.store
            .add_usage(&crate::store::Usage {
                run_id: Some(run),
                model: "local".into(),
                input_tokens: tokens,
                output_tokens: 0,
                cached_tokens: 0,
                duration_ms: 0,
                tools: Vec::new(),
            })
            .unwrap();
    }

    /// A card in progress whose latest `code` run ended as `status`.
    fn card_after_a_run(app: &App, repo: i64, status: &str) -> (Card, Run) {
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        app.move_card(card.id, "In progress", None).unwrap();
        let run = app
            .store
            .add_run(Some(card.id), "Coder", "fake", "code")
            .unwrap();
        app.store.set_run_status(run.id, status).unwrap();
        (card, run)
    }

    fn runs_of(app: &App, card: &Card) -> usize {
        app.store.runs_for_card(card.id).unwrap().len()
    }

    fn stuck_items(app: &App) -> Vec<String> {
        let inbox = app.store.inbox().unwrap();
        assert!(inbox.iter().all(|item| item.kind == "stuck"), "{inbox:?}");
        inbox
            .iter()
            .map(|item| item.data["reason"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn a_card_at_the_round_limit_is_stuck_in_the_inbox() {
        let (_dir, app, engine, repo) = setup();
        let (card, _) = card_after_a_run(&app, repo, "failed");
        for _ in 1..Limits::default().round_limit {
            let run = app
                .store
                .add_run(Some(card.id), "Coder", "fake", "code")
                .unwrap();
            app.store.set_run_status(run.id, "failed").unwrap();
        }
        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("the round limit is reached"));
        assert_eq!(step.started_run, None);
        assert_eq!(stuck_items(&app), ["the round limit is reached"]);
        assert_eq!(runs_of(&app, &card), 5);
    }

    #[test]
    fn a_card_that_spent_the_agents_token_budget_is_stuck_without_starting_a_run() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                agent_budgets: BTreeMap::from([("Coder".to_string(), 1000)]),
                ..Limits::default()
            },
        );
        let (card, run) = card_after_a_run(&app, repo, "failed");
        use_tokens(&app, run.id, 600);
        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.started_run, Some(2), "there is budget left");
        let second = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        app.store.set_run_status(second.id, "failed").unwrap();
        use_tokens(&app, second.id, 400);

        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.started_run, None);
        assert_eq!(
            step.waiting.as_deref(),
            Some("the token budget of 1000 is spent")
        );
        assert_eq!(stuck_items(&app), ["the token budget of 1000 is spent"]);
        assert_eq!(runs_of(&app, &card), 2);
    }

    #[test]
    fn the_reviewers_token_budget_stops_the_review() {
        let harness = Arc::new(FakeHarness::with_reviewer(&[]));
        let (_dir, app, _, repo) = setup_with_harness(None, harness.clone());
        let engine = Engine::new(
            app.clone(),
            harness,
            Kanban::default(),
            Limits {
                agent_budgets: BTreeMap::from([("Reviewer".to_string(), 100)]),
                ..Limits::default()
            },
        );
        let card = app
            .create_issue(repo, "Add a feature", "", OWNER_AUTHOR)
            .unwrap();
        engine.settle(card.id).unwrap();
        let code = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        use_tokens(&app, code.id, 100);
        let step = engine.settle(card.id).unwrap();
        assert_eq!(column(&app, &card), "In review");
        assert_eq!(
            step.waiting.as_deref(),
            Some("the token budget of 100 is spent")
        );
        assert_eq!(runs_of(&app, &card), 1);
        assert_eq!(stuck_items(&app).len(), 1);
    }

    /// A card of another repository that has a run going, which uses a slot of the limit.
    fn busy_card(app: &App, repo: i64) -> Run {
        let card = app.create_issue(repo, "Busy", "", OWNER_AUTHOR).unwrap();
        app.move_card(card.id, "In progress", None).unwrap();
        app.store
            .add_run(Some(card.id), "Coder", "fake", "code")
            .unwrap()
    }

    #[test]
    fn a_code_round_waits_for_a_free_slot_of_the_run_limit() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                run_limit: 1,
                ..Limits::default()
            },
        );
        let busy = busy_card(&app, repo);
        let card = app.create_issue(repo, "Waits", "", OWNER_AUTHOR).unwrap();
        app.move_card(card.id, "In progress", None).unwrap();
        let step = engine.settle(card.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("the run limit is reached"));
        assert_eq!(step.started_run, None);
        assert_eq!(runs_of(&app, &card), 0);
        app.store.set_run_status(busy.id, "succeeded").unwrap();
        let step = engine.settle(card.id).unwrap();
        assert!(step.started_run.is_some());
        assert_eq!(step.waiting.as_deref(), Some("coding"));
    }

    #[test]
    fn a_review_waits_for_a_free_slot_and_its_two_runs_do_not_block_each_other() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                run_limit: 1,
                ..Limits::default()
            },
        );
        let card = app
            .create_issue(repo, "Reviewed", "", OWNER_AUTHOR)
            .unwrap();
        while runs_of(&app, &card) == 0 {
            engine.settle(card.id).unwrap();
        }
        let busy = busy_card(&app, repo);
        // Publishing the change and waiting for its checks take a few ticks; the review itself waits for the slot.
        let mut step = engine.settle(card.id).unwrap();
        for _ in 0..2 {
            step = engine.settle(card.id).unwrap();
        }
        let purposes: Vec<_> = app
            .store
            .runs_for_card(card.id)
            .unwrap()
            .into_iter()
            .map(|r| r.purpose)
            .collect();
        assert_eq!(purposes, ["code"], "no review starts at the limit");
        assert_eq!(step.waiting.as_deref(), Some("the run limit is reached"));
        app.store.set_run_status(busy.id, "succeeded").unwrap();
        // With one slot, the Standards run ends before the Spec run starts: the review completes.
        for _ in 0..4 {
            engine.settle(card.id).unwrap();
        }
        let purposes: Vec<_> = app
            .store
            .runs_for_card(card.id)
            .unwrap()
            .into_iter()
            .map(|r| r.purpose)
            .collect();
        assert_eq!(purposes, ["code", "review-standards", "review-spec"]);
        assert_eq!(column(&app, &card), "Ready to merge");
    }

    /// A harness whose runs never end, so the runs going at once can be counted.
    struct GoingOn;

    impl crate::board::CardRuns for GoingOn {
        fn start(
            &self,
            app: &Arc<App>,
            card: &Card,
            agent: &str,
            (purpose, _): (&str, Option<&str>),
            _: crate::board::Variant,
        ) -> Result<Run, crate::board::StartError> {
            app.store
                .add_run(Some(card.id), agent, "fake", purpose)
                .map_err(|e| crate::board::StartError::Store(e.to_string()))
        }
        fn has_agent(&self, _: &str) -> bool {
            true
        }
        fn runners(&self) -> Vec<crate::harness::remote::RunnerInfo> {
            Vec::new()
        }
        fn agents(&self) -> Vec<crate::board::AgentInfo> {
            Vec::new()
        }
        fn token_budget(&self, _: &str) -> Option<i64> {
            None
        }
        fn steer(&self, _: i64, _: &str) -> bool {
            false
        }
        fn stop(&self, _: i64) -> bool {
            false
        }
        fn hold(&self, _: bool) {}
    }

    /// The most runs of cards going at once while 20 cards are in progress and the runs end one at a time: with
    /// `run_limit = 3` it is 3 (without the limit in `start_run` all 20 started together).
    #[test]
    fn twenty_cards_in_progress_never_have_more_runs_going_than_the_run_limit() {
        let (_dir, app, _, repo) = setup();
        let engine = Engine::new(
            app.clone(),
            Arc::new(GoingOn),
            Kanban::default(),
            Limits::default(),
        );
        let cards: Vec<Card> = (0..20)
            .map(|n| {
                let card = app
                    .create_issue(repo, &format!("Card {n}"), "", OWNER_AUTHOR)
                    .unwrap();
                app.move_card(card.id, "In progress", None).unwrap();
                card
            })
            .collect();
        let mut peak = 0;
        while cards.iter().any(|card| runs_of(&app, card) == 0) {
            for card in &cards {
                engine.settle(card.id).unwrap();
            }
            let active = app.store.active_runs().unwrap();
            peak = peak.max(active.len());
            assert!(active.len() <= 3, "{} runs go at once", active.len());
            for run in active {
                app.store.set_run_status(run.id, "failed").unwrap();
            }
        }
        assert_eq!(peak, 3);
    }

    #[test]
    fn a_chat_does_not_count_against_the_run_limit() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                run_limit: 1,
                ..Limits::default()
            },
        );
        let chat = app.store.add_run(None, "Coder", "fake", "chat").unwrap();
        app.store.set_run_status(chat.id, "waiting").unwrap();

        let first = app.create_issue(repo, "First", "", OWNER_AUTHOR).unwrap();
        let second = app.create_issue(repo, "Second", "", OWNER_AUTHOR).unwrap();
        app.move_card(second.id, "Not started", None).unwrap();

        // A card's run uses the one run the limit allows; the chat does not.
        let coding = app
            .store
            .add_run(Some(first.id), "Coder", "fake", "code")
            .unwrap();
        let step = engine.settle(second.id).unwrap();
        assert_eq!(step.waiting.as_deref(), Some("the run limit is reached"));
        app.store.set_run_status(coding.id, "succeeded").unwrap();
        assert!(engine.settle(second.id).unwrap().started_run.is_some());
    }

    #[test]
    fn a_spent_daily_budget_keeps_cards_from_starting_and_from_going_on() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                day_budget: Some(500),
                ..Limits::default()
            },
        );
        let (busy, run) = card_after_a_run(&app, repo, "succeeded");
        use_tokens(&app, run.id, 499);
        let ready = app.create_issue(repo, "Waiting", "", OWNER_AUTHOR).unwrap();
        app.move_card(ready.id, "Not started", None).unwrap();
        let (second_round, failed) = card_after_a_run(&app, repo, "failed");

        // 499 of 500 tokens are used: cards go on.
        engine.tick(ready.id).unwrap();
        assert_eq!(column(&app, &ready), "In progress");
        app.move_card(ready.id, "Not started", None).unwrap();

        use_tokens(&app, failed.id, 1);
        let step = engine.settle(ready.id).unwrap();
        assert_eq!(column(&app, &ready), "Not started");
        assert_eq!(
            step.waiting.as_deref(),
            Some("the daily token budget is spent")
        );
        // A card that needs another round waits, too, instead of starting a run the router would refuse.
        let step = engine.settle(second_round.id).unwrap();
        assert_eq!(step.started_run, None);
        assert_eq!(
            step.waiting.as_deref(),
            Some("the daily token budget is spent")
        );
        assert!(app.store.inbox().unwrap().is_empty());
        // A card that is done with coding goes on to review.
        engine.tick(busy.id).unwrap();
        assert_eq!(column(&app, &busy), "In review");
    }

    #[test]
    fn a_stopped_run_puts_its_card_in_the_inbox_and_it_is_not_retried_by_itself() {
        let (_dir, app, engine, repo) = setup();
        let (card, _) = card_after_a_run(&app, repo, "stopped");
        for _ in 0..3 {
            let step = engine.settle(card.id).unwrap();
            assert_eq!(step.waiting.as_deref(), Some("stopped by a person"));
        }
        assert_eq!(runs_of(&app, &card), 1);
        assert_eq!(stuck_items(&app), ["stopped by a person"]);
    }

    #[test]
    fn a_retry_gives_a_stuck_card_a_fresh_start() {
        let (_dir, app, _, repo) = setup();
        let engine = engine_with(
            &app,
            Limits {
                agent_budgets: BTreeMap::from([("Coder".to_string(), 1000)]),
                ..Limits::default()
            },
        );
        let (card, run) = card_after_a_run(&app, repo, "failed");
        for _ in 1..Limits::default().round_limit {
            let run = app
                .store
                .add_run(Some(card.id), "Coder", "fake", "code")
                .unwrap();
            app.store.set_run_status(run.id, "failed").unwrap();
        }
        use_tokens(&app, run.id, 1000);
        engine.settle(card.id).unwrap();
        assert_eq!(stuck_items(&app).len(), 1);
        assert!(
            app.waiting(std::slice::from_ref(&card))
                .contains_key(&card.id)
        );

        // Neither the round limit nor the budget stops it any more.
        app.retry_card(
            &crate::decide::Actor::first_owner(),
            card.id,
            "I raised the budget.",
        )
        .unwrap();
        assert!(app.waiting(std::slice::from_ref(&card)).is_empty());
        assert!(app.store.inbox().unwrap().is_empty());
        let step = engine.settle(card.id).unwrap();
        assert!(step.started_run.is_some(), "{step:?}");
        let judgements = app.store.judgements("retry").unwrap();
        assert_eq!(
            (judgements.len(), judgements[0].decider.as_str()),
            (1, "person:owner")
        );

        // A stopped card, too, starts again instead of being stuck again.
        let (stopped, _) = card_after_a_run(&app, repo, "stopped");
        engine.settle(stopped.id).unwrap();
        app.retry_card(&crate::decide::Actor::first_owner(), stopped.id, "Go on.")
            .unwrap();
        let step = engine.settle(stopped.id).unwrap();
        assert!(step.started_run.is_some(), "{step:?}");
        assert_eq!(app.card(stopped.id).unwrap().column, "In progress");
    }
}
