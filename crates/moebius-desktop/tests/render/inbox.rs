//! The inbox: every kind of item, the proposals of the Retro, the issues and reports of a grilling, and what merging,
//! approving, rejecting and filtering send.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{ClientError, Snapshot};
use moebius_desktop::screens::chat;
use moebius_desktop::screens::inbox::{self, Action};
use serde_json::json;

use crate::chat::chatting;
use crate::fixtures::{
    board, card, click, clicked, draw_sized, repository, shows, simulator, snapshot, tall_click,
    update_all, working,
};

/// The time the inbox tests stand at, Unix seconds.
pub const NOW: i64 = 1_790_100_000;

fn change(
    id: i64,
    card: Option<i64>,
    branch: &str,
    state: &str,
    checks: &str,
) -> serde_json::Value {
    json!({"id": id, "repository_id": 1, "card_id": card, "branch": branch, "state": state,
           "pull_request": 18, "checks": checks, "review": "approved"})
}

pub fn entry(
    id: i64,
    kind: &str,
    card: Option<i64>,
    title: &str,
    data: serde_json::Value,
    ago: i64,
) -> serde_json::Value {
    json!({"id": id, "kind": kind, "card_id": card, "title": title, "data": data, "at": NOW - ago})
}

/// One item of each kind: a stuck card, a change to merge, a change from a chat, an outside issue and a pull request
/// that was closed.
fn inbox_snapshot() -> Snapshot {
    let mut radiance = board(
        repository(1, "radiance", Some("Code000-211/Radiance")),
        vec![
            working(
                45,
                1,
                "Ready to merge",
                "place: release staged units on teardown",
                json!({"rounds": 2, "tokens": 590_000, "change": {"id": 2, "checks": "success"}}),
            ),
            working(
                46,
                1,
                "Ready to merge",
                "Retry model downloads on HTTP 5xx",
                json!({"change": {"id": 3, "checks": "success"}}),
            ),
            working(
                55,
                1,
                "Backlog",
                "Support gfx1100 (RDNA3) kernels",
                json!({"author": "lowfreq"}),
            ),
        ],
    );
    radiance["changes"] = json!([
        change(2, Some(45), "moebius/card-45", "open", "success"),
        change(3, Some(46), "moebius/card-46", "closed", "success"),
        change(5, None, "moebius/chat-9", "open", "pending"),
    ]);
    snapshot(
        vec![
            radiance,
            board(
                repository(2, "moebius", None),
                vec![card(
                    12,
                    2,
                    "In progress",
                    "Board: drag cards between columns",
                )],
            ),
        ],
        vec![
            entry(
                1,
                "stuck",
                Some(12),
                "Board: drag cards between columns",
                json!({"reason": "the round limit is reached"}),
                1200,
            ),
            entry(
                2,
                "ready_to_merge",
                Some(45),
                "place: release staged units on teardown",
                json!({"change": 2, "branch": "moebius/card-45"}),
                3600,
            ),
            entry(
                3,
                "ready_to_merge",
                None,
                "Add a retry helper to the downloader",
                json!({"change": 5, "branch": "moebius/chat-9"}),
                7200,
            ),
            entry(
                4,
                "approve_issue",
                Some(55),
                "Support gfx1100 (RDNA3) kernels",
                json!({"reason": "lowfreq is not a trusted author"}),
                10_800,
            ),
            entry(
                5,
                "change_closed",
                Some(46),
                "Retry model downloads on HTTP 5xx",
                json!({"change": 3, "pull_request": 18}),
                172_800,
            ),
        ],
    )
}

pub fn inbox_app() -> App {
    let mut app = App::with_snapshot(inbox_snapshot());
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Inbox(inbox::Message::Tick(NOW)));
    app
}

