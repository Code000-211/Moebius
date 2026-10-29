//! The Retro (spec 4, ADR 0001): once a day the day's errors are collected, without a model, into a report, and the
//! Retro analyst agent turns the report into proposals, which are changes to the harness configuration, each on a
//! branch of its own, or to Moebius's own configuration (see `own`). The report is stored with the day's `retros` row;
//! the proposals are stored with the branches the analyst left in the harness configuration clone, after they were
//! checked and pushed to `origin`; trying them is the work of `trial`. The schedule asks `due` once a
//! minute; `POST /v1/retros` starts a Retro by hand. Nothing here waits for a person.
//!
//! This module holds the report, the analyst's run and what becomes of it; `traces` has the raw traces of the
//! report's runs that the analyst reads besides the report.

mod traces;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{App, AppError, Result};
use crate::board::Runs;
use crate::bus::AppEvent;
use crate::config::TimeOfDay;
use crate::harness::repo::HarnessRepo;
use crate::improve::{lessons, own};
use crate::server::settings::Live;
use crate::store::{Found, Proposal, Retro, Store};

/// How many characters of a text a report keeps.
const TEXT_LIMIT: usize = 500;

/// One thing that went wrong, as the report has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportError {
    /// `tool_failed`, `tool_refused`, `run_failed`, `run_stopped`, `steered`, `checks_failed`, `conflict` or
    /// `review_findings`.
    pub kind: String,
    pub run: Option<i64>,
    pub card: Option<i64>,
    pub agent: Option<String>,
    pub text: String,
}

/// A card that reached Done after one code run and was never sent back from review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergedCard {
    pub card: i64,
    pub title: String,
    /// The start of the issue.
    pub text: String,
}

/// The errors of one local day, and the cards that merged at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub date: String,
    pub errors: Vec<ReportError>,
    /// The cards that merged at once; the Retro writes lessons from them too.
    #[serde(default)]
    pub merged: Vec<MergedCard>,
}

/// A Retro as the list shows it: without its report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetroSummary {
    pub date: String,
    pub status: String,
    pub run_id: Option<i64>,
    /// How many errors the report holds.
    pub errors: usize,
    /// How many proposals the analyst made.
    pub proposals: usize,
    pub at: i64,
}

/// A Retro with its report and its proposals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetroView {
    pub date: String,
    pub status: String,
    pub run_id: Option<i64>,
    pub warnings: Vec<String>,
    pub at: i64,
    pub report: Report,
    pub proposals: Vec<Proposal>,
}

