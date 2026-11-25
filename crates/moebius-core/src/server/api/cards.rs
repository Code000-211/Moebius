//! The boards and their cards: what a client reads to draw every board, and what a person does to a card and its
//! change.
//!
//! - `GET /v1/snapshot`: every repository with its cards (and what the board shows of each), and the open inbox; of a
//!   private project the person is no member of, nothing (see `app::projects::sight`).
//! - `POST /v1/repositories/{id}/issues` `{title, body, parent, labels, blocked_by}`: creates an issue, which becomes a
//!   card in the backlog, answered with the card and, when the forge refused a step after the issue was created, its
//!   `warnings`. The issue tracker's other routes are in `issues`.
//! - `GET /v1/cards/by-key/{key}`: the issue a card key such as `WEB-24` names, as `GET /v1/cards/{id}` answers it.
//! - `PUT /v1/cards/{id}` `{title, body}`: changes a card's title and body, and its issue on a forge.
//! - `POST /v1/cards/{id}/approve` `{reason}`: a person approves an outside issue; its card moves to ready.
//! - `POST /v1/cards/{id}/block` `{reason}` and `POST /v1/cards/{id}/unblock` `{reason}`: a person or the Moebius agent
//!   holds a card, which stops its runs and keeps the board from working on it, or releases it.
//! - `DELETE /v1/cards/{id}`: a person deletes a card with everything stored for it.
//! - `POST /v1/changes/{id}/merge` `{reason}`: a person merges a change; its card moves to done.
//! - `POST /v1/changes/{id}/reject` `{reason}`: a person sends a change waiting to be merged back to coding.
//! - `POST /v1/changes/{id}/review/end` `{reason}`: a person ends the review round of a followed pull request that is going.
//! - `POST /v1/changes/{id}/review` `{reason}`: a person has the Reviewer read a pull request someone else opened that a
//!   card follows.
//! - `GET /v1/changes/{id}/diff`: what a change changes, as a unified diff.
//! - `POST /v1/cards/{id}/retry` `{reason}`: a person retries a stuck card or one whose pull request was closed, which
//!   gets a fresh start.
//! - `POST /v1/cards/{id}/done` `{answer, reason}`: a person answers the question an assignee's `/done` comment asked
//!   (`close`, `give_back` or `release`); `POST /v1/cards/{id}/release-taken` `{reason}`: a person takes the assignee
//!   off an issue that was taken with `/take` (see `app::commands`).
//! - `GET /v1/cards/{id}/runs`: the runs of a card. `GET /v1/cards/{id}/history`: its timeline.

use std::collections::BTreeMap;

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, Reason, blocking, scope};
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
        .route(
            "/v1/cards/by-key/{key}",
            get(super::issues::show_issue_by_key),
        )
        .route("/v1/cards/{id}/approve", post(approve_issue))
        .route("/v1/cards/{id}/block", post(block_card))
        .route("/v1/cards/{id}/unblock", post(unblock_card))
        .route("/v1/cards/{id}/retry", post(retry_card))
        .route("/v1/cards/{id}/done", post(answer_done))
        .route("/v1/cards/{id}/release-taken", post(release_taken))
        .route("/v1/changes/{id}/merge", post(merge_change))
        .route("/v1/changes/{id}/reject", post(reject_change))
        .route("/v1/changes/{id}/review", post(review_change))
        .route("/v1/changes/{id}/review/end", post(end_review_round))
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
    /// The board of each project that has cards here, by project id: its columns left to right with the class each
    /// counts as. A project without custom columns has the six built-in ones.
    #[serde(default)]
    pub project_columns: BTreeMap<i64, Vec<crate::app::columns::ColumnView>>,
}