#[test]
fn the_inbox_shows_every_kind_of_item_like_the_mockup() {
    draw_sized(
        &inbox_app(),
        "inbox",
        &[
            "Inbox",
            "5 things need you.",
            "All 5",
            "To merge 2",
            "Approvals 1",
            "Stuck 1",
            "Closed 1",
            "Every repository",
            "radiance 4",
            "moebius 1",
            "Stuck",
            "moebius · #12",
            "Board: drag cards between columns",
            "Stopped: the round limit is reached.",
            "20 min ago",
            "Ready to merge",
            "radiance · #45",
            "Rounds",
            "590k tok",
            "CI green · review approved",
            "moebius/card-45",
            "Merge",
            "Open live view",
            "Reject with a reason",
            "Change from a chat",
            "radiance · chat",
            "Open chat",
            "Outside issue",
            "radiance · #55 · by lowfreq",
            "Approve",
            "Closed without a merge",
            "Pull request #18 was closed without a merge.",
            "Retry",
            "2 d ago",
        ],
        Size::new(1440.0, 1500.0),
    );
}

/// A positive and an untried proposal of the Retro, waiting for a person.
fn proposals_app() -> App {
    let proposals = vec![
        entry(
            1,
            "proposal",
            None,
            "Tell the Coder to run the tests first",
            json!({"proposal": 1, "branch": "proposal/2026-10-28-small-commits", "trials": [
                {"card": 48, "outcome": "positive",
                 "reason": "the run succeeded, checks_failed did not happen again, the checks pass, and it used 41200 tokens against 38900"},
                {"card": 50, "outcome": "positive",
                 "reason": "the run succeeded, run_failed did not happen again, there were no checks, and it used 12000 tokens against 15100"}]}),
            3600,
        ),
        entry(
            2,
            "proposal",
            None,
            "Allow reading the docs directory",
            json!({"proposal": 2, "branch": "proposal/2026-10-28-allow-docs", "trials": [
                {"card": null, "outcome": "skipped",
                 "reason": "the proposal names no cards, so there is no task to try it on"}]}),
            7200,
        ),
    ];
    let mut app = App::with_snapshot(snapshot(vec![], proposals));
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Inbox(inbox::Message::Tick(NOW)));
    app
}

/// The inbox with proposals drawn: compare `tests/snapshots/inbox-proposals-tiny-skia.png`.
#[test]
fn the_inbox_offers_a_proposal_of_the_retro_to_merge_or_dismiss() {
    draw_sized(
        &proposals_app(),
        "inbox-proposals",
        &[
            "2 things need you.",
            "Proposals 2",
            "Retro proposal",
            "Retro",
            "Tell the Coder to run the tests first",
            "The Retro proposes this change to the harness configuration.",
            "Card 48",
            "Positive",
            "the run succeeded, checks_failed did not happen again, the checks pass, and it used 41200 tokens against 38900",
            "Allow reading the docs directory",
            "No card",
            "Skipped",
            "Merge",
            "Dismiss",
        ],
        Size::new(1440.0, 900.0),
    );
}

/// A change to `moebius.toml` and one to a repository's rules that the Retro proposes, waiting for a person.
fn own_proposals_app() -> App {
    let items = vec![
        entry(
            1,
            "proposal",
            None,
            "Give cards fewer rounds",
            json!({"proposal": 3, "branch": "proposal/2026-10-28-rounds", "kind": "settings", "repository": null,
                   "reason": "Cards 48 and 50 went round five times on the same failing test; three rounds would have stopped them sooner.",
                   "diff": "--- a/moebius.toml\n+++ b/moebius.toml\n@@ -4,4 +4,4 @@\n [board]\n tick_ms = 2000\n-round_limit = 5\n+round_limit = 3\n"}),
            3600,
        ),
        entry(
            2,
            "proposal",
            None,
            "Forbid force-pushing in radiance",
            json!({"proposal": 4, "branch": "proposal/2026-10-28-no-force-push", "kind": "repository", "repository": 1,
                   "reason": "The Coder force-pushed twice and lost a reviewer's commit.",
                   "diff": "--- /dev/null\n+++ b/.moebius/rules.md\n@@ -0,0 +1,3 @@\n+## no-force-push\n+\n+Never force-push.\n"}),
            7200,
        ),
    ];
    let boards = vec![board(repository(1, "radiance", None), vec![])];
    let mut app = App::with_snapshot(snapshot(boards, items));
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Inbox(inbox::Message::Tick(NOW)));
    app
}