fn shorten(text: &str) -> String {
    match text.char_indices().nth(TEXT_LIMIT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// Collects the errors of the local day `day` (`YYYY-MM-DD`), from its first moment to the first of the next day.
pub fn report(store: &Store, day: &str) -> Result<Report> {
    let (from, to) = store
        .day_window(day)?
        .ok_or_else(|| AppError::Invalid(format!("{day:?} is not a date (YYYY-MM-DD)")))?;
    let mut found: Vec<Found> = Vec::new();
    found.extend(store.failed_tool_calls(from, to, None)?);
    found.extend(store.refused_tool_calls_between(from, to, None)?);
    found.extend(store.failed_runs(from, to, None)?);
    found.extend(store.stopped_runs(from, to, None)?);
    found.extend(store.steering(from, to, None)?);
    found.extend(store.sent_back_cards(from, to)?);
    found.sort_by_key(|f| f.at);
    Ok(Report {
        date: day.to_string(),
        errors: found
            .into_iter()
            .map(|f| ReportError {
                kind: f.kind.into(),
                run: f.run_id,
                card: f.card_id,
                agent: f.agent,
                text: shorten(&f.text),
            })
            .collect(),
        merged: store
            .merged_at_once(from, to)?
            .into_iter()
            .map(|(card, title, text)| MergedCard {
                card,
                title: shorten(&title),
                text: shorten(&text),
            })
            .collect(),
    })
}

/// Whether the Retro of a day runs now: the day's time for it has come and the day has no Retro yet. A day whose time
/// passed without the server running is not caught up, because the next day's Retro is the one that is looked for.
pub fn due(now: TimeOfDay, retro_at: TimeOfDay, exists: bool) -> bool {
    now.0 >= retro_at.0 && !exists
}

fn view_of(store: &Store, retro: Retro) -> Result<RetroView> {
    Ok(RetroView {
        proposals: store.proposals(Some(&retro.date))?,
        report: serde_json::from_value(retro.report).map_err(crate::store::StoreError::from)?,
        date: retro.date,
        status: retro.status,
        run_id: retro.run_id,
        warnings: retro.warnings,
        at: retro.at,
    })
}

/// The Retro of `date` with its report and proposals.
pub fn view(store: &Store, date: &str) -> Result<RetroView> {
    let retro = store
        .retro(date)?
        .ok_or_else(|| AppError::NotFound(format!("retro {date}")))?;
    view_of(store, retro)
}

/// All Retros, the latest day first, without their reports.
pub fn summaries(store: &Store) -> Result<Vec<RetroSummary>> {
    let proposals = store.proposals(None)?;
    Ok(store
        .retros()?
        .into_iter()
        .map(|retro| RetroSummary {
            errors: retro.report["errors"].as_array().map_or(0, Vec::len),
            proposals: proposals
                .iter()
                .filter(|p| p.retro_date == retro.date)
                .count(),
            date: retro.date,
            status: retro.status,
            run_id: retro.run_id,
            at: retro.at,
        })
        .collect())
}

/// What the analyst is told after the report; `own` is what it is told about proposals for Moebius's own
/// configuration (see `own::brief`).
fn prompt(report: &Report, own: &str, traces: &str) -> String {
    let lessons = lessons::BRIEF;
    let json = serde_json::to_string_pretty(report).unwrap_or_default();
    format!(
        "This is the Retro of {day}. Below are the errors of that day, collected by Moebius. Find what they have in \
         common and propose changes to the harness configuration (your worktree) that would avoid them. Make each \
         proposal its own branch named proposal/{day}-<short-name> from the current commit, with one commit whose \
         message is the proposal and ends with a line like `Cards: 48, 50` naming the `card` of each error of the \
         report that it addresses, and change only harness configuration files, but see below. Commit with `git -c \
         user.name=Retro -c user.email=retro@localhost commit`. Propose nothing the errors do not call for.\n\n\
         {own}\n\
         {lessons}\n\
         {traces}\n\
         The report:\n\n{json}\n",
        day = report.date
    )
}

/// Runs the Retro of `day`: the report is stored and published, and, when the day had errors and a harness
/// configuration repository is configured, the Retro analyst starts. The analysis goes on in the background; its end
/// is published as `retro_done`. A day that has a Retro already is an error.
pub fn start(
    app: &Arc<App>,
    (runs, live): (&Arc<dyn Runs>, &Arc<Live>),
    repo: Option<&Arc<HarnessRepo>>,
    day: &str,
) -> Result<RetroView> {
    let report = report(&app.store, day)?;
    let analyse = repo.filter(|_| !report.errors.is_empty() || !report.merged.is_empty());
    let status = if analyse.is_some() {
        "analysing"
    } else {
        "reported"
    };
    let value = serde_json::to_value(&report).map_err(crate::store::StoreError::from)?;
    if !app.store.add_retro(day, &value, status)? {
        return Err(AppError::Exists(format!("the Retro of {day}")));
    }
    let Some(repo) = analyse else {
        publish_done(app, day)?;
        return view(&app.store, day);
    };
    if let Err(why) = launch(app, (runs, live), repo, &report) {
        app.store.finish_retro(day, "failed", &[why])?;
        publish_done(app, day)?;
    }
    view(&app.store, day)
}

/// Starts the analyst's run and the thread that waits for it.
fn launch(
    app: &Arc<App>,
    (runs, live): (&Arc<dyn Runs>, &Arc<Live>),
    repo: &Arc<HarnessRepo>,
    report: &Report,
) -> std::result::Result<(), String> {
    // Best effort: the analyst should see the configuration as it is now, but a refresh that fails changes nothing.
    let _ = repo.refresh(app);
    let base = repo.default_branch().map_err(|e| e.to_string())?;
    let mut ended = app.bus.subscribe();
    let own = own::brief(app, live, report).map_err(|e| e.to_string())?;
    let files = traces::files(app, report);
    let prompt = prompt(report, &own, &traces::brief(report, &files));
    let run = runs.start_retro(app, repo.path(), &base, &prompt, &files)?;
    app.store
        .set_retro_run(&report.date, run.id)
        .map_err(|e| e.to_string())?;
    let (app, live, repo, day) = (app.clone(), live.clone(), repo.clone(), report.date.clone());
    std::thread::Builder::new()
        .name(format!("retro {day}"))
        .spawn(move || {
            wait_for_end(&app, &mut ended, run.id);
            let (status, warnings) = conclude(&app, &live, &repo, &day, run.id);
            let _ = app.store.finish_retro(&day, status, &warnings);
            let _ = publish_done(&app, &day);
        })
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Waits until the run has ended. `ended` is a subscription to the bus made before the run started: the run has ended
/// once the store says so, and the events only wake this up.
pub fn wait_for_end(
    app: &App,
    ended: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    run_id: i64,
) {
    while app
        .store
        .run(run_id)
        .ok()
        .flatten()
        .is_some_and(|r| r.ended_at.is_none())
    {
        match ended.blocking_recv() {
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            _ => continue,
        }
    }
}

/// What became of the analyst's run: the status of the Retro and its warnings. The proposals are stored: first those
/// for Moebius's own configuration, whose branches are then gone, then those of the harness configuration.
fn conclude(
    app: &App,
    live: &Live,
    repo: &HarnessRepo,
    day: &str,
    run_id: i64,
) -> (&'static str, Vec<String>) {
    let run = app.store.run(run_id).ok().flatten();
    if run.as_ref().is_none_or(|run| run.status != "succeeded") {
        let status = run.map_or("gone".to_string(), |run| run.status);
        let why = app
            .store
            .events(run_id)
            .unwrap_or_default()
            .iter()
            .rev()
            .find(|e| e.kind == "moebius_error")
            .and_then(|e| e.data["message"].as_str().map(shorten))
            .unwrap_or_default();
        return ("failed", vec![format!("the analyst's run {status}: {why}")]);
    }
    let prefix = format!("proposal/{day}-");
    let own_warnings = match repo.take_own_branches(&prefix) {
        Ok((branches, mut warnings)) => match own::store(app, live, day, &branches) {
            Ok(more) => {
                warnings.extend(more);
                warnings
            }
            Err(e) => return ("failed", vec![format!("cannot store the proposals: {e}")]),
        },
        Err(e) => return ("failed", vec![format!("cannot read the proposals: {e}")]),
    };
    let lesson_warnings =
        lessons::store_written(app, day, &run.and_then(|r| r.summary).unwrap_or_default());
    match repo.proposed(app, &prefix) {
        Ok((proposed, mut warnings)) => {
            warnings.splice(0..0, own_warnings);
            warnings.extend(lesson_warnings);
            for p in &proposed {
                if let Err(e) = app
                    .store
                    .add_proposal(day, &p.branch, &p.title, &p.body, &p.agents, &p.cards)
                {
                    return ("failed", vec![format!("cannot store {}: {e}", p.branch)]);
                }
            }
            ("proposed", warnings)
        }
        Err(e) => ("failed", vec![format!("cannot read the proposals: {e}")]),
    }
}

fn publish_done(app: &App, day: &str) -> Result<()> {
    let retro = view(&app.store, day)?;
    app.bus.publish(AppEvent::new(
        "retro_done",
        json!({ "date": day, "status": retro.status, "proposals": retro.proposals.len() }),
    ));
    Ok(())
}

/// The schedule's check, once a minute: starts today's Retro when `retro_at` has passed and today has none.
pub fn on_schedule(
    app: &Arc<App>,
    runs: (&Arc<dyn Runs>, &Arc<Live>),
    repo: Option<&Arc<HarnessRepo>>,
    retro_at: TimeOfDay,
) -> Result<()> {
    let day = app.store.today()?;
    let now = TimeOfDay(app.store.minutes_into_the_day()?);
    if due(now, retro_at, app.store.retro(&day)?.is_some()) {
        start(app, runs, repo, &day)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u16, minute: u16) -> TimeOfDay {
        TimeOfDay(hour * 60 + minute)
    }

    #[test]
    fn the_retro_is_due_from_retro_at_on_until_the_day_has_one() {
        let retro_at = at(23, 30);
        assert!(!due(at(0, 0), retro_at, false));
        assert!(!due(at(23, 29), retro_at, false));
        assert!(due(at(23, 30), retro_at, false));
        assert!(due(at(23, 59), retro_at, false));
        assert!(!due(at(23, 30), retro_at, true));
        assert!(!due(at(23, 59), retro_at, true));
        // A day starts at midnight, so the missed Retro of yesterday is not caught up.
        assert!(!due(at(0, 1), retro_at, false));
    }

    #[test]
    fn a_text_is_shortened_by_characters() {
        assert_eq!(shorten("short"), "short");
        let long = "é".repeat(TEXT_LIMIT + 1);
        assert_eq!(shorten(&long).chars().count(), TEXT_LIMIT + 1);
        assert_eq!(shorten(&"é".repeat(TEXT_LIMIT)), "é".repeat(TEXT_LIMIT));
    }
}
