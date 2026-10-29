//! Learning from what was recorded: usage, the judgements and their replays, the Retro and the trials of its
//! proposals, the decision model and its takeovers, research and the training-set exports.
//!
//! - `GET /v1/usage?days=`: the tokens, calls, model time and tool waits of the last days, and the daily tokens by model.
//! - `GET /v1/judgements?card=&run=&kind=&limit=`: the recorded decisions, newest first.
//! - `POST /v1/judgements/{id}/replay`: runs a refused tool call again in a throwaway copy of the worktree (see
//!   [`crate::improve::replay`]). `GET /v1/replays?judgement=&run=`: the stored replays, newest first.
//! - `GET /v1/judgements/kinds`: the counts of the stored judgements per kind. `POST /v1/exports` `{kinds?}` writes the
//!   training-set export a person asks for and records the decision; `GET /v1/exports` lists the exports; `GET
//!   /v1/exports/{id}/{file}` downloads a file (`<kind>.jsonl`) of one (see [`crate::improve::export`]).
//! - `POST /v1/retros` `{date?}`, `GET /v1/retros` and `GET /v1/retros/{date}`: the Retro of a day (see
//!   [`crate::improve::retro`]). `GET /v1/proposals`, `POST /v1/proposals/{id}/trial`, `/merge` and `/dismiss`: its
//!   proposals, tried, merged or dismissed (see [`crate::improve::trial`] and, for those that change Moebius's own
//!   configuration, [`crate::improve::own`]); a merged change to `moebius.toml` restarts the server when a key needs it.
//! - `GET /v1/decision-model`: whether a decision model is configured, how well it agrees per kind and its takeovers.
//!   `GET /v1/takeovers`: the takeovers. `POST /v1/takeovers/{id}/merge`, `/dismiss` and `/release` `{reason}`: a person
//!   lets the model take a kind over, refuses, or hands a kind back (see [`crate::decide::model`]).
//! - `POST /v1/research` `{repository, goal, measure, better, token_budget}`: stores a brief as a draft. `GET
//!   /v1/research` and `GET /v1/research/{id}`: research with its baseline and experiments. `PUT /v1/research/{id}`
//!   writes the brief of a draft anew. `POST /v1/research/{id}/confirm`, `/start` and `/stop`: a person confirms the
//!   brief, starts a confirmed research now, or stops it (see [`crate::improve::research`]).

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, Reason, blocking, harness_repo};
use crate::app::AppError;
use crate::decide::Actor;
use crate::decide::model;
use crate::improve::export::{self, Export, ExportError, KindCounts};
use crate::improve::own;
use crate::improve::replay;
use crate::improve::research::{self, NewResearch, ResearchView};
use crate::improve::retro::{self, RetroSummary, RetroView};
use crate::improve::trial;
use crate::store::{
    ActivityUsage, AgentUsage, ClassUsage, ModelUsage, Proposal, Replay, Takeover,
    ToolOutputSaving, ToolWait,
};

/// The routes of usage, judgements, the Retro, the decision model, research and exports.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/usage", get(usage))
        .route("/v1/judgements", get(judgements))
        .route("/v1/judgements/kinds", get(judgement_kinds))
        .route("/v1/judgements/{id}/replay", post(replay_judgement))
        .route("/v1/replays", get(replays))
        .route("/v1/exports", get(exports).post(start_export))
        .route("/v1/exports/{id}/{file}", get(download_export))
        .route("/v1/retros", get(retros).post(start_retro))
        .route("/v1/retros/{date}", get(get_retro))
        .route("/v1/proposals", get(proposals))
        .route("/v1/proposals/{id}/trial", post(try_proposal))
        .route("/v1/proposals/{id}/merge", post(merge_proposal))
        .route("/v1/proposals/{id}/dismiss", post(dismiss_proposal))
        .route("/v1/decision-model", get(decision_model_view))
        .route("/v1/takeovers", get(takeovers))
        .route("/v1/takeovers/{id}/merge", post(merge_takeover))
        .route("/v1/takeovers/{id}/dismiss", post(dismiss_takeover))
        .route("/v1/takeovers/{id}/release", post(release_takeover))
        .route("/v1/research", get(researches).post(create_research))
        .route("/v1/research/{id}", get(get_research).put(edit_research))
        .route("/v1/research/{id}/confirm", post(confirm_research))
        .route("/v1/research/{id}/start", post(start_research))
        .route("/v1/research/{id}/stop", post(stop_research))
}