/// The inbox with proposals for Moebius's own configuration drawn: compare
/// `tests/snapshots/inbox-own-proposals-tiny-skia.png`.
#[test]
fn the_inbox_shows_a_proposal_for_moebius_itself_as_a_diff_with_the_retros_reason() {
    let app = own_proposals_app();
    draw_sized(
        &app,
        "inbox-own-proposals",
        &[
            "2 things need you.",
            "Give cards fewer rounds",
            "The Retro proposes this change to moebius.toml. Merging saves it, and restarts the server when a key needs it.",
            "Cards 48 and 50 went round five times on the same failing test; three rounds would have stopped them sooner.",
            "Forbid force-pushing in radiance",
            "The Retro proposes this change to the repository's own rules or column scripts. Merging offers it as a change on the repository.",
            "The Coder force-pushed twice and lost a reviewer's commit.",
            "radiance",
        ],
        Size::new(1440.0, 1100.0),
    );
    let mut ui = simulator(&app);
    ui.click("Merge").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(matches!(
        &messages[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::MergeProposal { proposal: 3 }
        ))]
    ));
}

/// The report of the grilling in `grilling_app`.
const REPORT: &str = "# Is a local cache worth it?\n\nThe downloads are rare and large, so a cache would save\nlittle. Measure the repeat rate before building one.";

/// The issues a grilling proposes, and a research report, waiting for a person.
fn grilling_app() -> App {
    let items = vec![
        entry(
            1,
            "grilling_issues",
            None,
            "3 issues from a grilling",
            json!({"grilling": 7, "repository": 1, "issues": [
                {"title": "Add the export button", "body": "A button on the report page that downloads the CSV.",
                 "blocked_by": []},
                {"title": "Write the CSV", "body": "The columns are the report's.", "blocked_by": [0]},
                {"title": "Document the export", "body": "A section in the user guide.", "blocked_by": [0, 1]}]}),
            1800,
        ),
        entry(
            2,
            "grilling_report",
            None,
            "A research report from a grilling",
            json!({"grilling": 8, "repository": 1,
                   "report": REPORT}),
            5400,
        ),
    ];
    let boards = vec![board(repository(1, "radiance", None), vec![])];
    let mut app = App::with_snapshot(snapshot(boards, items));
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Inbox(inbox::Message::Tick(NOW)));
    app
}

/// The inbox with the issues of a grilling: compare `tests/snapshots/inbox-grilling-issues-tiny-skia.png`.
#[test]
fn the_inbox_offers_the_issues_of_a_grilling_as_a_list_to_edit() {
    let mut app = grilling_app();
    let _ = app.update(Message::Inbox(inbox::Message::Kind(Some(
        "grilling_issues".into(),
    ))));
    draw_sized(
        &app,
        "inbox-grilling-issues",
        &[
            "2 things need you; 1 shown.",
            "Grilled issues 1",
            "Issues from a grilling",
            "radiance · Grilling",
            "3 issues from a grilling",
            "Issue 1",
            "Issue 3",
            "Blocked by issue 1",
            "Blocked by issue 1, 2",
            "Add the export button",
            "Document the export",
            "Remove",
            "Confirm",
            "Dismiss",
        ],
        Size::new(1440.0, 1100.0),
    );
}

/// The inbox with a report of a grilling: compare `tests/snapshots/inbox-grilling-report-tiny-skia.png`.
#[test]
fn the_inbox_shows_the_report_of_a_grilling_to_read_and_dismiss() {
    let mut app = grilling_app();
    let _ = app.update(Message::Inbox(inbox::Message::Kind(Some(
        "grilling_report".into(),
    ))));
    draw_sized(
        &app,
        "inbox-grilling-report",
        &[
            "Reports 1",
            "Report from a grilling",
            "radiance · Grilling",
            "A research report from a grilling",
            REPORT,
            "Dismiss",
        ],
        Size::new(1440.0, 600.0),
    );
    assert!(!shows(&app, "Confirm"));
    assert!(matches!(
        &clicked(&app, "Dismiss")[..],
        [Message::Inbox(inbox::Message::Act(
            2,
            Action::DismissGrilling { grilling: 8 }
        ))]
    ));
}

