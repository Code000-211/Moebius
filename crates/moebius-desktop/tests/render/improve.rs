//! The Retro and Experiments pages: the days of the Retro with their errors and proposals, and the research with its
//! baseline, levers and brief.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, ClientError};
use moebius_desktop::screens::experiments;
use moebius_desktop::screens::retro;
use serde_json::json;

use crate::fixtures::{draw_sized, overview, pressed, shows};

fn retro_listing() -> Vec<client::RetroSummary> {
    serde_json::from_value(json!([
        {"date": "2026-10-28", "status": "proposed", "run_id": 31, "errors": 6, "proposals": 3, "at": 1790000000},
        {"date": "2026-10-27", "status": "reported", "run_id": null, "errors": 0, "proposals": 0, "at": 1789900000},
        {"date": "2026-10-26", "status": "failed", "run_id": 20, "errors": 3, "proposals": 0, "at": 1789800000},
    ]))
    .unwrap()
}

fn proposal_json() -> serde_json::Value {
    json!([
        {"id": 1, "branch": "proposal/2026-10-28-small-commits", "title": "Tell the Coder to run the tests first",
         "body": "The Coder's changes failed the checks twice on 2026-10-28; its instructions now say to run the tests before it stops.",
         "agents": ["Coder"], "cards": [48, 50], "status": "positive", "trials": [
            {"card_id": 48, "outcome": "positive",
             "reason": "the run succeeded, checks_failed did not happen again, the checks pass, and it used 41200 tokens against 38900"},
            {"card_id": 50, "outcome": "positive",
             "reason": "the run succeeded, run_failed did not happen again, there were no checks, and it used 12000 tokens against 15100"}]},
        {"id": 2, "branch": "proposal/2026-10-28-allow-docs", "title": "Allow reading the docs directory",
         "body": "The rule no-reads-outside refused three reads of docs/.", "agents": [], "cards": [], "status": "untried",
         "trials": [{"card_id": null, "outcome": "skipped",
                     "reason": "the proposal names no cards, so there is no task to try it on"}]},
        {"id": 3, "branch": "proposal/2026-10-28-write-plainly", "title": "Ask the Coder to write plainly",
         "body": "The Reviewer found unclear names in card 48.", "agents": ["Coder"], "cards": [48], "status": "negative",
         "trials": [{"card_id": 48, "outcome": "negative",
                     "reason": "the run did not succeed; it still has run_failed"}]},
    ])
}

pub fn proposal_listing() -> Vec<client::Proposal> {
    serde_json::from_value(proposal_json()).unwrap()
}

fn retro_view() -> client::RetroView {
    serde_json::from_value(json!({
        "date": "2026-10-28", "status": "proposed",
        "warnings": ["proposal/2026-10-28-code is left out: it changes src/main.rs, which is not harness configuration"],
        "report": {"date": "2026-10-28", "errors": [
            {"kind": "tool_failed", "run": 12, "card": 48, "agent": "Coder", "text": "bash: cargo test exited with 101"},
            {"kind": "tool_refused", "run": 12, "card": 48, "agent": "Coder", "text": "bash: no-destructive-commands: rm -rf target"},
            {"kind": "run_failed", "run": 14, "card": 50, "agent": "Coder", "text": "the agent changed nothing"},
            {"kind": "steered", "run": 12, "card": 48, "agent": "Coder", "text": "use the other file"},
            {"kind": "checks_failed", "run": null, "card": 48, "agent": null, "text": "The checks failed:\ncargo test"},
            {"kind": "review_findings", "run": null, "card": 50, "agent": null, "text": "The test for the empty case is missing."},
        ]},
        "proposals": proposal_json(),
    }))
    .unwrap()
}

fn retro_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Retro));
    let retro = |message| Message::Retro(message);
    let _ = app.update(retro(retro::Message::Listed(Ok(retro_listing()))));
    let _ = app.update(retro(retro::Message::Select("2026-10-28".into())));
    let _ = app.update(retro(retro::Message::Opened(
        "2026-10-28".into(),
        Ok(retro_view()),
    )));
    app
}

#[test]
fn the_retro_screen_lists_the_days_and_shows_the_errors_and_proposals_of_one() {
    draw_sized(
        &retro_app(),
        "retro",
        &[
            "Retro",
            "Run the Retro now",
            "2026-10-28",
            "Proposed",
            "6 errors · 3 proposals",
            "2026-10-27",
            "Reported",
            "0 errors · 0 proposals",
            "Failed",
            "Errors of 2026-10-28",
            "Failed tool calls · 1",
            "bash: cargo test exited with 101",
            "Refused tool calls · 1",
            "Failed runs · 1",
            "Steering · 1",
            "Failed checks · 1",
            "Review findings · 1",
            "card 48 · run 12 · Coder",
            "Left out",
            "Proposals",
            "Tell the Coder to run the tests first",
            "proposal/2026-10-28-small-commits",
            "Positive",
            "Cards 48, 50",
            "the run succeeded, run_failed did not happen again, there were no checks, and it used 12000 tokens against 15100",
            "Allow reading the docs directory",
            "Untried",
            "Skipped",
            "the proposal names no cards, so there is no task to try it on",
            "Ask the Coder to write plainly",
            "Negative",
            "the run did not succeed; it still has run_failed",
        ],
        Size::new(1440.0, 1500.0),
    );
}