/// How many judgements one request returns unless it asks for fewer or more.
const DEFAULT_JUDGEMENTS: i64 = 100;
/// The most judgements one request returns.
const MAX_JUDGEMENTS: i64 = 1000;

/// Which judgements to return: those of the card, of the run and of the kind, if given, at most `limit`.
#[derive(Debug, Deserialize)]
struct JudgementsQuery {
    card: Option<i64>,
    run: Option<i64>,
    kind: Option<String>,
    limit: Option<i64>,
}

/// The kinds to export; all of them when not given.
#[derive(Debug, Default, Deserialize)]
struct NewExport {
    kinds: Option<Vec<String>>,
}

/// Which replays to return: those of the judgement and of the run, if given.
#[derive(Debug, Deserialize)]
struct ReplaysQuery {
    judgement: Option<i64>,
    run: Option<i64>,
}

/// How many days the heatmap of the usage screen reaches back.
const HEATMAP_DAYS: i64 = 365;
/// The most tool names `tool_waits` lists.
const TOOL_WAITS: i64 = 10;

/// The window of `GET /v1/usage`: this many days, today included; one (today) when not given.
#[derive(Debug, Deserialize)]
struct UsageQuery {
    days: Option<i64>,
}

/// The usage of a window of days (`docs/reference/desktop-api.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageAnswer {
    /// Today's date on the server, `YYYY-MM-DD`.
    pub today: String,
    pub days_in_window: i64,
    pub totals: UsageTotals,
    /// Every day of the last 365 with usage, oldest first.
    pub days: Vec<DayUsage>,
    pub models: Vec<ModelUsage>,
    pub agents: Vec<AgentUsage>,
    /// What the model did with its time, the activity with most time first.
    pub activities: Vec<ActivityUsage>,
    pub tool_waits: Vec<ToolWait>,
    /// What each model used when a request named a model class, by class.
    #[serde(default)]
    pub classes: Vec<ClassUsage>,
    /// What making tool outputs smaller saved.
    pub tool_output: ToolOutputSaving,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageTotals {
    pub tokens: i64,
    pub calls: i64,
    /// The share of the input tokens that the provider served from its cache, 0 to 1; 0 without input.
    pub cached_share: f64,
    /// The time the model calls took, in milliseconds.
    pub model_ms: i64,
    pub day_budget: Option<i64>,
}

/// A day with usage and the model that used most tokens on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayUsage {
    pub date: String,
    pub tokens: i64,
    pub model: String,
}

/// A judgement as the API shows it: without the blobs of its context.
#[derive(Debug, Serialize, Deserialize)]
struct JudgementEntry {
    id: i64,
    kind: String,
    choice: String,
    reason: String,
    decider: String,
    outcome: Option<String>,
    card: Option<i64>,
    run: Option<i64>,
    at: i64,
}

/// The body of `POST /v1/retros`.
#[derive(Debug, Default, Deserialize)]
struct NewRetro {
    /// The local day to run the Retro of, as `YYYY-MM-DD`; today when left out.
    date: Option<String>,
}

async fn start_retro(
    State(state): State<ApiState>,
    axum::Json(new): axum::Json<NewRetro>,
) -> Result<axum::Json<RetroView>, ApiError> {
    let (runs, repo, live) = (
        state.runs.clone(),
        state.harness_repo.clone(),
        state.config.clone(),
    );
    blocking(&state.app, move |app| {
        let day = match new.date {
            Some(day) => day,
            None => app.store.today()?,
        };
        retro::start(app, (&runs, &live), repo.as_ref(), &day)
    })
    .await
}

