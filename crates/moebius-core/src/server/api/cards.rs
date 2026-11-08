//! The boards and their cards: what a client reads to draw every board, and what a person does to a card and its
//! change.
//!
//! - `GET /v1/snapshot`: every repository with its cards (and what the board shows of each), and the open inbox.
//! - `POST /v1/repositories/{id}/issues` `{title, body, parent, labels, blocked_by}`: creates an issue, which becomes a
//!   card in the backlog, answered with the card and, when the forge refused a step after the issue was created, its
//!   `warnings`. The issue tracker's other routes are in `issues`.
//! - `PUT /v1/cards/{id}` `{title, body}`: changes a card's title and body, and its issue on a forge.
//! - `POST /v1/cards/{id}/approve` `{reason}`: a person approves an outside issue; its card moves to ready.
//! - `POST /v1/cards/{id}/block` `{reason}` and `POST /v1/cards/{id}/unblock` `{reason}`: a person or the Moebius agent
//!   holds a card, which stops its runs and keeps the board from working on it, or releases it.
//! - `DELETE /v1/cards/{id}`: a person deletes a card with everything stored for it.
//! - `POST /v1/changes/{id}/merge` `{reason}`: a person merges a change; its card moves to done.
//! - `POST /v1/changes/{id}/reject` `{reason}`: a person sends a change waiting to be merged back to coding.
//! - `GET /v1/changes/{id}/diff`: what a change changes, as a unified diff.
//! - `POST /v1/cards/{id}/retry` `{reason}`: a person retries a stuck card or one whose pull request was closed, which
//!   gets a fresh start.
//! - `GET /v1/cards/{id}/runs`: the runs of a card. `GET /v1/cards/{id}/history`: its timeline.

use std::collections::BTreeMap;

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, Reason, blocking, person_only, scope};
use crate::app::{App, BoardFacts, HistoryEntry};
use crate::board::latest_review;
use crate::decide::Actor;
use crate::store::{Card, Change, InboxItem, Repository, Run};

/// The routes of the boards and their cards.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/snapshot", get(snapshot))
        .route(
            "/v1/repositories/{id}/issues",
            post(create_issue).get(super::issues::list_issues),
        )
        .route(
            "/v1/cards/{id}",
            get(super::issues::show_issue)
                .put(edit_card)
                .delete(delete_card),
        )
        .route("/v1/cards/{id}/approve", post(approve_issue))
        .route("/v1/cards/{id}/block", post(block_card))
        .route("/v1/cards/{id}/unblock", post(unblock_card))
        .route("/v1/cards/{id}/retry", post(retry_card))
        .route("/v1/changes/{id}/merge", post(merge_change))
        .route("/v1/changes/{id}/reject", post(reject_change))
        .route("/v1/changes/{id}/diff", get(change_diff))
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
    /// Whether the card counts as a sub-issue of its `parent`: a person can have vouched for it (see
    /// `App::counts_as_sub_issue`). One that does not is shown as a card of its own, and holds nothing back.
    pub counts_as_sub_issue: bool,
    /// How many tool calls of the latest run were refused.
    pub refusals: i64,
    /// The card's open change.
    pub change: Option<ChangeView>,
    /// The attempts of the card's latest group, if it was ever tried several times at once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<AttemptView>,
}

/// One attempt of a card tried several times at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptView {
    pub run: i64,
    /// The model or model class the attempt was started with in place of its agent's, if one was given.
    pub model: Option<String>,
    /// The attempt's run: `starting`, `running`, `succeeded`, `failed` or `stopped`.
    pub status: String,
    /// `passed` or `failed` once the tests and checks have judged it.
    pub result: Option<String>,
    /// Why it failed, or empty.
    pub output: String,
    pub picked: bool,
    /// Why it was picked.
    pub reason: Option<String>,
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
    /// The issue number of its parent.
    #[serde(default)]
    pub parent: Option<i64>,
    #[serde(default)]
    pub labels: Vec<String>,
    /// The issue numbers that block it.
    #[serde(default)]
    pub blocked_by: Vec<i64>,
}

