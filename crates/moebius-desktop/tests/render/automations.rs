//! The Automations page: the list, the editor of an automation with its schedule, chat and firings, a new automation,
//! and the clicks that save, fire it now and open a firing's chat.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{Automation, Firing};
use moebius_desktop::screens::automations;
use serde_json::json;

use crate::fixtures::{clicked_sized, draw_sized, overview};

/// The clock of the pictures.
const NOW: i64 = 1_790_000_000;

const TALL: Size = Size::new(1440.0, 1100.0);

fn automations_list() -> Vec<Automation> {
    serde_json::from_value(json!([
        {"id": 1, "name": "Morning triage", "schedule": "0 8 * * 1-5", "agent": "Moebius",
         "prompt": "Triage the issues opened since yesterday: label them and say which are ready to start.",
         "repository_id": 1, "harness_repository": false, "enabled": true, "next_firing": NOW + 3 * 3600,
         "last_firing": {"id": 3, "automation_id": 1, "status": "started", "run_id": 41, "chat_status": "succeeded",
                         "manual": false, "reason": "", "at": NOW - 21 * 3600}},
        {"id": 2, "name": "Outdated dependencies", "schedule": "0 9 * * 5", "agent": "Coder",
         "prompt": "Look for outdated dependencies and offer an upgrade.", "repository_id": 2,
         "harness_repository": false, "enabled": true, "next_firing": NOW + 2 * 86400,
         "last_firing": {"id": 2, "automation_id": 2, "status": "failed", "run_id": null, "chat_status": null,
                         "manual": false, "reason": "the agent Coder cannot run: its model is not served", "at": NOW - 5 * 86400}},
        {"id": 3, "name": "Nightly summary", "schedule": "30 23 * * *", "agent": "Moebius",
         "prompt": "Sum up what the agents did today.", "repository_id": null, "harness_repository": false,
         "enabled": false, "next_firing": null, "last_firing": null}
    ]))
    .unwrap()
}

fn firings() -> Vec<Firing> {
    serde_json::from_value(json!([
        {"id": 5, "automation_id": 1, "status": "started", "run_id": 52, "chat_status": "running", "manual": true,
         "reason": "", "at": NOW - 120},
        {"id": 4, "automation_id": 1, "status": "skipped", "run_id": null, "chat_status": null, "manual": false,
         "reason": "the chat of the firing before (41) still runs", "at": NOW - 3 * 3600},
        {"id": 3, "automation_id": 1, "status": "started", "run_id": 41, "chat_status": "succeeded",
         "manual": false, "reason": "", "at": NOW - 21 * 3600}
    ]))
    .unwrap()
}

fn agents() -> Vec<moebius_desktop::client::Agent> {
    serde_json::from_value(json!([
        {"name": "Moebius", "model": "claude", "built_in": true},
        {"name": "Coder", "model": "gpt"}
    ]))
    .unwrap()
}

/// The Automations page with the morning triage open.
fn automations_app() -> App {
    let mut app = App::with_snapshot(overview());
    let message = Message::Automations;
    let _ = app.update(Message::Select(Page::Automations));
    let _ = app.update(message(automations::Message::Tick(NOW)));
    let _ = app.update(message(automations::Message::Listed(
        Ok(automations_list()),
    )));
    let _ = app.update(message(automations::Message::Agents(Ok(agents()))));
    let _ = app.update(message(automations::Message::Select(1)));
    let _ = app.update(message(automations::Message::Firings(1, Ok(firings()))));
    app
}

#[test]
fn the_automations_page_lists_them_and_edits_one_with_its_firings() {
    draw_sized(
        &automations_app(),
        "automations",
        &[
            "Automations",
            "New automation",
            "Morning triage",
            "Weekdays at 8 · next in 3 h",
            "the agent Coder cannot run: its model is not served",
            "Off",
            "Run now",
            "Delete",
            "Save",
            "Schedule",
            "0 8 * * 1-5",
            "Fires next in 3 h.",
            "Moebius",
            "radiance",
            "Firings",
            "Chat 52 · running",
            "the chat of the firing before (41) still runs",
        ],
        TALL,
    );
}

#[test]
fn a_firing_opens_its_chat_and_run_now_and_save_are_sent() {
    let app = automations_app();
    assert!(matches!(
        &clicked_sized(&app, "Chat 52 · running", TALL)[..],
        [Message::OpenChat(52)]
    ));
    assert!(matches!(
        &clicked_sized(&app, "Run now", TALL)[..],
        [Message::Automations(automations::Message::RunNow)]
    ));
    // A skipped firing has no chat to open.
    assert!(clicked_sized(&app, "the chat of the firing before (41) still runs", TALL).is_empty());
    // Switched off, the next firing is known once saved.
    let mut app = app;
    let _ = app.update(Message::Automations(automations::Message::Switched(false)));
    assert!(crate::fixtures::shows(
        &app,
        "Save to see when it fires next."
    ));
}

#[test]
fn a_new_automation_starts_every_morning_and_cannot_run_before_it_is_saved() {
    let mut app = automations_app();
    let _ = app.update(Message::Automations(automations::Message::NewAutomation));
    let _ = app.update(Message::Automations(automations::Message::NameEdited(
        "Weekly cleanup".into(),
    )));
    draw_sized(
        &app,
        "automations-new",
        &[
            "Weekly cleanup",
            "0 8 * * *",
            "Fires once saved, when the schedule comes due.",
            "It has not fired yet.",
        ],
        TALL,
    );
    assert!(clicked_sized(&app, "Run now", TALL).is_empty());
    // A save without a prompt says what is missing, and sends nothing.
    let _ = app.update(Message::Automations(automations::Message::Save));
    assert!(crate::fixtures::shows(
        &app,
        "Write the prompt its chats start with."
    ));
    // A schedule typed by hand is kept as typed.
    let _ = app.update(Message::Automations(automations::Message::ScheduleEdited(
        "15 3 * * 0".into(),
    )));
    assert!(crate::fixtures::shows(&app, "15 3 * * 0"));
}
