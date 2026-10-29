//! The boards and their cards: what a client reads to draw every board, and what a person does to a card and its
//! change.
//!
//! - `GET /v1/snapshot`: every repository with its cards (and what the board shows of each), and the open inbox.
//! - `POST /v1/repositories/{id}/issues` `{title, body}`: creates an issue, which becomes a card in the backlog.
//! - `PUT /v1/cards/{id}` `{title, body}`: changes a card's title and body, and its issue on a forge.
//! - `POST /v1/cards/{id}/approve` `{reason}`: a person approves an outside issue; its card moves to ready.
//! - `POST /v1/changes/{id}/merge` `{reason}`: a person merges a change; its card moves to done.
//! - `POST /v1/changes/{id}/reject` `{reason}`: a person sends a change waiting to be merged back to coding.
//! - `POST /v1/cards/{id}/retry` `{reason}`: a person retries a stuck card or one whose pull request was closed, which
//!   gets a fresh start.
//! - `GET /v1/cards/{id}/runs`: the runs of a card. `GET /v1/cards/{id}/history`: its timeline.

use std::collections::BTreeMap;

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post, put};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, Reason, blocking};
use crate::app::{App, HistoryEntry, PERSON, columns, runs_since_retry};
use crate::board::latest_review;
use crate::decide::Actor;
use crate::store::{Card, Change, InboxItem, Repository, Run};

/// The routes of the boards and their cards.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/snapshot", get(snapshot))
        .route("/v1/repositories/{id}/issues", post(create_issue))
        .route("/v1/cards/{id}", put(edit_card))
        .route("/v1/cards/{id}/approve", post(approve_issue))
        .route("/v1/cards/{id}/retry", post(retry_card))
        .route("/v1/changes/{id}/merge", post(merge_change))
        .route("/v1/changes/{id}/reject", post(reject_change))
        .route("/v1/cards/{id}/runs", get(card_runs))
        .route("/v1/cards/{id}/history", get(card_history))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub repository: Repository,
    pub cards: Vec<CardView>,
    /// The most `code` rounds a card gets.
    pub round_limit: u32,
    /// Why each waiting card waits, by card id.
    pub waiting: BTreeMap<i64, String>,
    pub changes: Vec<Change>,
}

/// A card with what a board shows of it, so that a client needs one read for a board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardView {
    #[serde(flatten)]
    pub card: Card,
    /// The `code` rounds and the tokens since the card was last retried, as column scripts see them.
    pub rounds: i64,
    pub tokens: i64,
    /// The card's latest run.
    pub run: Option<RunView>,
    /// The numbers of the issues that block the card and are not done.
    pub blocked_by: Vec<i64>,
    /// How many tool calls of the latest run were refused.
    pub refusals: i64,
    /// The card's open change.
    pub change: Option<ChangeView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunView {
    pub id: i64,
    pub agent: String,
    pub purpose: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeView {
    pub id: i64,
    pub state: String,
    /// `pending`, `success`, `failure` or `none`, as column scripts see it.
    pub checks: String,
    /// `approve` or `changes` once a review finished since the latest `code` run.
    pub review_verdict: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub boards: Vec<Board>,
    /// The changes of the harness configuration repository, which belongs to no board.
    pub harness_changes: Vec<Change>,
    pub inbox: Vec<InboxItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewIssue {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

/// The body of `PUT /v1/cards/{id}`: the card's title and body, written anew.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditedCard {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Merged {
    pub commit: String,
}

/// What a board shows of a card. A card in Done carries none of it, which keeps the snapshot cheap to read on every
/// event.
fn card_view(app: &App, card: Card) -> crate::app::Result<CardView> {
    let mut view = CardView {
        card,
        rounds: 0,
        tokens: 0,
        run: None,
        blocked_by: Vec::new(),
        refusals: 0,
        change: None,
    };
    if view.card.column == columns::DONE {
        return Ok(view);
    }
    let runs = app.store.runs_for_card(view.card.id)?;
    view.rounds = runs_since_retry(&runs, &view.card)
        .filter(|r| r.purpose == "code")
        .count() as i64;
    view.tokens = app.store.tokens_for_card(view.card.id)?;
    view.blocked_by = app.blocked_by(&view.card)?;
    if let Some(run) = runs.last() {
        view.refusals = app.store.refused_tool_calls(run.id)?;
        view.run = Some(RunView {
            id: run.id,
            agent: run.agent.clone(),
            purpose: run.purpose.clone(),
            status: run.status.clone(),
        });
    }
    if let Some(change) = app.store.open_change(view.card.id)? {
        view.change = Some(ChangeView {
            id: change.id,
            state: change.state.clone(),
            checks: app.checks_of(&change)?.to_string(),
            review_verdict: latest_review(&runs).map(|r| r.verdict.to_string()),
        });
    }
    Ok(view)
}

async fn snapshot(State(state): State<ApiState>) -> Result<axum::Json<Snapshot>, ApiError> {
    let round_limit = state.config.get().round_limit;
    blocking(&state.app, move |app| {
        let mut boards = Vec::new();
        for repository in app.store.repositories()? {
            let cards = app.store.cards(repository.id)?;
            let waiting = app.waiting(&cards);
            boards.push(Board {
                waiting,
                cards: cards
                    .into_iter()
                    .map(|card| card_view(app, card))
                    .collect::<Result<_, _>>()?,
                round_limit,
                changes: app.store.changes(repository.id)?,
                repository,
            });
        }
        Ok(Snapshot {
            boards,
            harness_changes: app.store.harness_changes()?,
            inbox: app.store.inbox()?,
        })
    })
    .await
}

async fn create_issue(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(issue): axum::Json<NewIssue>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.create_issue(id, &issue.title, &issue.body, PERSON)
    })
    .await
}