/// The answer to `POST /v1/repositories/{id}/issues`: the new issue's card, and what could not be set on it after it was
/// created (its parent, labels or blockers, when the forge refused), which is left out when nothing failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedIssue {
    #[serde(flatten)]
    pub card: Card,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
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

/// What a board shows of a card, from the facts read for its whole board (`App::board_facts`). A card in Done carries
/// none of it, which keeps the snapshot cheap to read on every event.
fn card_view(app: &App, facts: &BoardFacts, card: Card) -> crate::app::Result<CardView> {
    let counts_as_sub_issue = app.counts_as_sub_issue(&card);
    let mut view = CardView {
        card,
        counts_as_sub_issue,
        rounds: 0,
        tokens: 0,
        run: None,
        blocked_by: Vec::new(),
        refusals: 0,
        change: None,
        attempts: Vec::new(),
    };
    let Some(card_facts) = facts.cards.get(&view.card.id) else {
        return Ok(view);
    };
    let runs = &card_facts.runs;
    view.rounds = card_facts.rounds;
    view.tokens = facts.tokens.get(&view.card.id).copied().unwrap_or(0);
    let attempts = facts
        .attempts
        .get(&view.card.id)
        .map_or(&[][..], Vec::as_slice);
    let latest = attempts.iter().map(|a| a.group_id).max();
    view.attempts = attempts
        .iter()
        .filter(|a| Some(a.group_id) == latest)
        .cloned()
        .map(|a| AttemptView {
            run: a.run_id,
            model: a.model,
            status: runs
                .iter()
                .find(|r| r.id == a.run_id)
                .map_or_else(String::new, |r| r.status.clone()),
            result: a.result,
            output: a.output,
            picked: a.picked,
            reason: a.reason,
        })
        .collect();
    view.blocked_by = app.blocked_by(&view.card)?;
    if let Some(run) = &card_facts.run {
        view.refusals = facts.refusals.get(&run.id).copied().unwrap_or(0);
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
            review_verdict: latest_review(runs).map(|r| r.verdict.to_string()),
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
            let facts = app.board_facts(repository.id, &cards)?;
            boards.push(Board {
                waiting,
                cards: cards
                    .into_iter()
                    .map(|card| card_view(app, &facts, card))
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
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(issue): axum::Json<NewIssue>,
) -> Result<axum::Json<CreatedIssue>, ApiError> {
    blocking(&state.app, move |app| {
        scope::repository(app, &by, id)?;
        scope::labels(app, &by, &issue.labels)?;
        for number in issue.parent.iter().chain(&issue.blocked_by) {
            scope::related(app, &by, id, *number)?;
        }
        let (card, warnings) = app.create_tracked_issue(
            id,
            &issue.title,
            &issue.body,
            issue.parent,
            &issue.labels,
            &issue.blocked_by,
        )?;
        Ok(CreatedIssue { card, warnings })
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
        scope::card(app, &by, id, true)?;
        app.edit_card(&by, id, &edited.title, &edited.body, &edited.reason)
    })
    .await
}

async fn block_card(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Card>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.block_card(&by, &*runs, id, Some(&reason))
    })
    .await
}

async fn unblock_card(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Card>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| app.unblock_card(&by, id, &reason)).await
}