#[test]
fn a_retro_that_could_not_be_started_says_why_and_a_day_without_errors_says_so() {
    let mut app = retro_app();
    let _ = app.update(Message::Retro(retro::Message::Started(Err(
        ClientError::Server {
            status: 409,
            message: "the Retro of 2026-10-28 exists already".into(),
        },
    ))));
    assert!(shows(&app, "The Retro did not start"));
    let empty: client::RetroView = serde_json::from_value(json!({
        "date": "2026-10-27", "status": "reported", "report": {"errors": []}, "proposals": []
    }))
    .unwrap();
    let _ = app.update(Message::Retro(retro::Message::Select("2026-10-27".into())));
    let _ = app.update(Message::Retro(retro::Message::Opened(
        "2026-10-27".into(),
        Ok(empty),
    )));
    assert!(shows(&app, "No errors that day."));
    assert!(shows(&app, "The Retro proposed nothing."));
}

fn research_listing() -> Vec<client::Research> {
    serde_json::from_value(json!([
        {"id": 2, "repository_id": 1, "goal": "Make the host build faster", "measure": "cargo build --release 2>&1 | tail -1",
         "better": "lower", "token_budget": 200000, "status": "draft", "baseline": null, "reason": "", "tokens": 0,
         "experiments": []},
        {"id": 1, "repository_id": 1, "goal": "Cut the clean build time of the host", "measure": "./bench/build.sh",
         "better": "lower", "token_budget": 500000, "status": "done", "baseline": 372.0,
         "reason": "a round proposed no more levers", "tokens": 211400, "experiments": [
            {"id": 1, "research_id": 1, "branch": "research/1-split-crates", "title": "Split the host crate in two",
             "value": 301.0, "checks": true, "kept": true, "reason": "372 to 301 (-19.1 %), better than the baseline",
             "change_id": 4},
            {"id": 2, "research_id": 1, "branch": "research/1-more-jobs", "title": "Build with more jobs",
             "value": 375.0, "checks": null, "kept": false,
             "reason": "375 against the baseline 372: not better by more than 1 %", "change_id": null},
            {"id": 3, "research_id": 1, "branch": "research/1-lld", "title": "Link with lld",
             "value": null, "checks": null, "kept": false,
             "reason": "the measurement failed: the measure ended with exit status 127: lld: not found", "change_id": null}]},
    ]))
    .unwrap()
}

fn experiments_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Experiments));
    let _ = app.update(Message::Experiments(experiments::Message::Listed(Ok(
        research_listing(),
    ))));
    let _ = app.update(Message::Experiments(experiments::Message::Select(1)));
    app
}

#[test]
fn the_experiments_screen_lists_the_research_and_shows_the_baseline_and_levers_of_one() {
    draw_sized(
        &experiments_app(),
        "experiments",
        &[
            "Experiments",
            "New brief",
            "Draft",
            "Done",
            "3 experiments · 1 kept",
            "Cut the clean build time of the host",
            "./bench/build.sh",
            "radiance · Lower is better · 211400 of 500000 tokens used",
            "Baseline",
            "372",
            "Lever",
            "Branch",
            "Value",
            "Change",
            "Split the host crate in two",
            "research/1-split-crates",
            "301",
            "-19.1 %",
            "Kept",
            "372 to 301 (-19.1 %), better than the baseline",
            "Not kept",
            "Failed",
            "—",
        ],
        Size::new(1440.0, 1100.0),
    );
}

#[test]
fn a_draft_is_confirmed_or_dropped_a_confirmed_one_started_and_a_running_one_stopped() {
    let mut app = experiments_app();
    let _ = app.update(Message::Experiments(experiments::Message::Select(2)));
    assert_eq!(
        pressed(&app, "Confirm").as_deref(),
        Some("[Experiments(Act(2, \"confirm\"))]")
    );
    assert_eq!(
        pressed(&app, "Drop").as_deref(),
        Some("[Experiments(Act(2, \"stop\"))]")
    );
    assert_eq!(pressed(&app, "Start now"), None);
    assert_eq!(pressed(&app, "Stop"), None);
    let mut listing = research_listing();
    // A confirmed one is started or dropped, a running one stopped.
    for (status, available, missing) in [
        ("confirmed", ["Start now", "Drop"], "Confirm"),
        ("running", ["Stop", "Stop"], "Start now"),
    ] {
        listing[0].status = status.to_string();
        let _ = app.update(Message::Experiments(experiments::Message::Listed(Ok(
            listing.clone(),
        ))));
        for label in available {
            assert!(pressed(&app, label).is_some(), "{status}: {label}");
        }
        assert_eq!(pressed(&app, missing), None, "{status} has no {missing}");
    }
    listing[0].status = "stopped".to_string();
    let _ = app.update(Message::Experiments(experiments::Message::Listed(Ok(
        listing,
    ))));
    for label in ["Confirm", "Start now", "Stop", "Drop"] {
        assert_eq!(pressed(&app, label), None, "a stopped research has {label}");
    }
    assert!(shows(&app, "No lever has been tried yet."));
}

#[test]
fn a_brief_needs_a_repository_a_goal_a_measure_and_a_budget_before_it_is_stored() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Experiments));
    assert!(shows(&app, "No research yet"));
    let _ = app.update(Message::Experiments(experiments::Message::Compose));
    for text in [
        "radiance",
        "Lower is better",
        "Higher is better",
        "Token budget",
    ] {
        assert!(shows(&app, text), "{text}");
    }
    let stored = |app: &App| pressed(app, "Store the brief") == Some("[Experiments(Store)]".into());
    assert!(!stored(&app));
    let edit = |app: &mut App, message| {
        let _ = app.update(Message::Experiments(message));
    };
    edit(&mut app, experiments::Message::GoalEdited("Faster".into()));
    assert!(!stored(&app));
    edit(
        &mut app,
        experiments::Message::MeasureEdited("cat score".into()),
    );
    assert!(stored(&app));
    edit(&mut app, experiments::Message::BudgetEdited("0".into()));
    assert!(!stored(&app));
    edit(&mut app, experiments::Message::BudgetEdited("1000".into()));
    assert!(stored(&app));
}