async fn retros(State(state): State<ApiState>) -> Result<axum::Json<Vec<RetroSummary>>, ApiError> {
    blocking(&state.app, |app| retro::summaries(&app.store)).await
}

async fn get_retro(
    State(state): State<ApiState>,
    UrlPath(date): UrlPath<String>,
) -> Result<axum::Json<RetroView>, ApiError> {
    blocking(&state.app, move |app| retro::view(&app.store, &date)).await
}

async fn proposals(State(state): State<ApiState>) -> Result<axum::Json<Vec<Proposal>>, ApiError> {
    blocking(&state.app, |app| Ok(app.store.proposals(None)?)).await
}

async fn try_proposal(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Proposal>, ApiError> {
    let (runs, repo) = (state.runs.clone(), harness_repo(&state)?);
    let regression = state.config.get().regression();
    blocking(&state.app, move |app| {
        trial::start(app, &runs, &repo, id, regression)
    })
    .await
}

async fn merge_proposal(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(decision): axum::Json<Reason>,
) -> Result<axum::Json<Proposal>, ApiError> {
    let (repo, live, reason) = (
        state.harness_repo.clone(),
        state.config.clone(),
        decision.reason,
    );
    let restart = live.clone();
    let (proposal, needed) = blocking(&state.app, move |app| {
        let proposal = trial::merge(app, &by, (repo.as_deref(), &live), id, &reason)?;
        let needed = proposal.kind == own::SETTINGS && live.view()?.restart_needed;
        Ok((proposal, needed))
    })
    .await?
    .0;
    if needed {
        tokio::spawn(async move {
            tokio::time::sleep(super::settings::RESTART_DELAY).await;
            restart.request_restart();
        });
    }
    Ok(axum::Json(proposal))
}

async fn dismiss_proposal(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(decision): axum::Json<Reason>,
) -> Result<axum::Json<Proposal>, ApiError> {
    let (repo, reason) = (state.harness_repo.clone(), decision.reason);
    blocking(&state.app, move |app| {
        trial::dismiss(app, &by, repo.as_deref(), id, &reason)
    })
    .await
}

async fn decision_model_view(
    State(state): State<ApiState>,
) -> Result<axum::Json<model::View>, ApiError> {
    blocking(&state.app, |app| model::view(app)).await
}

async fn takeovers(State(state): State<ApiState>) -> Result<axum::Json<Vec<Takeover>>, ApiError> {
    blocking(&state.app, |app| Ok(app.store.takeovers()?)).await
}

async fn merge_takeover(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(decision): axum::Json<Reason>,
) -> Result<axum::Json<Takeover>, ApiError> {
    blocking(&state.app, move |app| {
        model::merge(app, &by, id, &decision.reason)
    })
    .await
}

async fn dismiss_takeover(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(decision): axum::Json<Reason>,
) -> Result<axum::Json<Takeover>, ApiError> {
    blocking(&state.app, move |app| {
        model::dismiss(app, &by, id, &decision.reason)
    })
    .await
}

async fn release_takeover(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(decision): axum::Json<Reason>,
) -> Result<axum::Json<Takeover>, ApiError> {
    blocking(&state.app, move |app| {
        model::release(app, &by, id, &decision.reason)
    })
    .await
}

async fn researches(
    State(state): State<ApiState>,
) -> Result<axum::Json<Vec<ResearchView>>, ApiError> {
    blocking(&state.app, |app| research::views(app)).await
}

async fn create_research(
    State(state): State<ApiState>,
    axum::Json(new): axum::Json<NewResearch>,
) -> Result<axum::Json<ResearchView>, ApiError> {
    blocking(&state.app, move |app| research::create(app, new)).await
}

async fn get_research(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<ResearchView>, ApiError> {
    blocking(&state.app, move |app| research::view(app, id)).await
}

async fn edit_research(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(brief): axum::Json<research::Brief>,
) -> Result<axum::Json<ResearchView>, ApiError> {
    blocking(&state.app, move |app| research::edit(app, &by, id, &brief)).await
}

async fn confirm_research(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<ResearchView>, ApiError> {
    blocking(&state.app, move |app| research::confirm(app, &by, id)).await
}

async fn start_research(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<ResearchView>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        research::start(app, &runs, id, research::MEASURE_LIMIT)
    })
    .await
}

async fn stop_research(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<ResearchView>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| research::stop(app, &*runs, id)).await
}