async fn delete_card(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Card>, ApiError> {
    person_only(&by)?;
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.delete_card(&by, &*runs, id, &reason)
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
    super::extensions::refuse_agent_merge(&state, &by, id).await?;
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

async fn change_diff(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<crate::app::Diff>, ApiError> {
    blocking(&state.app, move |app| app.change_diff(id)).await
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
    use crate::app::PERSON;
    use crate::repos::git;
    use crate::server::api::testing::{code_and_offer, get, origin, post, start, until};
    use crate::store::Usage;

    #[tokio::test]
    async fn a_card_view_shows_the_latest_run_that_is_not_an_unpicked_attempt() {
        let (_dir, state, _addr) = start().await;
        let app = &state.app;
        let store = &app.store;
        let repo = store.repositories().unwrap().pop().unwrap();
        let card = store
            .add_card(repo.id, "Hard", "", "person", "In progress")
            .unwrap();
        let code = store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        store.set_run_status(code.id, "succeeded").unwrap();
        let attempt = store
            .add_run(Some(card.id), "Coder", "test", "attempt")
            .unwrap();
        store
            .add_attempt(attempt.id, card.id, attempt.id, None)
            .unwrap();
        for (run, decision) in [(code.id, "refuse"), (attempt.id, "refuse")] {
            let event = serde_json::json!({"tool": "bash", "decision": decision});
            app.record_run_event(run, "tool_check", &event).unwrap();
        }
        app.record_run_event(
            attempt.id,
            "tool_check",
            &serde_json::json!({"tool": "bash", "decision": "refuse"}),
        )
        .unwrap();
        let cards = store.cards(repo.id).unwrap();
        let facts = app.board_facts(repo.id, &cards).unwrap();
        let view = card_view(app, &facts, card.clone()).unwrap();
        assert_eq!(view.run.unwrap().id, code.id);
        assert_eq!(view.refusals, 1);
        assert_eq!(view.attempts.len(), 1);
        // The batched facts say what the facts of the one card say.
        assert_eq!(facts.cards[&card.id], app.card_facts(&card).unwrap());
    }

    #[tokio::test]
    async fn a_board_whose_cards_have_no_run_has_no_refusals() {
        let (_dir, state, _addr) = start().await;
        let app = &state.app;
        let repo = app.store.repositories().unwrap().pop().unwrap();
        let cards = app.store.cards(repo.id).unwrap();
        assert!(
            cards
                .iter()
                .all(|c| app.card_facts(c).unwrap().run.is_none())
        );
        let facts = app.board_facts(repo.id, &cards).unwrap();
        assert!(facts.refusals.is_empty());
    }

    #[tokio::test]
    async fn the_snapshot_holds_what_a_board_shows_of_each_card() {
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let store = &app.store;
        let blocker = store
            .add_card(repo.id, "Blocker", "", "person", "Not started")
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
    async fn a_change_and_a_run_give_their_diff() {
        use crate::app::Diff;
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let card = app
            .create_issue(repo.id, "Add a feature", "", PERSON)
            .unwrap();
        let change = code_and_offer(app, &repo, &card);

        let diff: Diff = get(&state, &addr, &format!("/v1/changes/{}/diff", change.id)).await;
        assert!(!diff.truncated);
        assert!(diff.diff.contains("+++ b/feature.txt"), "{}", diff.diff);
        assert!(diff.diff.contains("+done"));
        assert!(!diff.diff.contains("README"));

        // The run's branch is the one `code_and_offer` committed on.
        let run = app.store.runs_for_card(card.id).unwrap().remove(0);
        let diff: Diff = get(&state, &addr, &format!("/v1/runs/{}/diff", run.id)).await;
        assert!(diff.diff.contains("+done"), "{}", diff.diff);

        // A run that is going on shows what its worktree holds, uncommitted work included.
        let live = app
            .store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        let clone = std::path::PathBuf::from(&repo.path);
        let wt = app
            .repos
            .add_worktree(&repo.name, &clone, live.id, "main")
            .unwrap();
        std::fs::write(wt.path.join("README.md"), "hello\nmore\n").unwrap();
        app.store
            .set_run_worktree(live.id, &wt.path.to_string_lossy())
            .unwrap();
        app.store.set_run_status(live.id, "running").unwrap();
        let diff: Diff = get(&state, &addr, &format!("/v1/runs/{}/diff", live.id)).await;
        assert!(diff.diff.contains("+more"), "{}", diff.diff);
        // A file the agent created and has not committed is in it too, and the worktree's index is left alone.
        std::fs::write(wt.path.join("fresh.txt"), "brand new\n").unwrap();
        let diff: Diff = get(&state, &addr, &format!("/v1/runs/{}/diff", live.id)).await;
        assert!(diff.diff.contains("+++ b/fresh.txt"), "{}", diff.diff);
        assert!(diff.diff.contains("+brand new"));
        let status = crate::repos::git(&wt.path, &["status", "--porcelain"]).unwrap();
        assert!(status.contains("?? fresh.txt"), "{status}");

        // A run with nothing to compare, and runs and changes that do not exist.
        let bare = app.store.add_run(None, "Moebius", "test", "chat").unwrap();
        let diff: Diff = get(&state, &addr, &format!("/v1/runs/{}/diff", bare.id)).await;
        assert_eq!(diff, Diff::default());
        let missing = |path: &str| {
            let url = format!("http://{addr}{path}");
            let token = state.token.clone();
            async move {
                reqwest::Client::new()
                    .get(url)
                    .bearer_auth(&*token)
                    .send()
                    .await
                    .unwrap()
                    .status()
            }
        };
        assert_eq!(missing("/v1/changes/999/diff").await, 404);
        assert_eq!(missing("/v1/runs/999/diff").await, 404);
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
        app.move_card(1, "Not started").unwrap();
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
            serde_json::json!({ "from": "Not started", "to": "In progress", "note": "Add a test." })
        );
        assert_eq!(
            history[4].data,
            serde_json::json!({ "run": run.id, "status": "failed" })
        );
        assert_eq!(history[5].data["decider"], "person");
        assert_eq!(history[6].data["item"], item.id);
    }

    #[tokio::test]
    async fn the_attempts_of_a_card_show_in_its_board_and_its_history() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let mut runs = Vec::new();
        for model in [Some("strong"), Some("fast")] {
            let run = app
                .store
                .add_run(Some(1), "Coder", "test", "attempt")
                .unwrap();
            app.store
                .add_attempt(run.id, 1, runs.first().copied().unwrap_or(run.id), model)
                .unwrap();
            app.store.set_run_status(run.id, "succeeded").unwrap();
            runs.push(run.id);
        }
        app.store
            .set_attempt_result(runs[0], "failed", "the test fails")
            .unwrap();
        app.store.set_attempt_result(runs[1], "passed", "").unwrap();
        app.settle_attempts(1, Some(runs[1]), "The only attempt that passed.")
            .unwrap();

        let snapshot: Snapshot = get(&state, &addr, "/v1/snapshot").await;
        let card = snapshot.boards[0]
            .cards
            .iter()
            .find(|c| c.card.id == 1)
            .unwrap();
        assert_eq!(
            card.attempts
                .iter()
                .map(|a| (a.run, a.model.as_deref(), a.result.as_deref(), a.picked))
                .collect::<Vec<_>>(),
            [
                (runs[0], Some("strong"), Some("failed"), false),
                (runs[1], Some("fast"), Some("passed"), true)
            ]
        );
        assert_eq!(card.attempts[0].output, "the test fails");
        assert_eq!(
            card.attempts[1].reason.as_deref(),
            Some("The only attempt that passed.")
        );
        assert_eq!(card.attempts[1].status, "succeeded");

        let history: Vec<HistoryEntry> = get(&state, &addr, "/v1/cards/1/history").await;
        let judged: Vec<_> = history
            .iter()
            .filter(|e| e.kind == "attempt_judged")
            .collect();
        assert_eq!(judged.len(), 2);
        assert_eq!(judged[0].data["run"], runs[0]);
        assert_eq!(judged[0].data["output"], "the test fails");
        let settled = history
            .iter()
            .find(|e| e.kind == "attempts_settled")
            .unwrap();
        assert_eq!(settled.data["picked"], runs[1]);
        assert_eq!(settled.data["runs"], serde_json::json!(runs));
        assert_eq!(settled.data["reason"], "The only attempt that passed.");
    }

    /// A request with the server's token, as the person.
    fn request(
        state: &ApiState,
        addr: &str,
        method: reqwest::Method,
        path: String,
        body: Option<serde_json::Value>,
    ) -> impl std::future::Future<Output = reqwest::Response> {
        let mut request = reqwest::Client::new()
            .request(method, format!("http://{addr}{path}"))
            .bearer_auth(&*state.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        async move { request.send().await.unwrap() }
    }

    #[tokio::test]
    async fn a_card_is_held_and_released_with_its_reason_and_the_errors_are_told() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let card = app.store.cards(1).unwrap().pop().unwrap();
        let mut events = app.bus.subscribe();
        let send = |action: &str, body: Option<serde_json::Value>| {
            request(
                &state,
                &addr,
                reqwest::Method::POST,
                format!("/v1/cards/{}/{action}", card.id),
                body,
            )
        };

        // Nothing to release, and no such card.
        assert_eq!(send("unblock", None).await.status(), 409);
        let missing = request(
            &state,
            &addr,
            reqwest::Method::POST,
            "/v1/cards/99/block".into(),
            None,
        )
        .await;
        assert_eq!(missing.status(), 404);

        let blocked = send(
            "block",
            Some(serde_json::json!({ "reason": "  wait for the design " })),
        )
        .await;
        assert_eq!(blocked.status(), 200);
        let blocked: Card = blocked.json().await.unwrap();
        let held = blocked.held.clone().unwrap();
        assert_eq!(
            (held.by.as_str(), held.reason.as_deref()),
            ("person", Some("wait for the design"))
        );
        // The snapshot shows it, on the card the board shows.
        let snapshot: Snapshot = get(&state, &addr, "/v1/snapshot").await;
        let shown = &snapshot.boards[0].cards[0];
        assert_eq!(shown.card.held, Some(held.clone()));
        assert_eq!(send("block", None).await.status(), 409);

        let judgements = app.store.judgements("block_card").unwrap();
        assert_eq!(judgements.len(), 1);
        assert_eq!(
            (
                judgements[0].decider.as_str(),
                judgements[0].card_id,
                judgements[0].reason.as_str()
            ),
            ("person", Some(card.id), "wait for the design")
        );
        let context: serde_json::Value =
            serde_json::from_str(&app.store.blobs().get(&judgements[0].context[0]).unwrap())
                .unwrap();
        assert_eq!(context["reason"], "wait for the design");

        // Released without a body, it is a card like any other again.
        let released = send("unblock", None).await;
        assert_eq!(released.status(), 200);
        let released: Card = released.json().await.unwrap();
        assert_eq!(released.held, None);
        assert_eq!(app.store.judgements("unblock_card").unwrap().len(), 1);
        assert_eq!(send("unblock", None).await.status(), 409);

        let mut updated = 0;
        while let Ok(event) = events.try_recv() {
            if event.kind == "card_updated" {
                assert_eq!(event.data["id"], card.id);
                updated += 1;
            }
        }
        assert_eq!(
            updated, 2,
            "one card_updated for the block, one for the release"
        );
    }

    #[tokio::test]
    async fn a_person_deletes_a_card_and_every_client_is_told() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let card = app.store.cards(1).unwrap().pop().unwrap();
        let other = app
            .store
            .add_card(card.repository_id, "Other", "", "leon", "Backlog")
            .unwrap();
        let mut events = app.bus.subscribe();
        let delete = |id: i64| {
            request(
                &state,
                &addr,
                reqwest::Method::DELETE,
                format!("/v1/cards/{id}"),
                None,
            )
        };
        assert_eq!(delete(99).await.status(), 404);
        let deleted = delete(card.id).await;
        assert_eq!(deleted.status(), 200);
        let deleted: Card = deleted.json().await.unwrap();
        assert_eq!(deleted.id, card.id);
        assert_eq!(delete(card.id).await.status(), 404);

        let snapshot: Snapshot = get(&state, &addr, "/v1/snapshot").await;
        let left: Vec<i64> = snapshot.boards[0].cards.iter().map(|c| c.card.id).collect();
        assert_eq!(left, [other.id]);
        let judgements = app.store.judgements("delete_card").unwrap();
        assert_eq!(judgements.len(), 1);
        assert_eq!(
            (judgements[0].decider.as_str(), judgements[0].card_id),
            ("person", None)
        );
        let context: serde_json::Value =
            serde_json::from_str(&app.store.blobs().get(&judgements[0].context[0]).unwrap())
                .unwrap();
        assert_eq!(
            (context["title"].as_str(), context["number"].as_str()),
            (Some("First"), Some("1"))
        );
        let mut deleted_events = Vec::new();
        while let Ok(event) = events.try_recv() {
            if event.kind == "card_deleted" {
                deleted_events.push(event.data);
            }
        }
        assert_eq!(deleted_events, [serde_json::json!({ "card": card.id })]);
    }

    // Unix only: the fake Pi is a Python script started by its `#!` line.
    #[cfg(unix)]
    #[tokio::test]
    async fn blocking_a_card_stops_its_run_and_deleting_one_waits_for_its_run_to_end() {
        use crate::board::{Engine, Kanban, Limits};
        use std::sync::Arc;

        let (dir, state, addr) = start().await;
        let app = state.app.clone();
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let engine = Arc::new(Engine::new(
            app.clone(),
            state.runs.clone(),
            Kanban::default(),
            Limits::default(),
        ));
        let settle = |id: i64| {
            let engine = engine.clone();
            async move {
                tokio::task::spawn_blocking(move || engine.settle(id).unwrap())
                    .await
                    .unwrap()
            }
        };
        let work = |title: &str| app.create_issue(repo.id, title, "", PERSON).unwrap();
        let post_to = |id: i64, action: &str| {
            request(
                &state,
                &addr,
                reqwest::Method::POST,
                format!("/v1/cards/{id}/{action}"),
                Some(serde_json::json!({ "reason": "not now" })),
            )
        };

        // A Coder is working on the card, and waits for a person.
        let blocked = work("Please [hang] one");
        settle(blocked.id).await;
        let run = app.store.runs_for_card(blocked.id).unwrap().pop().unwrap();
        until("Pi takes the prompt", || {
            app.store.events(run.id).unwrap().len() >= 2
        })
        .await;
        assert_eq!(post_to(blocked.id, "block").await.status(), 200);
        until("the run ends", || {
            app.store.run(run.id).unwrap().unwrap().status == "stopped"
        })
        .await;
        assert_eq!(app.store.judgements("stop").unwrap().len(), 1);

        // The board starts nothing for a held card and leaves it where it was.
        let before = app.card(blocked.id).unwrap().column;
        let step = settle(blocked.id).await;
        assert_eq!(
            (step.moved, step.started_run, step.waiting),
            (None, None, None)
        );
        assert_eq!(app.store.runs_for_card(blocked.id).unwrap().len(), 1);
        assert_eq!(app.card(blocked.id).unwrap().column, before);
        assert!(app.store.inbox().unwrap().is_empty());

        // Released, its script runs again; the run the block stopped is no reason to call the card stuck, so it starts
        // another one.
        assert_eq!(post_to(blocked.id, "unblock").await.status(), 200);
        let step = settle(blocked.id).await;
        assert_eq!(step.waiting.as_deref(), Some("coding"));
        assert_eq!(app.store.runs_for_card(blocked.id).unwrap().len(), 2);
        assert!(app.store.inbox().unwrap().is_empty());

        // A card with a run that is going is deleted once its run has ended, with its worktree and branch.
        let doomed = work("Please [hang] two");
        settle(doomed.id).await;
        let run = app.store.runs_for_card(doomed.id).unwrap().pop().unwrap();
        until("Pi takes the prompt", || {
            app.store.events(run.id).unwrap().len() >= 2
        })
        .await;
        let clone = std::path::PathBuf::from(&repo.path);
        let worktree = std::path::PathBuf::from(run.worktree.clone().unwrap());
        assert!(worktree.is_dir());
        let branch = format!("moebius/run-{}", run.id);
        assert!(git(&clone, &["rev-parse", "--verify", &branch]).is_ok());

        let deleted = request(
            &state,
            &addr,
            reqwest::Method::DELETE,
            format!("/v1/cards/{}", doomed.id),
            None,
        )
        .await;
        assert_eq!(deleted.status(), 200);
        assert_eq!(app.store.card(doomed.id).unwrap(), None);
        assert_eq!(app.store.run(run.id).unwrap(), None);
        assert!(!worktree.exists());
        assert!(git(&clone, &["rev-parse", "--verify", &branch]).is_err());
        // Without a forge there is no issue to leave behind, and the other card is untouched.
        assert!(!app.store.card_was_deleted(repo.id, doomed.number).unwrap());
        assert!(app.store.card(blocked.id).unwrap().is_some());
    }
}