/// A card with what a board shows of it, so that a client needs one read for a board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardView {
    #[serde(flatten)]
    pub card: Card,
    /// The built-in column the card's column counts as: the column itself for a built-in one.
    #[serde(default)]
    pub column_class: String,
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
    /// The card's open change in its own issue's repository.
    pub change: Option<ChangeView>,
    /// Every open change of the card, one per repository, in the order they are merged; `change` is among them.
    #[serde(default)]
    pub changes: Vec<ChangeView>,
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
    /// The name of the repository the change is in.
    pub repository: String,
    pub state: String,
    /// `pending`, `success`, `failure` or `none`, as column scripts see it.
    pub checks: String,
    /// `approve` or `changes` once a review finished since the latest `code` run.
    pub review_verdict: Option<String>,
    /// The forge login of the person who opened the change's pull request when someone on another server did and the
    /// card follows it (ADR 0027); left out for Moebius's own change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outside_author: Option<String>,
    /// The latest round of the Reviewer on a followed pull request that is going or was written on the forge (ADR 0029);
    /// left out for Moebius's own change and while nobody asked for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_review: Option<ExternalReviewView>,
}

/// A round of the Reviewer on a followed pull request, as the card window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalReviewView {
    /// The round's place among the pull request's rounds, from 1; 0 until it starts.
    pub round: i64,
    /// How many rounds a pull request gets.
    pub round_limit: i64,
    /// `wanted` (waiting for the checks), `running`, `approved` (waiting for the checks to merge), `posted`, `merged`,
    /// `stale` or `failed`.
    pub state: String,
    /// `approve` or `changes` once the Reviewer concluded.
    pub verdict: Option<String>,
    pub summary: String,
    /// The comments on lines of the diff.
    pub comments: Vec<crate::store::ReviewComment>,
    /// Where the review is on the forge, once it is written there.
    pub url: Option<String>,
    /// The commit the round read.
    pub head: String,
    /// Whether the round read the pull request as it is now.
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub boards: Vec<Board>,
    /// The changes of the harness configuration repository, which belongs to no board.
    pub harness_changes: Vec<Change>,
    pub inbox: Vec<InboxItem>,
    /// Who paused all agents and when, while they are paused; left out otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<crate::store::Paused>,
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
    /// The kind of the issue, one its project offers.
    #[serde(default)]
    pub kind: Option<String>,
    /// Values for the fields of the kind.
    #[serde(default)]
    pub fields: std::collections::BTreeMap<String, serde_json::Value>,
    /// Starts from the kind's template: its title and body are used where the issue has none, and its field values
    /// where `fields` has none.
    #[serde(default)]
    pub use_template: bool,
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
fn card_view(
    lookups: &crate::app::CardLookups,
    sight: &crate::app::Sight,
    facts: &BoardFacts,
    card: Card,
) -> crate::app::Result<CardView> {
    let counts_as_sub_issue = lookups.counts_as_sub_issue(&card);
    let mut view = CardView {
        column_class: String::new(),
        card,
        counts_as_sub_issue,
        rounds: 0,
        tokens: 0,
        run: None,
        blocked_by: Vec::new(),
        refusals: 0,
        change: None,
        changes: Vec::new(),
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
    view.blocked_by = lookups.blocked_by_seen(&view.card, sight);
    if let Some(run) = &card_facts.run {
        view.refusals = facts.refusals.get(&run.id).copied().unwrap_or(0);
        view.run = Some(RunView {
            id: run.id,
            agent: run.agent.clone(),
            purpose: run.purpose.clone(),
            status: run.status.clone(),
        });
    }
    let verdict = latest_review(runs).map(|r| r.verdict.to_string());
    for change in lookups.open_changes(&view.card)? {
        let shown = ChangeView {
            id: change.id,
            repository: match change.repository_id {
                Some(id) => lookups.repository_name(id)?,
                None => String::new(),
            },
            state: change.state.clone(),
            checks: lookups.checks_of(&change)?.to_string(),
            // A review of a pull request someone else opened counts for the commit it read, not for a later push.
            review_verdict: verdict.clone().filter(|_| change.review_is_current()),
            outside_author: change.outside_author.clone(),
            external_review: match change.outside_author {
                Some(_) => lookups
                    .external_reviews(change.id)?
                    .into_iter()
                    .rev()
                    .filter(|r| r.kind == "author")
                    .find(|r| r.state != "failed" && r.state != "stale" || r.verdict.is_some())
                    .map(|r| ExternalReviewView {
                        current: change.head.as_deref() == Some(r.head.as_str()),
                        round: r.round,
                        round_limit: crate::app::ROUND_LIMIT,
                        state: r.state,
                        verdict: r.verdict,
                        summary: r.summary,
                        comments: r.comments,
                        url: r.url,
                        head: r.head,
                    }),
                None => None,
            },
        };
        if change.repository_id == Some(view.card.repository_id) {
            view.change = Some(shown.clone());
        }
        view.changes.push(shown);
    }
    Ok(view)
}

async fn snapshot(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
) -> Result<axum::Json<Snapshot>, ApiError> {
    let round_limit = state.config.get().round_limit;
    blocking(&state.app, move |app| build_snapshot(app, &by, round_limit)).await
}

/// What `GET /v1/snapshot` answers to `by`. The cards, changes and the rest the views are made from are read once for
/// all of them, so the number of reads grows with the number of repositories and not with the number of cards.
fn build_snapshot(app: &App, by: &Actor, round_limit: u32) -> crate::app::Result<Snapshot> {
    {
        let sight = app.sight(by)?;
        let mut boards = Vec::new();
        let repositories = app.store.repositories()?;
        // Every card and change is read once; the views below are made from what was read.
        let mut cards = std::collections::HashMap::new();
        let mut changes = std::collections::HashMap::new();
        for repository in &repositories {
            changes.insert(repository.id, app.store.changes(repository.id)?);
            if sight.repository(repository.id) {
                cards.insert(repository.id, app.store.cards(repository.id)?);
            }
        }
        let lookups = app.card_lookups(&repositories, &cards, &changes)?;
        // A card over several repositories is on its own repository's board, and its change in another repository is
        // on that repository's board, so a change is left out only when its card is hidden or archived everywhere.
        let mut seen = std::collections::HashSet::new();
        for card in cards.values().flatten() {
            if card.archived_at.is_none() && sight.card(card) {
                seen.insert(card.id);
            }
        }
        for repository in repositories {
            if !sight.repository(repository.id) {
                continue;
            }
            // An archived card is off the board.
            let mut board_cards = cards.remove(&repository.id).unwrap_or_default();
            board_cards.retain(|card| card.archived_at.is_none() && sight.card(card));
            let waiting = app.waiting(&board_cards);
            let facts = app.board_facts(repository.id, &board_cards)?;
            let mut project_columns = BTreeMap::new();
            let mut classes = std::collections::HashMap::new();
            for card in &board_cards {
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    project_columns.entry(card.project_id)
                {
                    let board = app.project_columns_of(card.project_id)?;
                    entry.insert(board.views());
                    classes.insert(card.project_id, board);
                }
            }
            let class_of = |card: &Card| {
                classes
                    .get(&card.project_id)
                    .and_then(|board| board.class_of(&card.column))
                    .unwrap_or(crate::app::columns::BACKLOG)
                    .to_string()
            };
            boards.push(Board {
                waiting,
                project_columns,
                cards: board_cards
                    .into_iter()
                    .map(|card| {
                        let class = class_of(&card);
                        let mut view = card_view(&lookups, &sight, &facts, card)?;
                        view.column_class = class;
                        Ok::<_, crate::app::AppError>(view)
                    })
                    .collect::<Result<_, _>>()?,
                round_limit,
                changes: changes
                    .remove(&repository.id)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|change| change.card_id.is_none_or(|card| seen.contains(&card)))
                    .collect(),
                repository,
            });
        }
        let mut inbox = Vec::new();
        for item in app.store.inbox()? {
            if super::privacy::sees_inbox_item(app, &sight, &item)? {
                let item = super::privacy::seen_digest(app, &sight, item)?;
                inbox.push(app.unquoted_item(&sight, item)?);
            }
        }
        Ok(Snapshot {
            boards,
            harness_changes: app.store.harness_changes()?,
            inbox,
            paused: app.store.runs_paused()?,
        })
    }
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
        let (title, body, fields) = app.issue_start(
            issue.kind.as_deref(),
            issue.use_template,
            &issue.title,
            &issue.body,
            &issue.fields,
        )?;
        let (mut card, mut warnings) = app.create_tracked_issue(
            &by,
            id,
            &title,
            &body,
            issue.parent,
            &issue.labels,
            &issue.blocked_by,
        )?;
        if let Some(kind) = &issue.kind {
            let set = app.set_card_kind(card.id, Some(kind)).and_then(|card| {
                if fields.is_empty() {
                    Ok(card)
                } else {
                    app.set_card_fields(card.id, &fields)
                }
            });
            match set {
                Ok(done) => card = done,
                Err(e) => warnings.push(format!("kind {kind}: {e}")),
            }
        }
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

async fn review_change(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<crate::store::Run>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.review_outside_change(&by, &*runs, id, &reason)
    })
    .await
}

