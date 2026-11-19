//! What a private project's existence or number would show through even when none of its things do: the projects a kind
//! lists, the count in the refusal to delete a kind, the errors a Retro list counts, a card's blockers, and an id
//! that answers differently when the thing is hidden than when it is not there.

use super::*;
use crate::store::{Card, FieldDef};

/// A card of the project Open in the private project's repository, which everyone sees.
fn open_card(state: &ApiState, private: &Private) -> Card {
    let store = &state.app.store;
    let open = store.add_project("Open", "OPEN").unwrap();
    store.link_repository(open.id, private.repository).unwrap();
    let card = store
        .add_card(private.repository, "In the open", "", "lea", "Backlog")
        .unwrap();
    store.set_card_project(card.id, open.id).unwrap();
    store.card(card.id).unwrap().unwrap()
}

async fn send(addr: &str, token: &str, method: &str, path: &str) -> (u16, Value) {
    call(addr, token, method, path).await
}

#[tokio::test]
async fn a_kind_lists_only_the_projects_whose_records_the_asker_sees() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let store = &state.app.store;
    let open = open_card(&state, &private).project_id;
    let kind = store.add_kind("Bug", &[] as &[FieldDef]).unwrap();
    store
        .set_project_kinds(private.project, &[kind.id])
        .unwrap();
    store.set_project_kinds(open, &[kind.id]).unwrap();
    let projects = |answer: &Value| -> Vec<i64> {
        answer[0]["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_i64().unwrap())
            .collect()
    };
    // A member sees both; a person who is no member sees only the open one; an owner sees that the project exists.
    let (_, answer) = send(&addr, &private.lea, "GET", "/v1/kinds").await;
    assert_eq!(projects(&answer), [private.project, open]);
    let (_, answer) = send(&addr, &private.ben, "GET", "/v1/kinds").await;
    assert_eq!(projects(&answer), [open], "{answer}");
    let (_, answer) = send(&addr, &private.olga, "GET", "/v1/kinds").await;
    assert_eq!(projects(&answer), [private.project, open]);
}

#[tokio::test]
async fn deleting_a_kind_does_not_count_the_cards_of_projects_the_asker_does_not_see() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let store = &state.app.store;
    let kind = store.add_kind("Bug", &[] as &[FieldDef]).unwrap();
    store.set_card_kind(private.card, Some(kind.id)).unwrap();
    let path = format!("/v1/kinds/{}", kind.id);
    // Only hidden cards have the kind (deleting is for owners): the kind stays, and the answer has no number.
    let (status, answer) = send(&addr, &private.olga, "DELETE", &path).await;
    assert_eq!(status, 409, "{answer}");
    assert!(
        !answer.to_string().chars().any(|c| c.is_ascii_digit()),
        "{answer}"
    );
    // With one card of an open project too, the answer counts that card only.
    let open = open_card(&state, &private);
    store.set_card_kind(open.id, Some(kind.id)).unwrap();
    let (status, answer) = send(&addr, &private.olga, "DELETE", &path).await;
    assert_eq!(status, 409);
    assert!(answer.to_string().contains("1 cards"), "{answer}");
    // Someone who sees both projects is told of both.
    let refused = state
        .app
        .delete_kind(kind.id, &crate::app::Sight::everything())
        .unwrap_err();
    assert!(refused.to_string().contains("2 cards"), "{refused}");
}

#[tokio::test]
async fn the_retro_list_counts_only_the_errors_the_asker_sees() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let errors = |answer: &Value| answer[0]["errors"].as_i64().unwrap();
    let (_, answer) = send(&addr, &private.lea, "GET", "/v1/retros").await;
    assert_eq!(errors(&answer), 2, "{answer}");
    let (_, answer) = send(&addr, &private.ben, "GET", "/v1/retros").await;
    assert_eq!(errors(&answer), 1, "{answer}");
    // The list agrees with the Retro itself.
    let (_, retro) = send(&addr, &private.ben, "GET", "/v1/retros/2026-03-10").await;
    assert_eq!(retro["report"]["errors"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_card_does_not_list_the_numbers_of_hidden_cards_that_block_it() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let store = &state.app.store;
    let secret = store.card(private.card).unwrap().unwrap();
    let open = open_card(&state, &private);
    let visible = store
        .add_card(private.repository, "Another open one", "", "lea", "Backlog")
        .unwrap();
    store.set_card_project(visible.id, open.project_id).unwrap();
    store
        .set_dependencies(open.id, &[secret.number, visible.number].into())
        .unwrap();
    let blocked_by = |answer: &Value| -> Vec<i64> {
        let cards = answer["boards"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|board| board["cards"].as_array().unwrap().iter());
        cards
            .filter(|card| card["id"] == open.id)
            .flat_map(|card| card["blocked_by"].as_array().unwrap().iter())
            .map(|n| n.as_i64().unwrap())
            .collect()
    };
    let (_, answer) = send(&addr, &private.ben, "GET", "/v1/snapshot").await;
    assert_eq!(blocked_by(&answer), [visible.number], "{answer}");
    let (_, answer) = send(&addr, &private.lea, "GET", "/v1/snapshot").await;
    assert_eq!(blocked_by(&answer), [secret.number, visible.number]);
}