async fn edit_card(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(edited): axum::Json<EditedCard>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.edit_card(&by, id, &edited.title, &edited.body, &edited.reason)
    })
    .await
}

async fn approve_issue(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(approval): axum::Json<Reason>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.approve_issue(&by, id, &approval.reason)
    })
    .await
}

async fn merge_change(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(merge): axum::Json<Reason>,
) -> Result<axum::Json<Merged>, ApiError> {
    blocking(&state.app, move |app| {
        app.merge_change(&by, id, &merge.reason)
            .map(|commit| Merged { commit })
    })
    .await
}

async fn retry_card(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(retry): axum::Json<Reason>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.retry_card(&by, id, &retry.reason)
    })
    .await
}

async fn reject_change(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(rejection): axum::Json<Reason>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.reject_change(&by, id, &rejection.reason)
    })
    .await
}

async fn card_runs(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Vec<Run>>, ApiError> {
    blocking(&state.app, move |app| {
        app.card(id)?;
        Ok(app.store.runs_for_card(id)?)
    })
    .await
}

async fn card_history(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Vec<HistoryEntry>>, ApiError> {
    blocking(&state.app, move |app| app.card_history(id)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repos::git;
    use crate::server::api::testing::{code_and_offer, get, origin, post, start, until};
    use crate::store::Usage;

    #[tokio::test]
    async fn the_snapshot_holds_what_a_board_shows_of_each_card() {
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let store = &app.store;
        let blocker = store
            .add_card(repo.id, "Blocker", "", "person", "Ready")
            .unwrap();
        let card = store
            .add_card(repo.id, "Work", "", "person", "In progress")
            .unwrap();
        let done = store
            .add_card(repo.id, "Done", "", "person", "Done")
            .unwrap();
        // Blocked by an open card and by an issue that has no card; the card of a done issue does not block.
        store
            .set_dependencies(card.id, &[blocker.number, 99, done.number].into())
            .unwrap();
        store
            .set_dependencies(done.id, &[blocker.number].into())
            .unwrap();

        // An earlier round, a retry after it, then a round, its review, and a second review that is working.
        let before = store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        store.set_run_status(before.id, "failed").unwrap();
        store.set_card_retried(card.id).unwrap();
        let code = store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        store.set_run_status(code.id, "succeeded").unwrap();
        let review = store
            .add_run(Some(card.id), "Reviewer", "test", "review")
            .unwrap();
        store
            .set_run_summary(review.id, "Looks right.\nVERDICT: approve")
            .unwrap();
        store.set_run_status(review.id, "succeeded").unwrap();
        let live = store
            .add_run(Some(card.id), "Reviewer", "test", "review")
            .unwrap();
        store.set_run_status(live.id, "running").unwrap();
        for (tool, decision) in [("bash", "refuse"), ("read", "allow")] {
            let event = serde_json::json!({"tool": tool, "decision": decision});
            app.record_run_event(live.id, "tool_check", &event).unwrap();
        }
        for (run, input, output) in [(before.id, 1000, 1), (code.id, 100, 50), (live.id, 7, 3)] {
            let usage = Usage {
                run_id: Some(run),
                model: "fake".into(),
                input_tokens: input,
                output_tokens: output,
                cached_tokens: 0,
                duration_ms: 0,
                tools: Vec::new(),
            };
            store.add_usage(&usage).unwrap();
        }
        let change = store
            .add_change(repo.id, Some(card.id), "moebius/card-2", None, "abc")
            .unwrap();
        store
            .set_local_checks(change.id, "failure", "abc", None)
            .unwrap();
        store
            .add_inbox_item("stuck", Some(card.id), "Work", &serde_json::json!({}))
            .unwrap();

        let snap: Snapshot = reqwest::Client::new()
            .get(format!("http://{addr}/v1/snapshot"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let board = snap
            .boards
            .iter()
            .find(|b| b.repository.id == repo.id)
            .unwrap();
        assert_eq!(board.round_limit, 5);
        let view = |number| {
            board
                .cards
                .iter()
                .find(|c| c.card.number == number)
                .unwrap()
        };

        let shown = view(card.number);
        assert_eq!((shown.rounds, shown.tokens), (1, 160));
        assert_eq!(shown.blocked_by, [blocker.number, 99]);
        // The latest run is the one working, and one of its two tool calls was refused.
        let run = shown.run.as_ref().unwrap();
        assert_eq!(
            (run.id, run.agent.as_str(), run.status.as_str()),
            (live.id, "Reviewer", "running")
        );
        assert_eq!(shown.refusals, 1);
        let change = shown.change.as_ref().unwrap();
        assert_eq!(
            (change.state.as_str(), change.checks.as_str()),
            ("open", "failure")
        );
        assert_eq!(change.review_verdict.as_deref(), Some("approve"));
        assert_eq!(snap.inbox[0].card_id, Some(card.id));

        let blocked = view(blocker.number);
        assert_eq!(
            (
                blocked.rounds,
                blocked.run.as_ref(),
                blocked.change.as_ref()
            ),
            (0, None, None)
        );
        // A card in Done carries no details.
        let finished = view(done.number);
        assert_eq!(
            (finished.rounds, finished.tokens, finished.blocked_by.len()),
            (0, 0, 0)
        );
    }

    #[tokio::test]
    async fn an_issue_becomes_a_card_and_its_merged_change_moves_it_to_done() {
        let (dir, state, addr) = start().await;
        let origin = origin(dir.path());

        let client = reqwest::Client::new();
        let post = |path: String, body: serde_json::Value| {
            client
                .post(format!("http://{addr}{path}"))
                .bearer_auth(&*state.token)
                .json(&body)
                .send()
        };
        let repo: Repository = post(
            "/v1/repositories".into(),
            serde_json::json!({"name": "demo", "url": origin.to_str().unwrap()}),
        )
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(
            (repo.forge.as_str(), repo.default_branch.as_str()),
            ("none", "main")
        );
        let card: Card = post(
            format!("/v1/repositories/{}/issues", repo.id),
            serde_json::json!({"title": "Add a feature", "body": "Please."}),
        )
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(card.column, "Backlog");

        let app = state.app.clone();
        let clone = std::path::PathBuf::from(&repo.path);
        let change = code_and_offer(&app, &repo, &card);
        assert_eq!(app.store.inbox().unwrap().len(), 1);

        let merged = post(
            format!("/v1/changes/{}/merge", change.id),
            serde_json::json!({"reason": "looks good"}),
        )
        .await
        .unwrap();
        assert_eq!(merged.status(), 200);
        let merged: Merged = merged.json().await.unwrap();
        assert_eq!(git(&clone, &["rev-parse", "main"]).unwrap(), merged.commit);
        assert!(clone.join("feature.txt").is_file());

        let snap: Snapshot = client
            .get(format!("http://{addr}/v1/snapshot"))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let board = snap
            .boards
            .iter()
            .find(|b| b.repository.id == repo.id)
            .unwrap();
        assert_eq!(board.cards[0].card.column, "Done");
        assert!(board.waiting.is_empty());
        assert_eq!(board.changes[0].state, "merged");
        assert!(snap.inbox.is_empty());
        let judgements = app.store.judgements("merge").unwrap();
        assert_eq!(
            (judgements.len(), judgements[0].decider.as_str()),
            (1, "person")
        );

        let again = post(
            format!("/v1/changes/{}/merge", change.id),
            serde_json::json!({}),
        )
        .await
        .unwrap();
        assert_eq!(again.status(), 422);
    }

    #[tokio::test]
    async fn a_person_retries_a_stuck_card_once() {
        let (_dir, state, addr) = start().await;
        let store = &state.app.store;
        let card = store.cards(1).unwrap().pop().unwrap();
        let retry = |reason: &str| {
            post(
                &state,
                &addr,
                format!("/v1/cards/{}/retry", card.id),
                serde_json::json!({ "reason": reason }),
            )
        };
        // Nothing to retry before the card is stuck, and no such card.
        assert_eq!(retry("again").await.status(), 422);
        let missing = post(
            &state,
            &addr,
            "/v1/cards/99/retry".into(),
            serde_json::json!({}),
        )
        .await;
        assert_eq!(missing.status(), 404);

        let run = store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        store
            .add_inbox_item("stuck", Some(card.id), "First", &serde_json::json!({}))
            .unwrap();
        state
            .app
            .set_waiting(card.id, Some("the round limit is reached".into()));
        let retried: Card = retry("the flaky test is fixed").await.json().await.unwrap();
        assert_eq!(retried.retried_after, Some(run.id));
        assert!(store.inbox().unwrap().is_empty());
        assert!(state.app.waiting(std::slice::from_ref(&card)).is_empty());
        let judgements = store.judgements("retry").unwrap();
        assert_eq!(
            (
                judgements.len(),
                judgements[0].decider.as_str(),
                judgements[0].choice.as_str(),
                judgements[0].reason.as_str(),
                judgements[0].card_id
            ),
            (
                1,
                "person",
                "retry",
                "the flaky test is fixed",
                Some(card.id)
            )
        );
        assert_eq!(retry("again").await.status(), 422);
    }

    #[tokio::test]
    async fn a_person_rejects_a_change_waiting_to_be_merged() {
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let card = app
            .create_issue(repo.id, "Add a feature", "", PERSON)
            .unwrap();
        let reject = |id: i64, reason: &str| {
            post(
                &state,
                &addr,
                format!("/v1/changes/{id}/reject"),
                serde_json::json!({ "reason": reason }),
            )
        };
        assert_eq!(reject(99, "no").await.status(), 404);

        let change = code_and_offer(app, &repo, &card);
        let rejected: Card = reject(change.id, "Add a test.").await.json().await.unwrap();
        assert_eq!(rejected.column, "In progress");
        assert_eq!(rejected.note.as_deref(), Some("Add a test."));
        assert!(app.store.inbox().unwrap().is_empty());
        assert_eq!(app.store.change(change.id).unwrap().unwrap().state, "open");
        let judgements = app.store.judgements("reject").unwrap();
        assert_eq!(
            (
                judgements.len(),
                judgements[0].decider.as_str(),
                judgements[0].reason.as_str()
            ),
            (1, "person", "Add a test.")
        );
        // The card no longer waits to be merged.
        assert_eq!(reject(change.id, "again").await.status(), 422);

        // A change that was merged cannot be rejected either.
        app.merge_change(&crate::decide::Actor::Person, change.id, "fine")
            .unwrap();
        assert_eq!(reject(change.id, "too late").await.status(), 422);
    }

    #[tokio::test]
    async fn a_card_has_its_runs_and_its_history_in_time_order() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        // An inbox item, then a wait until the clock has moved on, so that time order differs from the order
        // in which the history gathers its parts.
        let item = app
            .store
            .add_inbox_item("stuck", Some(1), "Stuck", &serde_json::json!({}))
            .unwrap();
        let first = item.at;
        until("the next second", || crate::store::now() > first).await;
        app.store.set_card_note(1, "Add a test.").unwrap();
        app.move_card(1, "Ready").unwrap();
        app.move_card(1, "In progress").unwrap();
        let run = app.store.add_run(Some(1), "Coder", "test", "code").unwrap();
        app.store.set_run_status(run.id, "failed").unwrap();
        app.store
            .add_judgement(&crate::store::NewJudgement {
                kind: "retry".into(),
                choice: "retry".into(),
                reason: "again".into(),
                decider: "person".into(),
                card_id: Some(1),
                ..Default::default()
            })
            .unwrap();
        app.store.resolve_inbox_item(item.id).unwrap();

        let runs: Vec<Run> = get(&state, &addr, "/v1/cards/1/runs").await;
        assert_eq!(runs.iter().map(|r| r.id).collect::<Vec<_>>(), [run.id]);
        let history: Vec<HistoryEntry> = get(&state, &addr, "/v1/cards/1/history").await;
        let kinds: Vec<_> = history.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "inbox_added",
                "moved",
                "moved",
                "run_started",
                "run_ended",
                "decided",
                "inbox_resolved"
            ]
        );
        assert!(history.windows(2).all(|w| w[0].at <= w[1].at));
        assert_eq!(
            history[2].data,
            serde_json::json!({ "from": "Ready", "to": "In progress", "note": "Add a test." })
        );
        assert_eq!(
            history[4].data,
            serde_json::json!({ "run": run.id, "status": "failed" })
        );
        assert_eq!(history[5].data["decider"], "person");
        assert_eq!(history[6].data["item"], item.id);
    }
}