#[test]
fn confirming_or_dismissing_a_grilling_names_it_and_removing_an_issue_takes_it_off_the_list() {
    let mut app = grilling_app();
    let _ = app.update(Message::Inbox(inbox::Message::Kind(Some(
        "grilling_issues".into(),
    ))));
    assert!(matches!(
        &tall_click(&app, "Dismiss")[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::DismissGrilling { grilling: 7 }
        ))]
    ));
    // The first Remove is that of the first issue.
    let messages = tall_click(&app, "Remove");
    update_all(&mut app, messages);
    assert!(!shows(&app, "Add the export button"));
    assert!(shows(&app, "Write the CSV"));
    assert!(shows(&app, "Issue 2"));
    assert!(!shows(&app, "Issue 3"));
    assert!(matches!(
        &tall_click(&app, "Confirm")[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::ConfirmGrilling { grilling: 7 }
        ))]
    ));
    let messages = tall_click(&app, "Confirm");
    update_all(&mut app, messages);
    assert!(shows(&app, "Making the cards…"));

    // With every issue removed there is nothing to confirm.
    let mut empty = grilling_app();
    let _ = empty.update(Message::Inbox(inbox::Message::Kind(Some(
        "grilling_issues".into(),
    ))));
    for _ in 0..3 {
        let _ = empty.update(Message::Inbox(inbox::Message::Remove(1, 0)));
    }
    assert!(shows(
        &empty,
        "No issue is left. Dismiss the item, or ask the agent again."
    ));
    let mut ui = simulator(&empty);
    let _ = ui.click("Confirm");
    assert_eq!(ui.into_messages().count(), 0);
}

#[test]
fn a_grilling_chat_needs_a_repository_before_it_starts() {
    let mut app = chatting("waiting", false);
    click(&mut app, "New chat");
    let agents = serde_json::from_value(json!([
        {"name": "Moebius", "model": "claude-fake", "protocol": "anthropic", "built_in": true},
        {"name": "Grilling-partner", "model": "claude-fake", "protocol": "anthropic", "built_in": true}
    ]))
    .unwrap();
    let _ = app.update(Message::Chat(chat::Message::Agents(Ok(agents))));
    let _ = app.update(Message::Chat(chat::Message::AgentChosen(
        "Grilling-partner".into(),
    )));
    assert!(shows(
        &app,
        "The Grilling partner questions your idea in a repository. Ending the chat puts the issues it proposes in the Inbox, to confirm."
    ));
    assert!(shows(
        &app,
        "A grilling needs a repository: choose one below."
    ));
    let _ = app.update(Message::Chat(chat::Message::Edited(
        "Export the reports".into(),
    )));
    // Without a repository the chat does not start, with one it does.
    assert_eq!(app.update(Message::Chat(chat::Message::Send)).units(), 0);
    assert_eq!(
        app.update(Message::Chat(chat::Message::StartInTerminal))
            .units(),
        0
    );
    let mut ui = simulator(&app);
    let _ = ui.click("Send");
    assert_eq!(ui.into_messages().count(), 0);
    let _ = app.update(Message::Chat(chat::Message::RepositoryChosen(Some(1))));
    assert!(shows(&app, "Grilling-partner in radiance"));
    assert!(matches!(
        &clicked(&app, "Send")[..],
        [Message::Chat(chat::Message::Send)]
    ));
}

#[test]
fn merging_or_dismissing_a_proposal_names_it_and_shows_the_request_until_it_is_answered() {
    let mut app = proposals_app();
    // Both items have the buttons; the first of each is the newest item's.
    let mut ui = simulator(&app);
    ui.click("Dismiss").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(matches!(
        &messages[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::DismissProposal { proposal: 1 }
        ))]
    ));
    let mut ui = simulator(&app);
    ui.click("Merge").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(matches!(
        &messages[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::MergeProposal { proposal: 1 }
        ))]
    ));
    update_all(&mut app, messages);
    assert!(shows(&app, "Merging…"));
    let refused = inbox::Message::Done(
        1,
        Err(ClientError::Server {
            status: 409,
            message: "the branch does not merge cleanly".into(),
        }),
    );
    let _ = app.update(Message::Inbox(refused));
    assert!(shows(&app, "the branch does not merge cleanly"));
}

