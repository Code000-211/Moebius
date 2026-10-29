//! Editors in place: the pencil that edits a card's title and body, a GitHub board's trusted authors, and a draft's
//! brief where they are shown.

use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, ClientError};
use moebius_desktop::screens::experiments;
use moebius_desktop::widgets::edits;
use serde_json::json;

use crate::fixtures::{
    board, card, clicked, clicked_id, draw, overview, radiance_app, repository, shows, simulator,
    snapshot, update_all,
};

/// A card without a run, its history read.
fn card_page() -> App {
    let mut app = radiance_app();
    let _ = app.update(Message::OpenCard(50));
    let _ = app.update(Message::History(50, Ok(vec![])));
    app
}

/// A card's title edited in place: `tests/snapshots/card-edit-tiny-skia.png`.
#[test]
fn the_pencil_of_a_card_edits_its_title_and_body_in_place() {
    let mut app = card_page();
    assert!(shows(&app, "No description."));
    let messages = clicked_id(&app, "edit-card");
    assert!(
        matches!(
            &messages[..],
            [Message::Edit(edits::Message::EditCard(50, title, body))]
                if title == "Retry model downloads on HTTP 5xx" && body.is_empty()
        ),
        "{messages:?}"
    );
    update_all(&mut app, messages);
    let _ = app.update(Message::Edit(edits::Message::Title(
        "Retry model downloads on HTTP 5xx and 429".into(),
    )));
    draw(
        &app,
        "card-edit",
        &[
            "Edit the issue",
            "Retry model downloads on HTTP 5xx and 429",
            "Save",
            "Cancel",
        ],
    );
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Edit(edits::Message::Save)]
    ));
    assert_eq!(app.update(Message::Edit(edits::Message::Save)).units(), 1);
    let refused = ClientError::Server {
        status: 500,
        message: "GitHub answered 403".into(),
    };
    let _ = app.update(Message::Edit(edits::Message::CardSaved(Err(refused))));
    assert!(
        shows(&app, "GitHub answered 403"),
        "the refusal stays in the editor"
    );

    // Cancel closes the editor and the card is shown as it was.
    assert!(matches!(
        &clicked(&app, "Cancel")[..],
        [Message::Edit(edits::Message::Cancel)]
    ));
    let _ = app.update(Message::Edit(edits::Message::Cancel));
    assert!(!shows(&app, "Edit the issue"));
    assert!(shows(&app, "No description."));
}

#[test]
fn a_card_saved_in_place_closes_its_editor_and_the_board_is_read_again() {
    let mut app = card_page();
    let _ = app.update(Message::Edit(edits::Message::EditCard(
        50,
        "Retry".into(),
        "".into(),
    )));
    let _ = app.update(Message::Edit(edits::Message::Title("  ".into())));
    assert!(
        clicked(&app, "Save").is_empty(),
        "a blank title cannot be saved"
    );
    let card: client::Card = serde_json::from_value(card(50, 1, "Ready", "Retry")).unwrap();
    let task = app.update(Message::Edit(edits::Message::CardSaved(Ok(Box::new(card)))));
    assert_eq!(task.units(), 1, "the snapshot is read again");
    assert!(!shows(&app, "Edit the issue"));
}

#[test]
fn the_pencil_in_a_github_boards_header_edits_the_trusted_authors() {
    let mut app = radiance_app();
    assert!(shows(&app, " · no trusted authors"));
    let messages = clicked_id(&app, "edit-trusted");
    assert!(
        matches!(&messages[..], [Message::Edit(edits::Message::EditTrusted(1, trusted))] if trusted.is_empty()),
        "{messages:?}"
    );
    update_all(&mut app, messages);
    assert!(shows(&app, "Trusted authors"));
    let _ = app.update(Message::Edit(edits::Message::Trusted("leon, mona".into())));
    assert_eq!(app.update(Message::Edit(edits::Message::Save)).units(), 1);
    let _ = app.update(Message::Edit(edits::Message::Cancel));
    assert!(!shows(&app, "Trusted authors"));

    // A repository without a forge trusts nobody on GitHub and has no pencil.
    let app = App::with_snapshot(snapshot(
        vec![board(repository(2, "plain", None), vec![])],
        vec![],
    ));
    assert!(!shows(&app, " · no trusted authors"));

    // One on GitLab names its project and trusts usernames there.
    let mut tools = repository(3, "tools", None);
    tools["forge"] = "gitlab".into();
    tools["project"] = "acme/platform/tools".into();
    tools["trusted"] = serde_json::json!(["leon"]);
    let app = App::with_snapshot(snapshot(vec![board(tools, vec![])], vec![]));
    assert!(shows(&app, "GitLab · acme/platform/tools"));
    assert!(shows(&app, " · trusts leon"));
    assert_eq!(clicked_id(&app, "edit-trusted").len(), 1);

    // One on Forgejo names its repository there.
    let mut gadgets = repository(4, "gadgets", None);
    gadgets["forge"] = "forgejo".into();
    gadgets["project"] = "acme/gadgets".into();
    let app = App::with_snapshot(snapshot(vec![board(gadgets, vec![])], vec![]));
    assert!(shows(&app, "Forgejo · acme/gadgets"));
    assert!(shows(&app, " · no trusted authors"));
    assert_eq!(clicked_id(&app, "edit-trusted").len(), 1);
}

#[test]
fn the_pencil_of_a_drafts_brief_edits_it_in_place() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Experiments));
    let draft: client::Research = serde_json::from_value(json!({
        "id": 4, "repository_id": 1, "goal": "Faster builds", "measure": "make bench",
        "better": "lower", "token_budget": 1000, "status": "draft"
    }))
    .unwrap();
    let _ = app.update(Message::Experiments(experiments::Message::Listed(Ok(
        vec![draft.clone()],
    ))));
    let messages = clicked_id(&app, "edit-brief");
    assert!(
        matches!(
            &messages[..],
            [Message::Experiments(experiments::Message::EditBrief(4))]
        ),
        "{messages:?}"
    );
    update_all(&mut app, messages);
    let _ = app.update(Message::Experiments(experiments::Message::BriefGoal(
        "Faster release builds".into(),
    )));
    assert!(shows(&app, "Faster release builds"));
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Experiments(experiments::Message::SaveBrief)]
    ));
    let _ = app.update(Message::Experiments(experiments::Message::CancelBrief));
    assert!(!shows(&app, "Faster release builds"));

    // A confirmed brief has no pencil.
    let mut confirmed = draft;
    confirmed.status = "confirmed".into();
    let _ = app.update(Message::Experiments(experiments::Message::Listed(Ok(
        vec![confirmed],
    ))));
    let mut ui = simulator(&app);
    assert!(
        ui.click(iced::widget::Id::from("edit-brief".to_string()))
            .is_err()
    );
}
