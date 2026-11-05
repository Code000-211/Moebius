//! The issue tracker over the cards: what a person, the Moebius agent or a session of an agent does to an issue besides
//! editing its text. Issues are addressed by their card's id; the relations (parent, blockers) name issue numbers of the
//! same repository.
//!
//! - `GET /v1/repositories/{id}/issues`: the issues of a repository, filtered by `parent`, `label`, `state`, `claimed`
//!   and `blocked` (`POST` on the same route creates one, see `cards`).
//! - `GET /v1/cards/{id}`: one issue with its state, open blockers and sub-issues.
//! - `PUT /v1/cards/{id}/parent` `{parent}`: gives the issue a parent, or none.
//! - `PUT /v1/cards/{id}/labels` `{labels}`: replaces its labels.
//! - `POST /v1/cards/{id}/blockers` `{blocker}` and `DELETE /v1/cards/{id}/blockers/{number}`: adds and removes a blocker.
//! - `POST /v1/cards/{id}/claim` `{by}` and `DELETE /v1/cards/{id}/claim` `{by}`: claims and releases it (a person
//!   releases any claim, the Moebius agent only its own, which it may name with `by`).
//! - `POST /v1/cards/{id}/close` `{reason}` and `POST /v1/cards/{id}/reopen` `{reason}`: closes and reopens it.
//! - `GET /v1/cards/{id}/comments` and `POST /v1/cards/{id}/comments` `{body}`: reads and writes its comments.

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::routing::{get, post, put};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, Reason, blocking};
use crate::app::{IssueQuery, IssueView};
use crate::decide::Actor;
use crate::store::{Card, Comment};

/// The routes of the issue tracker that are not the card's own (see `cards`).
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/cards/{id}/parent", put(set_parent))
        .route("/v1/cards/{id}/labels", put(set_labels))
        .route("/v1/cards/{id}/blockers", post(add_blocker))
        .route(
            "/v1/cards/{id}/blockers/{number}",
            axum::routing::delete(remove_blocker),
        )
        .route("/v1/cards/{id}/claim", post(claim).delete(release))
        .route("/v1/cards/{id}/close", post(close))
        .route("/v1/cards/{id}/reopen", post(reopen))
        .route("/v1/cards/{id}/comments", get(comments).post(comment))
}

/// The body of `PUT /v1/cards/{id}/parent`: the issue number of the parent, or null for none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewParent {
    pub parent: Option<i64>,
}

/// The body of `PUT /v1/cards/{id}/labels`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewLabels {
    pub labels: Vec<String>,
}

/// The body of `POST /v1/cards/{id}/blockers`: the issue number of the blocker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewBlocker {
    pub blocker: i64,
}

/// The body of `POST /v1/cards/{id}/claim`: who claims; the caller when left out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    #[serde(default)]
    pub by: Option<String>,
}

/// The body of `POST /v1/cards/{id}/comments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewComment {
    pub body: String,
}

pub(super) async fn list_issues(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    Query(query): Query<IssueQuery>,
) -> Result<axum::Json<Vec<IssueView>>, ApiError> {
    blocking(&state.app, move |app| app.issues(id, &query)).await
}

pub(super) async fn show_issue(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<IssueView>, ApiError> {
    blocking(&state.app, move |app| app.issue(id)).await
}

async fn set_parent(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(new): axum::Json<NewParent>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| app.set_parent(id, new.parent)).await
}

async fn set_labels(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(new): axum::Json<NewLabels>,
) -> Result<axum::Json<Card>, ApiError> {
    blocking(&state.app, move |app| app.set_labels(id, &new.labels)).await
}

async fn add_blocker(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(new): axum::Json<NewBlocker>,
) -> Result<axum::Json<Vec<i64>>, ApiError> {
    blocking(&state.app, move |app| app.add_blocker(id, new.blocker)).await
}

async fn remove_blocker(
    State(state): State<ApiState>,
    UrlPath((id, number)): UrlPath<(i64, i64)>,
) -> Result<axum::Json<Vec<i64>>, ApiError> {
    blocking(&state.app, move |app| app.remove_blocker(id, number)).await
}

