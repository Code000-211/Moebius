//! A project's board: its columns and cards and their tags, a card that moved, the form of a new issue above the
//! columns, and the editor of a column's script that the pencil opens.

use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, ClientError};
use moebius_desktop::widgets::new_issue;
use moebius_desktop::widgets::script_editor;
use serde_json::json;

use crate::fixtures::{
    board, card, clicked, draw, item, pressed, radiance, radiance_app, repository, run, shows,
    simulator, snapshot, working,
};

/// The board of the mockup: every column, the cards and their tags.
#[test]
fn the_board_shows_its_columns_and_cards_like_the_mockup() {
    let snapshot = snapshot(vec![radiance("Ready")], vec![item(1, "ready_to_merge", 45)]);
    let app = App::with_snapshot(snapshot);
    draw(
        &app,
        "board",
        &[
            "radiance",
            "3 runs working · 1 change waits for you · default workflow",
            "Ready",
            "ready.luau",
            "In progress",
            "code.luau",
            "In review",
            "review.luau",
            "Ready to merge",
            "merge.luau",
            "Done",
            "done.luau",
            "#54",
            "docs: explain the coupled MTP mode",
            "Blocked by #48",
            "#50",
            "Next",
            "#48 · Coder · round 2 of 5",
            "Live",
            "412k tok",
            "1 rule broken",
            "#52 · Coder · round 1 of 5",
            "96k tok",
            "#47 · Reviewer · round 3 of 5",
            "CI running",
            "#45 · 2 rounds · 590k tok",
            "Green",
            "Waits for you",
            "#43",
        ],
    );
}

/// The states the mockup does not show, and titles that must stay inside their cards.
#[test]
fn every_tag_and_a_long_title_stay_inside_the_cards() {
    let long = "Supercalifragilisticexpialidocious_and_a_path_without_any_break/that/goes/on/and/on/and/on.rs";
    let snapshot = snapshot(
        vec![board(
            repository(1, "radiance", None),
            vec![
                working(1, 1, "Backlog", "An issue from outside", json!({})),
                working(2, 1, "Backlog", long, json!({"blocked_by": [7, 8]})),
                card(6, 1, "Backlog", "Another issue from outside"),
                card(7, 1, "Ready", "Ready to start"),
                working(
                    3,
                    1,
                    "In progress",
                    "Two broken rules",
                    json!({
                    "rounds": 5, "tokens": 1_250_000, "refusals": 2, "run": run(1, "Coder", "code", "failed"),
                    "change": {"id": 1, "checks": "failure"}}),
                ),
                working(
                    4,
                    1,
                    "In progress",
                    "Stuck at the round limit",
                    json!({
                    "rounds": 5, "run": run(2, "Coder", "code", "failed")}),
                ),
                working(
                    5,
                    1,
                    "In review",
                    "Waiting for a decision",
                    json!({"rounds": 2}),
                ),
            ],
        )],
        vec![item(1, "approve_issue", 1), item(2, "stuck", 4)],
    );
    let app = App::with_snapshot(snapshot);
    draw(
        &app,
        "board-states",
        &[
            "Backlog",
            "backlog.luau",
            "Waits for you",
            "Blocked by #7, #8",
            "2 rules broken",
            "1.2M tok",
            "CI failed",
            "Stuck",
            "#3 · Coder · round 5 of 5",
        ],
    );
    let mut ui = simulator(&app);
    let title = ui.find(long).unwrap();
    // The count of the Backlog, the only column of three cards, is at its right edge, inside the column's padding; the
    // title ends at or before it.
    let count = ui.find("3").unwrap();
    assert!(title.bounds().x + title.bounds().width <= count.bounds().x + count.bounds().width);
}

/// Whether the card's line is in the column of the script: it starts a little to the right of the script's name.
fn is_in_column(app: &App, card: &str, script: &str) -> bool {
    let mut ui = simulator(app);
    let card = ui.find(card).unwrap().bounds().x;
    let script = ui.find(script).unwrap().bounds().x;
    (0.0..20.0).contains(&(card - script))
}

/// A card moved by the server is in its new column once the snapshot is read again.
#[test]
fn a_card_that_moved_is_drawn_in_its_new_column() {
    let mut app = radiance_app();
    assert!(is_in_column(&app, "#50", "ready.luau"));
    assert!(!is_in_column(&app, "#50", "review.luau"));

    let moved = snapshot(vec![radiance("In review")], vec![]);
    let _ = app.update(Message::Loaded(1, Ok(moved)));
    assert!(is_in_column(&app, "#50", "review.luau"));
    assert!(!is_in_column(&app, "#50", "ready.luau"));
    // Cards keep their order by number: #47 stays above #50.
    let mut ui = simulator(&app);
    assert!(
        ui.find("#47 · Reviewer · round 3 of 5").unwrap().bounds().y
            < ui.find("#50").unwrap().bounds().y
    );
}

pub fn paste(text: &str) -> new_issue::Message {
    new_issue::Message::BodyEdited(iced::widget::text_editor::Action::Edit(
        iced::widget::text_editor::Edit::Paste(std::sync::Arc::new(text.to_string())),
    ))
}