/// What `POST /v1/changes/{id}/review/end` answers.
#[derive(Debug, Clone, Serialize)]
struct EndedRounds {
    ended: usize,
}

async fn end_review_round(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<EndedRounds>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        app.end_review_round(&by, id, &reason)
            .map(|ended| EndedRounds { ended })
    })
    .await
}

/// The body of `POST /v1/cards/{id}/done`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct DoneAnswer {
    /// `close`, `give_back` or `release`.
    answer: String,
    #[serde(default)]
    reason: String,
}

async fn answer_done(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(done): axum::Json<DoneAnswer>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| {
        app.answer_done(&by, id, &done.answer, &done.reason)
    })
    .await
}

async fn release_taken(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Card>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| app.release_taken(&by, id, &reason)).await
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
) -> Result<axum::Json<Option<Card>>, ApiError> {
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
    use crate::app::OWNER_AUTHOR;
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
        let repositories = app.store.repositories().unwrap();
        let lookups = app
            .card_lookups(
                &repositories,
                &std::collections::HashMap::from([(repo.id, cards.clone())]),
                &std::collections::HashMap::from([(repo.id, store.changes(repo.id).unwrap())]),
            )
            .unwrap();
        let view = card_view(
            &lookups,
            &crate::app::Sight::everything(),
            &facts,
            card.clone(),
        )
        .unwrap();
        assert_eq!(view.run.unwrap().id, code.id);
        assert_eq!(view.refusals, 1);
        assert_eq!(view.attempts.len(), 1);
        // The batched facts say what the facts of the one card say.
        assert_eq!(facts.cards[&card.id], app.card_facts(&card).unwrap());
    }

    /// Adds `count` cards to `repo`: each is blocked by the one before, every third was approved by a person, and every
    /// second has an open change.
    fn fill_board(app: &App, repo: &crate::store::Repository, count: usize) -> Vec<Card> {
        let store = &app.store;
        let mut cards: Vec<Card> = Vec::new();
        for i in 0..count {
            let column = if i % 5 == 0 { "Done" } else { "In review" };
            let card = store
                .add_card(repo.id, &format!("Card {i}"), "", "outsider", column)
                .unwrap();
            if let Some(before) = cards.last() {
                store.add_dependency(card.id, before.number).unwrap();
            }
            if i % 3 == 0 {
                store
                    .add_judgement(&crate::store::NewJudgement {
                        kind: "approve_issue".into(),
                        choice: "approve".into(),
                        card_id: Some(card.id),
                        ..Default::default()
                    })
                    .unwrap();
            }
            if i % 2 == 0 {
                store
                    .add_change(repo.id, Some(card.id), &format!("b{i}"), None, "head")
                    .unwrap();
            }
            cards.push(card);
        }
        cards
    }

    #[tokio::test]
    async fn the_snapshot_reads_the_store_a_number_of_times_that_does_not_grow_with_the_cards() {
        let (dir, state, _addr) = start().await;
        let app = &state.app;
        let sight_of = Actor::Script {
            column: "In review".into(),
        };
        let reads = |app: &App| {
            let before = crate::store::connections_taken();
            let snapshot = build_snapshot(app, &sight_of, 3).unwrap();
            (crate::store::connections_taken() - before, snapshot)
        };
        let small = app
            .add_repository("small", origin(dir.path()).to_str().unwrap())
            .unwrap();
        fill_board(app, &small, 10);
        // The first snapshot also reads the project's columns, which the app keeps for the ones after it.
        reads(app);
        let (few, _) = reads(app);
        fill_board(app, &small, 190);
        let (many, snapshot) = reads(app);
        let board = snapshot
            .boards
            .iter()
            .find(|b| b.repository.id == small.id)
            .unwrap();
        assert_eq!(board.cards.len(), 200);
        assert_eq!(few, many, "10 cards took {few} reads, 200 took {many}");
    }

    #[tokio::test]
    async fn a_card_view_made_from_the_lookups_says_what_the_calls_for_one_card_say() {
        let (dir, state, _addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let cards = fill_board(app, &repo, 30);
        // A blocker the forge was seen to close, and one that does not exist.
        app.store.add_dependency(cards[1].id, 900).unwrap();
        app.store.add_dependency(cards[1].id, 901).unwrap();
        app.store
            .set_forge_seen(repo.id, 900, "state", "closed")
            .unwrap();
        let repositories = app.store.repositories().unwrap();
        let all = app.store.cards(repo.id).unwrap();
        let lookups = app
            .card_lookups(
                &repositories,
                &std::collections::HashMap::from([(repo.id, all.clone())]),
                &std::collections::HashMap::from([(repo.id, app.store.changes(repo.id).unwrap())]),
            )
            .unwrap();
        let sight = crate::app::Sight::everything();
        for card in &all {
            assert_eq!(
                lookups.counts_as_sub_issue(card),
                app.counts_as_sub_issue(card)
            );
            assert_eq!(
                lookups.blocked_by_seen(card, &sight),
                app.blocked_by_seen(card, &sight).unwrap(),
                "blockers of {}",
                card.number
            );
            assert_eq!(
                lookups.open_changes(card).unwrap(),
                app.open_changes(card).unwrap()
            );
        }
        let blocked = lookups.blocked_by_seen(&all[1], &sight);
        assert_eq!(
            blocked,
            [901],
            "900 is closed on the forge, and card 1 is done"
        );
    }

    #[tokio::test]
    async fn a_waiting_reason_the_card_already_has_is_not_published_again() {
        let (_dir, state, _addr) = start().await;
        let app = &state.app;
        let mut events = app.bus.subscribe();
        for reason in [
            Some("reviewing"),
            Some("reviewing"),
            None,
            None,
            Some("reviewing"),
        ] {
            app.set_waiting(7, reason.map(String::from));
        }
        let mut published = Vec::new();
        while let Ok(event) = events.try_recv() {
            if event.kind == "card_waiting" {
                published.push(event.data["reason"].clone());
            }
        }
        assert_eq!(
            published,
            [
                serde_json::json!("reviewing"),
                serde_json::Value::Null,
                serde_json::json!("reviewing")
            ]
        );
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
    async fn the_snapshot_leaves_out_an_archived_card_that_the_issue_list_still_has() {
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let repo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let kept = app
            .store
            .add_card(repo.id, "Kept", "", "person", "Done")
            .unwrap();
        let gone = app
            .store
            .add_card(repo.id, "Gone", "", "person", "Done")
            .unwrap();
        assert!(app.archive_card(gone.id).unwrap());
        let get = |path: String| {
            let url = format!("http://{addr}{path}");
            let token = state.token.clone();
            async move {
                reqwest::Client::new()
                    .get(url)
                    .bearer_auth(&*token)
                    .send()
                    .await
                    .unwrap()
                    .json::<serde_json::Value>()
                    .await
                    .unwrap()
            }
        };
        let snap = get("/v1/snapshot".into()).await;
        let board = snap["boards"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["repository"]["id"] == repo.id)
            .unwrap();
        let titles = |cards: &serde_json::Value| -> Vec<String> {
            cards
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["title"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(titles(&board["cards"]), ["Kept"]);
        let issues = get(format!("/v1/repositories/{}/issues", repo.id)).await;
        assert_eq!(titles(&issues), ["Kept", "Gone"]);
        let archived = get(format!("/v1/repositories/{}/issues?archived=true", repo.id)).await;
        assert_eq!(titles(&archived), ["Gone"]);
        assert!(archived[0]["archived_at"].is_number());
        let _ = kept;
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
        assert_eq!(change.repository, repo.name);
        assert_eq!(shown.changes, std::slice::from_ref(change));
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
    async fn the_snapshot_shows_each_open_change_of_a_card_over_two_repositories() {
        let (dir, state, addr) = start().await;
        let app = &state.app;
        let demo = app
            .add_repository("demo", origin(dir.path()).to_str().unwrap())
            .unwrap();
        let second = dir.path().join("second");
        let web = app
            .add_repository("web", origin(&second).to_str().unwrap())
            .unwrap();
        let store = &app.store;
        let card = store
            .add_card(demo.id, "Both", "", "person", "In progress")
            .unwrap();
        store.set_card_repositories(card.id, &[web.id]).unwrap();
        let first = store
            .add_change(demo.id, Some(card.id), "moebius/card-1", None, "abc")
            .unwrap();
        let other = store
            .add_change(web.id, Some(card.id), "moebius/card-WEB-1", None, "def")
            .unwrap();
        store
            .set_local_checks(other.id, "failure", "def", None)
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
            .find(|b| b.repository.id == demo.id)
            .unwrap();
        let shown = &board.cards[0];
        let seen: Vec<_> = shown
            .changes
            .iter()
            .map(|c| (c.id, c.repository.as_str(), c.checks.as_str()))
            .collect();
        assert_eq!(
            seen,
            [(first.id, "demo", "none"), (other.id, "web", "failure")]
        );
        // The change in `web` is on the board of `web`, which does not list the card.
        let on_web = snap
            .boards
            .iter()
            .find(|b| b.repository.id == web.id)
            .unwrap();
        assert_eq!(
            on_web.changes.iter().map(|c| c.id).collect::<Vec<_>>(),
            [other.id]
        );
        // `change` stays the change in the card's own repository.
        assert_eq!(shown.change.as_ref().map(|c| c.id), Some(first.id));
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
        assert_eq!(card.author, "person:owner");

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
            (1, "person:owner")
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
                "person:owner",
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
            .create_issue(repo.id, "Add a feature", "", OWNER_AUTHOR)
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
            .create_issue(repo.id, "Add a feature", "", OWNER_AUTHOR)
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
            (1, "person:owner", "Add a test.")
        );
        // The card no longer waits to be merged.
        assert_eq!(reject(change.id, "again").await.status(), 422);

        // A change that was merged cannot be rejected either.
        app.merge_change(&crate::decide::Actor::first_owner(), change.id, "fine")
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
        app.move_card(1, "Not started", None).unwrap();
        app.move_card(1, "In progress", None).unwrap();
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
            ("person:owner", Some("wait for the design"))
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
            ("person:owner", Some(card.id), "wait for the design")
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
            ("person:owner", None)
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
        let work = |title: &str| app.create_issue(repo.id, title, "", OWNER_AUTHOR).unwrap();
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
