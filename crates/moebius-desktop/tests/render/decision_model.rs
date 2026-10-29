//! The Decision model page: the judgements counted by kind, the exports, the agreement per shadowed kind, and the
//! takeovers that are released here or merged from the inbox.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::ClientError;
use moebius_desktop::client::{Export, KindCounts, ShadowedKind, Shadowing, Takeover};
use moebius_desktop::screens::decision_model;
use moebius_desktop::screens::inbox;
use serde_json::json;

use crate::fixtures::{draw_sized, overview, pressed, shows, snapshot, tall_click};
use crate::inbox::{NOW, entry};

fn counts(kind: &str, count: i64, with_outcome: i64, by_person: i64, held_out: i64) -> KindCounts {
    KindCounts {
        kind: kind.to_string(),
        count,
        with_outcome,
        by_person,
        held_out,
    }
}

fn decision_model_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::DecisionModel));
    let _ = app.update(Message::DecisionModel(decision_model::Message::Kinds(Ok(
        vec![
            counts("approve_issue", 14, 12, 14, 1),
            counts("merge", 9, 9, 9, 2),
            counts("tool_call", 1204, 311, 0, 118),
        ],
    ))));
    let _ = app.update(Message::DecisionModel(decision_model::Message::Exports(
        Ok(vec![
            Export {
                id: 2,
                at: 1_793_182_500,
                kinds: vec![
                    counts("merge", 9, 9, 9, 2),
                    counts("tool_call", 1100, 300, 0, 104),
                ],
            },
            Export {
                id: 1,
                at: 1_793_000_000,
                kinds: vec![counts("merge", 4, 4, 4, 0)],
            },
        ]),
    )));
    app
}

#[test]
fn the_decision_model_screen_counts_the_kinds_lists_the_exports_and_keeps_room_for_shadowing() {
    draw_sized(
        &decision_model_app(),
        "decision-model",
        &[
            "Decision model",
            "Export training set",
            "1227 judgements recorded in 3 kinds; they leave the server only in an export you start.",
            "Kinds of decision",
            "With an outcome",
            "By a person",
            "Held out",
            "approve_issue",
            "1204",
            "GET /v1/exports/2/tool_call.jsonl",
            "GET /v1/exports/2/merge.jsonl",
            "Not exported yet",
            "moebius export --out <directory>",
            "Exports",
            "#2",
            "2026-10-28 10:15 UTC",
            "merge, tool_call",
            "Shadowing",
            "No decision model is configured.",
        ],
        Size::new(1440.0, 1100.0),
    );
    assert_eq!(
        pressed(&decision_model_app(), "Export training set").as_deref(),
        Some("[DecisionModel(Export)]")
    );
}

#[test]
fn an_empty_decision_model_screen_and_a_refused_export_say_so() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::DecisionModel));
    for text in [
        "Nothing has been decided yet.",
        "No export has been made yet.",
    ] {
        assert!(shows(&app, text), "{text}");
    }
    let _ = app.update(Message::DecisionModel(decision_model::Message::Exported(
        Err(ClientError::Server {
            status: 422,
            message: "no judgements of kind \"x\"".into(),
        }),
    )));
    assert!(shows(&app, "The export was refused"));
}

fn shadowed(kind: &str, shadows: i64, agreement: Option<f64>, routed: bool) -> ShadowedKind {
    ShadowedKind {
        kind: kind.to_string(),
        shadows,
        agreement,
        failed: if kind == "merge" { 3 } else { 0 },
        dropped: if kind == "merge" { 2 } else { 0 },
        routed,
    }
}

fn takeover(id: i64, kind: &str, agreement: f64, count: i64, status: &str) -> Takeover {
    Takeover {
        id,
        kind: kind.to_string(),
        agreement,
        count,
        status: status.to_string(),
    }
}

/// The Decision model screen with a model that shadows three kinds and has taken one over.
fn shadowing_app() -> App {
    let mut app = decision_model_app();
    let _ = app.update(Message::DecisionModel(decision_model::Message::Shadowing(
        Ok(Shadowing {
            configured: true,
            model: Some("moebius-decider".into()),
            agreement: Some(0.95),
            min_judgements: Some(100),
            kinds: vec![
                shadowed("approve_issue", 0, None, false),
                shadowed("merge", 40, Some(0.85), false),
                shadowed("tool_call", 118, Some(0.975), true),
            ],
            takeovers: vec![
                takeover(2, "merge", 0.9, 80, "dismissed"),
                takeover(3, "tool_call", 0.97, 100, "merged"),
            ],
        }),
    )));
    app
}

#[test]
fn the_decision_model_screen_shows_the_agreement_per_kind_and_releases_a_taken_over_kind() {
    draw_sized(
        &shadowing_app(),
        "decision-model-shadowing",
        &[
            "Shadowing",
            "moebius-decider decides alongside whoever decides now on the judgements held back from training. A kind is proposed in the inbox once it has 100 of them and agrees on at least 95%.",
            "Shadows",
            "Agreement",
            "Decided by",
            "No shadow yet",
            "40, 3 failed, 2 dropped",
            "85% (needs 95%)",
            "97.5% (needs 95%)",
            "The decision model",
            "As before",
            "The decision model decides tool_call: it agreed on 97% of 100 judgements.",
            "Release",
        ],
        Size::new(1440.0, 1250.0),
    );
    assert_eq!(
        pressed(&shadowing_app(), "Release").as_deref(),
        Some("[DecisionModel(Release(3))]")
    );
}

/// A takeover proposal waits in the inbox to be merged or dismissed.
#[test]
fn the_inbox_offers_a_takeover_of_the_decision_model_to_merge_or_dismiss() {
    let items = vec![entry(
        1,
        "takeover",
        None,
        "The decision model can take over tool_call",
        json!({"takeover": 4, "kind": "tool_call", "agreement": 0.972, "count": 140}),
        600,
    )];
    let mut app = App::with_snapshot(snapshot(vec![], items));
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Inbox(inbox::Message::Tick(NOW)));
    for text in [
        "Takeovers 1",
        "Decision model",
        "The decision model can take over tool_call",
        "It chose what was recorded on 97% of 140 judgements held back from training. Merging lets it decide this kind from now on.",
        "Dismiss",
    ] {
        assert!(shows(&app, text), "{text}");
    }
    assert_eq!(
        tall_click(&app, "Merge")
            .iter()
            .map(|m| format!("{m:?}"))
            .collect::<Vec<_>>(),
        ["Inbox(Act(1, MergeTakeover { takeover: 4 }))"]
    );
    assert_eq!(
        tall_click(&app, "Dismiss")
            .iter()
            .map(|m| format!("{m:?}"))
            .collect::<Vec<_>>(),
        ["Inbox(Act(1, DismissTakeover { takeover: 4 }))"]
    );
}