async fn claim(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Claim>>,
) -> Result<axum::Json<Card>, ApiError> {
    let claimer = body.and_then(|body| body.0.by);
    blocking(&state.app, move |app| {
        app.claim_issue(&by, id, claimer.as_deref())
    })
    .await
}

async fn release(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Claim>>,
) -> Result<axum::Json<Card>, ApiError> {
    let claimer = body.and_then(|body| body.0.by);
    blocking(&state.app, move |app| {
        app.release_issue(&by, id, claimer.as_deref())
    })
    .await
}

async fn close(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Card>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| app.close_issue(&by, id, &reason)).await
}

async fn reopen(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<axum::Json<Reason>>,
) -> Result<axum::Json<Card>, ApiError> {
    let reason = body.map(|body| body.0.reason).unwrap_or_default();
    blocking(&state.app, move |app| app.reopen_issue(&by, id, &reason)).await
}

async fn comments(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Vec<Comment>>, ApiError> {
    blocking(&state.app, move |app| app.comments(id)).await
}

async fn comment(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(new): axum::Json<NewComment>,
) -> Result<axum::Json<Comment>, ApiError> {
    blocking(&state.app, move |app| app.comment_issue(&by, id, &new.body)).await
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::server::api::testing::{get, post, start};

    /// A request with the server's token, as the person.
    async fn send(
        state: &ApiState,
        addr: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> reqwest::Response {
        let mut request = reqwest::Client::new()
            .request(method, format!("http://{addr}{path}"))
            .bearer_auth(&*state.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        request.send().await.unwrap()
    }

    #[tokio::test]
    async fn the_tracker_routes_make_a_map_with_tickets_and_find_the_frontier() {
        let (_dir, state, addr) = start().await;
        let repository = 1;
        let issues = format!("/v1/repositories/{repository}/issues");
        let put = |path: String, body: Value| {
            let (state, addr) = (state.clone(), addr.clone());
            async move { send(&state, &addr, reqwest::Method::PUT, &path, Some(body)).await }
        };
        let mut events = state.app.bus.subscribe();

        // Card 1 exists already, number 1; the map is made with a label.
        let map: Card = post(
            &state,
            &addr,
            issues.clone(),
            json!({ "title": "Map", "labels": ["wayfinder:map"] }),
        )
        .await
        .json()
        .await
        .unwrap();
        assert_eq!(map.labels, ["wayfinder:map"]);
        let mut tickets = Vec::new();
        for (title, blocked_by) in [("Spike", json!([])), ("Decide", json!([3]))] {
            let ticket = post(
                &state,
                &addr,
                issues.clone(),
                json!({ "title": title, "parent": map.number, "labels": ["wayfinder:spike"], "blocked_by": blocked_by }),
            )
            .await;
            assert_eq!(ticket.status(), 200, "{title}");
            tickets.push(ticket.json::<Card>().await.unwrap());
        }
        assert_eq!(tickets[0].number, 3);
        assert_eq!(tickets[1].parent, Some(map.number));

        // A cycle, a parent that is not there and a second claim are refused with the right statuses.
        let cycle = post(
            &state,
            &addr,
            format!("/v1/cards/{}/blockers", tickets[0].id),
            json!({ "blocker": 4 }),
        )
        .await;
        assert_eq!(cycle.status(), 422);
        let missing = put(
            format!("/v1/cards/{}/parent", tickets[0].id),
            json!({ "parent": 99 }),
        )
        .await;
        assert_eq!(missing.status(), 404);

        let frontier = format!(
            "{issues}?parent={}&state=open&blocked=false&claimed=false",
            map.number
        );
        let found: Vec<IssueView> = get(&state, &addr, &frontier).await;
        assert_eq!(found.iter().map(|i| i.card.number).collect::<Vec<_>>(), [3]);
        assert_eq!(found[0].state, "open");

        let claim = format!("/v1/cards/{}/claim", tickets[0].id);
        assert_eq!(
            post(&state, &addr, claim.clone(), json!({ "by": "session 1" }))
                .await
                .status(),
            200
        );
        assert_eq!(
            post(&state, &addr, claim.clone(), json!({ "by": "session 2" }))
                .await
                .status(),
            409
        );
        let found: Vec<IssueView> = get(&state, &addr, &frontier).await;
        assert!(found.is_empty());
        let by_claimer = send(&state, &addr, reqwest::Method::DELETE, &claim, None).await;
        assert_eq!(by_claimer.status(), 200);
        assert_eq!(
            send(&state, &addr, reqwest::Method::DELETE, &claim, None)
                .await
                .status(),
            409
        );

        // Resolving a ticket: a comment, then closing; the ticket it blocked is on the frontier now.
        let comments = format!("/v1/cards/{}/comments", tickets[0].id);
        let comment = post(
            &state,
            &addr,
            comments.clone(),
            json!({ "body": "Use a queue." }),
        )
        .await;
        assert_eq!(comment.status(), 200);
        assert_eq!(
            post(&state, &addr, comments.clone(), json!({ "body": " " }))
                .await
                .status(),
            422
        );
        let read: Vec<Comment> = get(&state, &addr, &comments).await;
        assert_eq!(
            (read.len(), read[0].body.as_str(), read[0].author.as_str()),
            (1, "Use a queue.", "person")
        );
        let closed = post(
            &state,
            &addr,
            format!("/v1/cards/{}/close", tickets[0].id),
            json!({ "reason": "decided" }),
        )
        .await;
        assert_eq!(closed.status(), 200);
        let found: Vec<IssueView> = get(&state, &addr, &frontier).await;
        assert_eq!(found.iter().map(|i| i.card.number).collect::<Vec<_>>(), [4]);

        // The map shows its sub-issues with their state, and the snapshot carries what a board draws.
        let shown: IssueView = get(&state, &addr, &format!("/v1/cards/{}", map.id)).await;
        assert_eq!(
            shown
                .children
                .iter()
                .map(|c| (c.number, c.state.as_str()))
                .collect::<Vec<_>>(),
            [(3, "closed"), (4, "open")]
        );
        let snapshot: crate::server::api::Snapshot = get(&state, &addr, "/v1/snapshot").await;
        let spike = snapshot.boards[0]
            .cards
            .iter()
            .find(|c| c.card.number == 3)
            .unwrap();
        assert_eq!(
            (
                spike.card.parent,
                spike.card.closed,
                spike.card.labels.clone()
            ),
            (Some(map.number), true, vec!["wayfinder:spike".to_string()])
        );

        let reopened = post(
            &state,
            &addr,
            format!("/v1/cards/{}/reopen", tickets[0].id),
            json!({}),
        )
        .await;
        assert_eq!(reopened.json::<Card>().await.unwrap().column, "Backlog");
        let unblocked = send(
            &state,
            &addr,
            reqwest::Method::DELETE,
            &format!("/v1/cards/{}/blockers/3", tickets[1].id),
            None,
        )
        .await;
        assert_eq!(unblocked.json::<Vec<i64>>().await.unwrap(), [] as [i64; 0]);
        let labelled = put(
            format!("/v1/cards/{}/labels", tickets[1].id),
            json!({ "labels": [] }),
        )
        .await;
        assert!(labelled.json::<Card>().await.unwrap().labels.is_empty());
        let orphan = put(
            format!("/v1/cards/{}/parent", tickets[1].id),
            json!({ "parent": null }),
        )
        .await;
        assert_eq!(orphan.json::<Card>().await.unwrap().parent, None);

        let mut kinds = Vec::new();
        while let Ok(event) = events.try_recv() {
            kinds.push(event.kind);
        }
        for expected in [
            "card_added",
            "card_parent_changed",
            "card_labels_changed",
            "card_blocking_changed",
            "card_claimed",
            "card_released",
            "card_commented",
            "card_closed",
            "card_reopened",
        ] {
            assert!(
                kinds.iter().any(|k| k == expected),
                "{expected} in {kinds:?}"
            );
        }
    }
}