/// The board with the form open: compare `tests/snapshots/board-new-issue-tiny-skia.png`.
#[test]
fn a_boards_new_issue_button_opens_a_form_above_the_columns_that_creates_or_is_cancelled() {
    let mut app = radiance_app();
    assert_eq!(
        pressed(&app, "New issue").as_deref(),
        Some("[NewIssue(Open(1))]")
    );
    let _ = app.update(Message::NewIssue(new_issue::Message::Open(1)));
    assert!(shows(&app, "Title"));
    assert_eq!(pressed(&app, "New issue").as_deref(), Some("[]"), "spent");
    assert_eq!(pressed(&app, "Create").as_deref(), Some("[]"), "no title");
    let _ = app.update(Message::NewIssue(new_issue::Message::TitleEdited(
        "  ".into(),
    )));
    assert_eq!(pressed(&app, "Create").as_deref(), Some("[]"), "blank");
    let _ = app.update(Message::NewIssue(new_issue::Message::TitleEdited(
        "Add a cache".into(),
    )));
    let _ = app.update(Message::NewIssue(paste(
        "Downloads are slow.\n\nKeep the files for a day.",
    )));
    assert_eq!(
        pressed(&app, "Create").as_deref(),
        Some("[NewIssue(Create)]")
    );
    draw(
        &app,
        "board-new-issue",
        &["New issue", "Describe the issue", "Create", "Cancel"],
    );
    // The server's refusal stays in the form, and the form is gone after Cancel.
    let _ = app.update(Message::NewIssue(new_issue::Message::Answered(Err(
        ClientError::Server {
            status: 422,
            message: "an issue needs a title".into(),
        },
    ))));
    assert!(shows(
        &app,
        "the server answered 422: an issue needs a title"
    ));
    assert_eq!(
        pressed(&app, "Cancel").as_deref(),
        Some("[NewIssue(Cancel)]")
    );
    let _ = app.update(Message::NewIssue(new_issue::Message::Cancel));
    assert!(!shows(&app, "Cancel"));
    assert!(shows(&app, "New issue"));
}

/// The detail of a column's script as the server answers it.
pub fn script(source: &str, text: &str) -> client::ScriptDetail {
    serde_json::from_value(json!({
        "column": "Ready", "path": ".moebius/ready.luau", "source": source, "text": text
    }))
    .unwrap()
}

pub const HOLD: &str = "-- Hold large cards until nothing else runs.\nfunction on_card(card)\n  if moebius.runs_active() > 0 then\n    moebius.wait(\"another run is going\")\n    return\n  end\n  moebius.move(\"In progress\", \"ready to code\")\nend\n";

/// The board of the mockup with the editor of the Ready column's script open.
pub fn script_app(source: &str) -> App {
    let mut app = radiance_app();
    let _ = app.update(Message::EditScript(1, "Ready".into()));
    let _ = app.update(Message::Script(script_editor::Message::Opened(Ok(script(
        source, HOLD,
    )))));
    app
}

#[test]
fn the_pencil_of_a_column_opens_the_editor_of_its_script() {
    let app = radiance_app();
    for (column, _) in [("Ready", 0), ("In progress", 0), ("Done", 0)] {
        let mut ui = simulator(&app);
        ui.click(iced::widget::Id::from(format!("edit-script-{column}")))
            .unwrap_or_else(|_| panic!("no pencil in the {column} column"));
        let messages: Vec<_> = ui.into_messages().collect();
        assert!(
            matches!(&messages[..], [Message::EditScript(1, name)] if name == column),
            "{column}: {messages:?}"
        );
    }
}

#[test]
fn the_editor_shows_the_script_of_the_repository_and_can_save_or_give_it_back() {
    let app = script_app("repository");
    draw(
        &app,
        "script-editor",
        &[
            "Ready",
            "Repository",
            ".moebius/ready.luau",
            "Use the built-in script",
            "Save",
        ],
    );
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Script(script_editor::Message::Save)]
    ));
    assert!(matches!(
        &clicked(&app, "Use the built-in script")[..],
        [Message::Script(script_editor::Message::AskBuiltIn)]
    ));
    // Giving the script back asks first.
    let mut asking = script_app("repository");
    let _ = asking.update(Message::Script(script_editor::Message::AskBuiltIn));
    assert!(shows(&asking, "Remove the repository's script?"));
    assert!(matches!(
        &clicked(&asking, "Keep")[..],
        [Message::Script(script_editor::Message::Keep)]
    ));
    let _ = asking.update(Message::Script(script_editor::Message::Keep));
    assert!(!shows(&asking, "Remove the repository's script?"));
}

#[test]
fn the_built_in_script_has_no_button_to_go_back_to_it_and_a_refusal_is_shown() {
    let mut app = script_app("built-in");
    assert!(shows(&app, "Built in"));
    assert!(!shows(&app, "Use the built-in script"));
    let refusal = ClientError::Server {
        status: 422,
        message: "ready.luau: the script does not define a function on_card(card)".into(),
    };
    let _ = app.update(Message::Script(script_editor::Message::Saved(Err(refusal))));
    draw(
        &app,
        "script-editor-refused",
        &["ready.luau: the script does not define a function on_card(card)"],
    );
}

#[test]
fn the_speech_bubble_opens_the_floating_chat_with_a_message_about_the_script() {
    let mut app = script_app("repository");
    let mut ui = simulator(&app);
    ui.click(iced::widget::Id::from(script_editor::ASK_AGENT))
        .unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        matches!(
            &messages[..],
            [Message::Script(script_editor::Message::AskAgent)]
        ),
        "{messages:?}"
    );
    let _ = app.update(messages.into_iter().next().unwrap());
    assert_eq!(app.page(), Page::Board(1));
    assert!(shows(&app, "Works in radiance"));
    assert!(shows(&app, "Your first message starts the chat."));
    assert!(shows(
        &app,
        "About the Ready column's script (.moebius/ready.luau): "
    ));
}