async fn replay_judgement(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Replay>, ApiError> {
    blocking(&state.app, move |app| {
        replay::replay(app, id, replay::TIME_LIMIT)
    })
    .await
}

async fn replays(
    State(state): State<ApiState>,
    Query(query): Query<ReplaysQuery>,
) -> Result<axum::Json<Vec<Replay>>, ApiError> {
    blocking(&state.app, move |app| {
        Ok(app
            .store
            .replays(query.judgement, query.run, replay::MAX_LISTED)?)
    })
    .await
}

async fn judgements(
    State(state): State<ApiState>,
    Query(query): Query<JudgementsQuery>,
) -> Result<axum::Json<Vec<JudgementEntry>>, ApiError> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_JUDGEMENTS)
        .clamp(1, MAX_JUDGEMENTS);
    blocking(&state.app, move |app| {
        let found =
            app.store
                .judgements_page(query.card, query.run, query.kind.as_deref(), limit)?;
        Ok(found
            .into_iter()
            .map(|j| JudgementEntry {
                id: j.id,
                kind: j.kind,
                choice: j.choice,
                reason: j.reason,
                decider: j.decider,
                outcome: j.outcome,
                card: j.card_id,
                run: j.run_id,
                at: j.at,
            })
            .collect())
    })
    .await
}

fn export_failed(e: ExportError) -> AppError {
    match e {
        ExportError::UnknownKind(_) => AppError::Invalid(e.to_string()),
        ExportError::Store(e) => AppError::Store(e),
        ExportError::Io(e) => AppError::Io(e),
        e @ ExportError::Context(..) => AppError::Io(std::io::Error::other(e.to_string())),
    }
}

async fn judgement_kinds(
    State(state): State<ApiState>,
) -> Result<axum::Json<Vec<KindCounts>>, ApiError> {
    blocking(&state.app, |app| {
        export::counts(&app.store).map_err(export_failed)
    })
    .await
}

async fn exports(State(state): State<ApiState>) -> Result<axum::Json<Vec<Export>>, ApiError> {
    blocking(&state.app, |app| Ok(export::list(&app.data_dir))).await
}

async fn start_export(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    axum::Json(request): axum::Json<NewExport>,
) -> Result<axum::Json<Export>, ApiError> {
    blocking(&state.app, move |app| {
        export::create(app, &by, request.kinds.as_deref()).map_err(export_failed)
    })
    .await
}

/// Streams one file of an export as JSON Lines.
async fn download_export(
    State(state): State<ApiState>,
    UrlPath((id, file)): UrlPath<(i64, String)>,
) -> Result<Response, ApiError> {
    use tokio::io::AsyncReadExt;
    let path = file
        .strip_suffix(".jsonl")
        .and_then(|kind| export::file(&state.app.data_dir, id, kind))
        .ok_or_else(|| AppError::NotFound(format!("file {file} of export {id}")))?;
    let file = tokio::fs::File::open(&path).await.map_err(AppError::from)?;
    let body =
        axum::body::Body::from_stream(futures_util::stream::unfold(file, |mut file| async move {
            let mut chunk = vec![0; 64 * 1024];
            match file.read(&mut chunk).await {
                Ok(0) => None,
                Ok(n) => {
                    chunk.truncate(n);
                    Some((Ok(chunk), file))
                }
                Err(e) => Some((Err(e), file)),
            }
        }));
    Ok(([(header::CONTENT_TYPE, "application/x-ndjson")], body).into_response())
}