#[test]
fn merging_sends_the_message_and_the_item_shows_the_request_until_it_is_answered() {
    let mut app = inbox_app();
    let messages = clicked(&app, "Merge");
    assert!(matches!(
        &messages[..],
        [Message::Inbox(inbox::Message::Act(
            2,
            Action::Merge { change: 2 }
        ))]
    ));
    update_all(&mut app, messages);
    assert!(shows(&app, "Merging…"));

    let refused = inbox::Message::Done(
        2,
        Err(ClientError::Server {
            status: 409,
            message: "the branch does not merge cleanly".into(),
        }),
    );
    let _ = app.update(Message::Inbox(refused));
    assert!(shows(&app, "the branch does not merge cleanly"));
    assert!(!shows(&app, "Merging…"));
}

#[test]
fn approving_and_retrying_name_the_card() {
    let app = inbox_app();
    assert!(matches!(
        &clicked(&app, "Approve")[..],
        [Message::Inbox(inbox::Message::Act(
            4,
            Action::Approve { card: 55 }
        ))]
    ));
    let mut stuck = inbox_app();
    let _ = stuck.update(Message::Inbox(inbox::Message::Kind(Some("stuck".into()))));
    assert!(matches!(
        &clicked(&stuck, "Retry")[..],
        [Message::Inbox(inbox::Message::Act(
            1,
            Action::Retry { card: 12 }
        ))]
    ));
}

#[test]
fn rejecting_asks_for_a_reason_first() {
    let mut app = inbox_app();
    click(&mut app, "Reject with a reason");
    assert!(!shows(&app, "Reject with a reason"));
    assert!(shows(&app, "Send back"));
    // Without a reason the button does nothing.
    let mut ui = simulator(&app);
    let _ = ui.click("Send back");
    assert_eq!(ui.into_messages().count(), 0);

    let _ = app.update(Message::Inbox(inbox::Message::Reason(
        2,
        "Handle the empty list".into(),
    )));
    assert!(matches!(
        &clicked(&app, "Send back")[..],
        [Message::Inbox(inbox::Message::Act(
            2,
            Action::Reject { change: 2 }
        ))]
    ));
    click(&mut app, "Cancel");
    assert!(shows(&app, "Reject with a reason"));
}

#[test]
fn opening_the_live_view_from_the_inbox_goes_to_the_card() {
    let mut app = inbox_app();
    assert!(matches!(
        &clicked(&app, "Open live view")[..],
        [Message::OpenCard(45)]
    ));
    let _ = app.update(Message::OpenCard(45));
    assert_eq!(app.page(), Page::Board(1));
}

#[test]
fn filtering_by_kind_or_repository_hides_the_other_items() {
    let mut app = inbox_app();
    click(&mut app, "To merge 2");
    assert!(shows(&app, "Merge"));
    assert!(!shows(&app, "Retry"));
    assert!(!shows(&app, "Approve"));
    assert!(shows(&app, "5 things need you; 2 shown."));
    // The sidebar keeps the total.
    assert!(shows(&app, "5"));

    let _ = app.update(Message::Inbox(inbox::Message::Kind(None)));
    click(&mut app, "moebius 1");
    assert!(shows(&app, "Board: drag cards between columns"));
    assert!(!shows(&app, "Merge"));

    let _ = app.update(Message::Inbox(inbox::Message::Kind(Some(
        "approve_issue".into(),
    ))));
    assert!(shows(&app, "No item matches the filter."));
}

#[test]
fn an_empty_inbox_says_so() {
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Select(Page::Inbox));
    assert!(shows(&app, "Nothing needs you."));
}
