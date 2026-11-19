//! The relations of an issue to others in a repository that a private project shares with a public one: a card of the
//! private project is neither named, linked nor compared for someone who does not see it.

use super::*;
use crate::store::Card;

/// A card of the project Open in the private project's repository, which everyone sees, and the private card.
fn shared(state: &ApiState, private: &Private) -> (Card, Card) {
    let store = &state.app.store;
    let open = store.add_project("Open", "OPEN").unwrap();
    store.link_repository(open.id, private.repository).unwrap();
    let secret = store.card(private.card).unwrap().unwrap();
    let public = store
        .add_card(
            private.repository,
            &format!("{} as well", secret.title),
            &secret.body,
            "lea",
            "Backlog",
        )
        .unwrap();
    store.set_card_project(public.id, open.id).unwrap();
    (store.card(public.id).unwrap().unwrap(), secret)
}

async fn send(addr: &str, token: &str, method: &str, path: &str, body: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .request(method.parse().unwrap(), format!("http://{addr}{path}"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

#[tokio::test]
async fn a_relation_cannot_name_a_card_of_a_private_project_its_asker_does_not_see() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let (public, secret) = shared(&state, &private);
    let card = format!("/v1/cards/{}", public.id);
    let n = secret.number;
    for (method, path, body) in [
        ("POST", format!("{card}/related"), json!({ "related": n })),
        ("PUT", format!("{card}/duplicate"), json!({ "original": n })),
        ("PUT", format!("{card}/parent"), json!({ "parent": n })),
        ("POST", format!("{card}/blockers"), json!({ "blocker": n })),
    ] {
        let (status, answer) = send(&addr, &private.ben, method, &path, body).await;
        assert_eq!(status, 404, "{method} {path}: {answer}");
        assert!(!answer.to_string().contains(&secret.title), "{answer}");
    }
    let public_now = state.app.store.card(public.id).unwrap().unwrap();
    assert!(!public_now.closed, "not closed as a duplicate");
    assert_eq!(public_now.parent, None);
    assert!(
        state
            .app
            .store
            .links_to(secret.id, "related")
            .unwrap()
            .is_empty()
    );

    // A member relates them; the other still sees the public card, without the private one among its links.
    let path = format!("{card}/related");
    let (status, links) = send(&addr, &private.lea, "POST", &path, json!({ "related": n })).await;
    assert_eq!(status, 200, "{links}");
    assert!(
        links.to_string().contains(&secret.title),
        "the member sees it"
    );
    let (status, issue) = call(&addr, &private.ben, "GET", &card).await;
    assert_eq!(status, 200, "{issue}");
    assert_eq!(issue["related"], json!([]), "{issue}");
    let unrelate = format!("{card}/related/{n}");
    let (status, _) = call(&addr, &private.ben, "DELETE", &unrelate).await;
    assert_eq!(status, 404);
    assert_eq!(
        state
            .app
            .store
            .links_to(secret.id, "related")
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn an_estimate_is_not_proposed_from_a_card_of_another_private_project() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let (public, secret) = shared(&state, &private);
    let store = &state.app.store;
    store
        .set_card_text(
            secret.id,
            "Cache the build outputs",
            "Keep the build outputs between runs",
        )
        .unwrap();
    store
        .set_card_text(
            public.id,
            "Cache the build outputs",
            "Keep the build outputs between runs",
        )
        .unwrap();
    state.app.set_estimate(secret.id, Some(3)).unwrap();
    let path = format!("/v1/cards/{}/estimate/proposal", public.id);
    let (status, proposal) = call(&addr, &private.ben, "GET", &path).await;
    assert_eq!(status, 200, "{proposal}");
    assert_eq!(proposal["compared"], json!([]), "{proposal}");
    assert_eq!(proposal["estimate"], Value::Null);
}