async fn usage(
    State(state): State<ApiState>,
    Query(query): Query<UsageQuery>,
) -> Result<axum::Json<UsageAnswer>, ApiError> {
    let days = query.days.unwrap_or(1);
    if !(1..=HEATMAP_DAYS).contains(&days) {
        return Err(AppError::Invalid(format!("days is from 1 to {HEATMAP_DAYS}")).into());
    }
    let day_budget = state.config.get().day_tokens;
    blocking(&state.app, move |app| {
        let store = &app.store;
        let totals = store.usage_totals(days)?;
        let mut by_day: Vec<DayUsage> = vec![];
        let mut leading_tokens = 0;
        for (date, model, tokens) in store.usage_by_day(HEATMAP_DAYS)? {
            match by_day.last_mut().filter(|day| day.date == date) {
                Some(day) => {
                    day.tokens += tokens;
                    if tokens > leading_tokens {
                        day.model = model;
                        leading_tokens = tokens;
                    }
                }
                None => {
                    leading_tokens = tokens;
                    by_day.push(DayUsage {
                        date,
                        tokens,
                        model,
                    });
                }
            }
        }
        Ok(UsageAnswer {
            today: store.today()?,
            days_in_window: days,
            totals: UsageTotals {
                tokens: totals.tokens,
                calls: totals.calls,
                cached_share: if totals.input_tokens > 0 {
                    totals.cached_tokens as f64 / totals.input_tokens as f64
                } else {
                    0.0
                },
                model_ms: totals.model_ms,
                day_budget,
            },
            days: by_day,
            models: store.usage_by_model(days)?,
            agents: store.usage_by_agent(days)?,
            activities: store.usage_by_activity(days)?,
            tool_waits: store.tool_waits(days, TOOL_WAITS)?,
            classes: store.usage_by_class(days)?,
            tool_output: store.tool_output_saving(days)?,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::testing::{get, start};

    #[tokio::test]
    async fn usage_is_read_by_window_with_the_models_agents_activities_and_tool_waits() {
        let (_dir, state, addr) = start().await;
        let store = &state.app.store;
        let coder = store.add_run(None, "Coder", "test", "code").unwrap().id;
        let reviewer = store
            .add_run(None, "Reviewer", "test", "review")
            .unwrap()
            .id;
        for (run, model, input, output, cached, duration_ms, tools) in [
            (Some(coder), "big", 1000, 100, 600, 4000, vec!["bash"]),
            (Some(coder), "big", 500, 50, 0, 2000, vec!["bash", "read"]),
            (Some(reviewer), "small", 200, 20, 100, 1000, vec![]),
            (None, "small", 300, 0, 0, 500, vec!["read"]),
        ] {
            store
                .add_usage(&crate::store::Usage {
                    run_id: run,
                    model: model.into(),
                    input_tokens: input,
                    output_tokens: output,
                    cached_tokens: cached,
                    duration_ms,
                    tools: tools.into_iter().map(String::from).collect(),
                })
                .unwrap();
        }
        let call = serde_json::json!({"toolCallId": "c1", "toolName": "bash"});
        store
            .append_event(coder, "tool_execution_start", &call)
            .unwrap();
        store
            .append_event(coder, "tool_execution_end", &call)
            .unwrap();
        let saved = serde_json::json!({"bytes_before": 8000, "bytes_after": 2000});
        store.append_event(coder, "tool_output", &saved).unwrap();

        let answer: UsageAnswer = get(&state, &addr, "/v1/usage").await;
        assert_eq!(answer.today, store.today().unwrap());
        assert_eq!(answer.days_in_window, 1);
        assert_eq!(
            answer.totals,
            UsageTotals {
                tokens: 2170,
                calls: 4,
                cached_share: 700.0 / 2000.0,
                model_ms: 7500,
                day_budget: Some(10_000),
            }
        );
        // Both models used tokens today; the leading one has 1650 against 520.
        assert_eq!(
            answer.days,
            [DayUsage {
                date: answer.today.clone(),
                tokens: 2170,
                model: "big".into()
            }]
        );
        let models: Vec<_> = answer
            .models
            .iter()
            .map(|m| (m.model.as_str(), m.tokens, m.calls, m.cached_tokens))
            .collect();
        assert_eq!(models, [("big", 1650, 2, 600), ("small", 520, 2, 100)]);
        let agents: Vec<_> = answer
            .agents
            .iter()
            .map(|a| (a.agent.as_str(), a.calls, a.tokens, a.model_ms))
            .collect();
        assert_eq!(
            agents,
            [
                ("Coder", 2, 1650, 6000),
                ("No run", 1, 300, 500),
                ("Reviewer", 1, 220, 1000)
            ]
        );
        let activities: Vec<_> = answer
            .activities
            .iter()
            .map(|a| (a.activity.as_str(), a.model_ms, a.calls, a.output_tokens))
            .collect();
        assert_eq!(
            activities,
            [
                ("bash", 5000, 2, 125),
                ("read", 1500, 2, 25),
                ("answering", 1000, 1, 20)
            ]
        );
        assert_eq!(answer.tool_waits.len(), 1);
        assert_eq!(answer.tool_waits[0].tool, "bash");
        assert_eq!(
            answer.tool_output,
            ToolOutputSaving {
                results: 1,
                bytes_before: 8000,
                bytes_after: 2000
            }
        );

        let week: UsageAnswer = get(&state, &addr, "/v1/usage?days=7").await;
        assert_eq!((week.days_in_window, week.totals.tokens), (7, 2170));
        for days in ["0", "366", "x"] {
            let response = reqwest::Client::new()
                .get(format!("http://{addr}/v1/usage?days={days}"))
                .bearer_auth(&*state.token)
                .send()
                .await
                .unwrap();
            assert!(response.status().is_client_error(), "{days}");
        }
    }

    #[tokio::test]
    async fn judgements_are_read_newest_first_by_card_run_and_kind() {
        let (_dir, state, addr) = start().await;
        let store = &state.app.store;
        let run = store.add_run(None, "Coder", "test", "chat").unwrap().id;
        for (n, kind, card, run) in [
            (1, "retry", Some(1), None),
            (2, "tool_call", None, Some(run)),
            (3, "retry", None, None),
        ] {
            store
                .add_judgement(&crate::store::NewJudgement {
                    kind: kind.into(),
                    choice: format!("choice {n}"),
                    reason: format!("reason {n}"),
                    decider: "person".into(),
                    card_id: card,
                    run_id: run,
                    ..Default::default()
                })
                .unwrap();
        }
        let ids = |path: &str| {
            let path = path.to_string();
            let (state, addr) = (state.clone(), addr.clone());
            async move {
                let found: Vec<serde_json::Value> = get(&state, &addr, &path).await;
                found
                    .iter()
                    .map(|j| j["id"].as_i64().unwrap())
                    .collect::<Vec<_>>()
            }
        };
        assert_eq!(ids("/v1/judgements").await, [3, 2, 1]);
        assert_eq!(ids("/v1/judgements?kind=retry").await, [3, 1]);
        assert_eq!(ids("/v1/judgements?card=1").await, [1]);
        assert_eq!(ids(&format!("/v1/judgements?run={run}")).await, [2]);
        let none = [] as [i64; 0];
        assert_eq!(
            ids(&format!("/v1/judgements?run={run}&kind=retry")).await,
            none
        );
        assert_eq!(ids(&format!("/v1/judgements?run={}", run + 1)).await, none);
        assert_eq!(ids("/v1/judgements?limit=2").await, [3, 2]);
        assert_eq!(ids("/v1/judgements?limit=0").await, [3]);
        let found: Vec<serde_json::Value> = get(&state, &addr, "/v1/judgements?card=1").await;
        assert_eq!(
            found[0],
            serde_json::json!({ "id": 1, "kind": "retry", "choice": "choice 1", "reason": "reason 1",
                "decider": "person", "outcome": null, "card": 1, "run": null, "at": found[0]["at"] })
        );
    }
}
