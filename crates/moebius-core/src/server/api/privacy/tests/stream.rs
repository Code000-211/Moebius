//! The event stream and the notices of a private project: what names its project as `project`, an automation, a
//! research or a notice reaches its members only, and a mention there tells a non-member nothing.

use super::*;
use crate::store::{MENTIONED, NewNotice};

fn ids_of(private: &Private, kind: &str) -> i64 {
    private.ids.iter().find(|(k, _)| *k == kind).unwrap().1
}

/// Events that name a private project's things by a field other than `project_id`, `card` or `run` reach its members
/// only.
#[tokio::test]
async fn events_naming_a_private_project_s_things_otherwise_reach_its_members_only() {
    let (_dir, state, _addr) = start().await;
    let private = private(&state);
    let app = &state.app;
    let person = |handle: &str| {
        let id = app.store.person_by_handle(handle).unwrap().unwrap().id;
        app.sight(&Actor::person(id, handle)).unwrap()
    };
    let (lea, ben, olga) = (person("lea"), person("ben"), person("olga"));
    let ben_id = app.store.person_by_handle("ben").unwrap().unwrap().id;
    let notice = app
        .store
        .add_notice(
            ben_id,
            &NewNotice {
                kind: MENTIONED,
                by: Some("lea"),
                card_id: Some(private.card),
                inbox_item_id: None,
                title: "Secret card",
                body: "@ben look at the secret",
            },
        )
        .unwrap();
    let of = AppEvent::from;
    let events = [
        of(Event::ScriptChanged {
            project: private.project,
            column: "Ready".into(),
            path: "projects/Secret/ready.luau".into(),
        }),
        of(Event::ProjectKindsChanged {
            project: private.project,
            kinds: vec![],
        }),
        of(Event::IntakeChanged {
            project: private.project,
            request: 1,
            status: "open".into(),
        }),
        of(Event::AutomationFired {
            automation: ids_of(&private, "automations"),
            firing: 1,
            workflow: "Secret nightly".into(),
            status: "failed".into(),
            run: None,
            workflow_run: None,
        }),
        of(Event::ResearchStatus {
            research: ids_of(&private, "research"),
            status: "running".into(),
        }),
        of(Event::NoticeAdded {
            person: "ben".into(),
            notice,
        }),
    ];
    for event in &events {
        assert!(
            super::super::super::events::seen(app, &lea, event),
            "{event:?}"
        );
        assert!(
            !super::super::super::events::seen(app, &ben, event),
            "{event:?}"
        );
        assert!(
            !super::super::super::events::seen(app, &olga, event),
            "{event:?}"
        );
    }
}

/// A mention in a private card's comment tells a non-member nothing, and a notice they got of a private project's card
/// or inbox item is neither listed nor read for them.
#[tokio::test]
async fn a_non_member_is_told_nothing_of_a_private_project_and_cannot_read_such_a_notice() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let app = &state.app;
    let store = &app.store;
    let lea_id = store.person_by_handle("lea").unwrap().unwrap().id;
    let ben_id = store.person_by_handle("ben").unwrap().unwrap().id;
    app.comment_issue(
        &Actor::person(lea_id, "lea"),
        private.card,
        "@ben look at the secret",
    )
    .unwrap();
    assert!(store.notices(ben_id).unwrap().is_empty());

    // A notice from before the project was private, of its card and of an inbox item of no card.
    let item = store
        .inbox()
        .unwrap()
        .into_iter()
        .find(|item| item.kind == "workflow_approval")
        .unwrap();
    let mut notices = Vec::new();
    for (card, inbox_item) in [(Some(private.card), None), (None, Some(item.id))] {
        let notice = store
            .add_notice(
                ben_id,
                &NewNotice {
                    kind: MENTIONED,
                    by: Some("lea"),
                    card_id: card,
                    inbox_item_id: inbox_item,
                    title: "Secret card",
                    body: "@ben look at the secret",
                },
            )
            .unwrap();
        notices.push(notice.id);
    }
    let (status, listed) = call(&addr, &private.ben, "GET", "/v1/notices").await;
    assert_eq!(status, 200);
    assert_eq!(listed, json!([]));
    for id in notices {
        let (status, body) = call(
            &addr,
            &private.ben,
            "POST",
            &format!("/v1/notices/{id}/read"),
        )
        .await;
        assert_eq!(status, 404, "{body}");
        assert!(!body.to_string().contains("Secret card"), "{body}");
    }
}
