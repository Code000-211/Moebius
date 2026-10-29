//! The inbox against a real server: a change merged, an outside issue approved, and the issues of a grilling confirmed
//! into cards.

use std::time::Duration;

use iced_test::Simulator;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{Client, ClientError, NewChat};
use serde_json::json;

use crate::fixtures::{add_demo, post, snapshot_where, start_with_fake_pi};

#[tokio::test(flavor = "multi_thread")]
async fn the_inbox_merges_a_change_and_approves_an_outside_issue() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start_with_fake_pi(dir.path()).await;
    let repository = add_demo(&settings, dir.path()).await;
    let repository = repository["id"].as_i64().unwrap();
    post(
        &settings,
        &format!("/v1/repositories/{repository}/issues"),
        json!({"title": "Add a feature"}),
    )
    .await;
    let outside = server
        .app
        .store
        .add_card(repository, "An outside issue", "", "stranger", "Backlog")
        .unwrap();

    // The first card is coded by the fake Pi and waits to be merged; the outside issue waits to be approved.
    let client = Client::new(settings);
    let snapshot = snapshot_where(&client, |snapshot| {
        ["ready_to_merge", "approve_issue"]
            .iter()
            .all(|kind| snapshot.inbox.iter().any(|item| item.kind == *kind))
    })
    .await;
    let item = |kind: &str| {
        snapshot
            .inbox
            .iter()
            .find(|item| item.kind == kind)
            .unwrap()
            .clone()
    };

    let to_merge = item("ready_to_merge");
    let card = to_merge.card_id.unwrap();
    client.merge(to_merge.change().unwrap()).await.unwrap();
    let column_of = |snapshot: &moebius_desktop::client::Snapshot, id: i64| {
        snapshot.boards[0]
            .cards
            .iter()
            .find(|card| card.id == id)
            .map(|card| card.column.clone())
    };
    let merged = snapshot_where(&client, |s| column_of(s, card).as_deref() == Some("Done")).await;
    assert!(merged.inbox.iter().all(|item| item.id != to_merge.id));

    let to_approve = item("approve_issue");
    assert_eq!(to_approve.card_id, Some(outside.id));
    client.approve(outside.id).await.unwrap();
    let approved = snapshot_where(&client, |s| {
        column_of(s, outside.id).as_deref() != Some("Backlog")
    })
    .await;
    assert!(approved.inbox.iter().all(|item| item.id != to_approve.id));

    // A request the server refuses comes back with its message: the change is merged already.
    match client.merge(to_merge.change().unwrap()).await.unwrap_err() {
        ClientError::Server { status, .. } => assert_eq!(status, 422),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_grilling_ends_in_an_inbox_item_whose_issues_the_client_confirms_into_cards() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let repository = add_demo(&settings, dir.path()).await;
    let client = Client::new(settings.clone());

    // The chat is started in the repository, answered by the fake Pi, which leaves the issues, and ended.
    let message = "Export the reports.\n[put .moebius/grilling/issues.json]\n\
        [{\"title\": \"Add the button\", \"body\": \"On the report page.\", \"blocked_by\": []}, \
         {\"title\": \"Write the CSV\", \"body\": \"Its columns.\", \"blocked_by\": [0]}]\n[/put]";
    let run = client
        .start_chat(&NewChat {
            agent: Some("Grilling-partner".into()),
            message: Some(message.into()),
            repository: repository["id"].as_i64(),
            ..NewChat::default()
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(60), async {
        while client.chats().await.unwrap()[0].status != "waiting" {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the agent did not answer");
    client.end_chat(run.id).await.unwrap();

    let snapshot = snapshot_where(&client, |s| {
        s.inbox.iter().any(|item| item.kind == "grilling_issues")
    })
    .await;
    let item = snapshot
        .inbox
        .iter()
        .find(|i| i.kind == "grilling_issues")
        .unwrap();
    assert_eq!(item.title, "2 issues from a grilling");
    assert_eq!(item.repository(), repository["id"].as_i64());
    let issues = item.issues();
    assert_eq!(issues[1].blocked_by, [0]);

    // The inbox page draws the list to edit, with the buttons that act on it.
    let mut app = App::with_snapshot(snapshot.clone());
    let _ = app.update(Message::Select(Page::Inbox));
    let mut ui = Simulator::with_size(
        iced::Settings {
            fonts: moebius_desktop::theme::fonts(),
            ..iced::Settings::default()
        },
        iced::Size::new(1440.0, 900.0),
        app.view(),
    );
    assert!(ui.find("Add the button").is_ok());

    // Confirming the list the person left makes the cards, and the item leaves the inbox.
    client
        .confirm_grilling(item.grilling().unwrap(), &issues)
        .await
        .unwrap();
    let confirmed = snapshot_where(&client, |s| {
        s.inbox.iter().all(|item| item.kind != "grilling_issues") && s.boards[0].cards.len() == 2
    })
    .await;
    let cards = &confirmed.boards[0].cards;
    assert_eq!(cards[0].title, "Add the button");
    assert_eq!(cards[1].blocked_by, [cards[0].number]);
    // Confirming again is refused with the server's message.
    match client
        .confirm_grilling(item.grilling().unwrap(), &issues)
        .await
        .unwrap_err()
    {
        ClientError::Server { status, .. } => assert_eq!(status, 422),
        other => panic!("{other:?}"),
    }
}
