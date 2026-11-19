//! What a person pins to the sidebar and what they opened lately, kept for them on the server (ADR 0018), so that
//! both follow them to every browser they sign in on.
//!
//! - `GET /v1/places`: `{"favourites": [Place], "recent": [Place]}` of the person asking.
//! - `POST /v1/favourites` `{kind, repository_id, key, title}`: pins a place at the end of the list.
//! - `PUT /v1/favourites/order` `{ids}`: puts the pins in the order of `ids`.
//! - `DELETE /v1/favourites/{id}`: removes a pin.
//! - `POST /v1/recent` `{kind, repository_id, key, title}`: notes that the person opened a place.
//! - `DELETE /v1/recent/{id}`: forgets a recent place.
//!
//! Every answer but the delete of a single pin is the person's two lists. A place the person can no longer see — its
//! repository or its card is gone — is left out of the lists and forgotten the first time they are read. Only a
//! person may ask, never an agent's run token.

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use super::session::SignedIn;
use super::{ApiError, ApiState, internal};
use crate::store::{PLACE_KINDS, Person, Place};

/// The most characters a title may have.
const TITLE_LIMIT: usize = 200;

/// The routes of the favourites and the recent places.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/places", get(places))
        .route("/v1/favourites", post(pin))
        .route("/v1/favourites/order", put(reorder))
        .route("/v1/favourites/{id}", delete(unpin))
        .route("/v1/recent", post(see))
        .route("/v1/recent/{id}", delete(forget))
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, message.into())
}

/// The person who asked, or the refusal of an agent.
fn asker(signed_in: Option<Extension<SignedIn>>) -> Result<Person, ApiError> {
    signed_in
        .map(|Extension(s)| s.person)
        .ok_or_else(super::people::not_a_person)
}

/// The two lists of a person.
#[derive(Debug, Serialize, Deserialize)]
pub struct Places {
    pub favourites: Vec<Place>,
    pub recent: Vec<Place>,
}

/// What `POST /v1/favourites` and `POST /v1/recent` take. A project needs its `repository_id`, a card its `key` (the
/// card's id; its repository and title are the server's), a view or a page its `repository_id`, `key` (a slug) and
/// `title`.
#[derive(Debug, Deserialize)]
pub struct PlaceBody {
    pub kind: String,
    #[serde(default)]
    pub repository_id: Option<i64>,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub title: String,
}

/// What `PUT /v1/favourites/order` takes.
#[derive(Debug, Deserialize)]
pub struct Order {
    pub ids: Vec<i64>,
}

/// A place as the server will keep it: `(kind, repository id, key, title)`.
type Kept = (String, i64, String, String);

/// Checks a request and fills in what the server knows: a project's name, a card's repository and title.
fn kept(state: &ApiState, body: &PlaceBody) -> Result<Kept, ApiError> {
    if !PLACE_KINDS.contains(&body.kind.as_str()) {
        return Err(invalid(format!(
            "the kind is one of {}",
            PLACE_KINDS.join(", ")
        )));
    }
    let store = &state.app.store;
    let not_found = |what: String| ApiError(StatusCode::NOT_FOUND, what);
    let title = body.title.trim();
    if title.chars().count() > TITLE_LIMIT {
        return Err(invalid(format!(
            "the title is longer than {TITLE_LIMIT} characters"
        )));
    }
    match body.kind.as_str() {
        "card" => {
            let id: i64 = body
                .key
                .parse()
                .map_err(|_| invalid("the key of a card is its id"))?;
            let card = store
                .card(id)
                .map_err(internal)?
                .ok_or_else(|| not_found(format!("card {id}")))?;
            Ok((
                "card".into(),
                card.repository_id,
                id.to_string(),
                card.title,
            ))
        }
        kind => {
            let id = body
                .repository_id
                .ok_or_else(|| invalid("the repository_id is missing"))?;
            let repository = store
                .repository(id)
                .map_err(internal)?
                .ok_or_else(|| not_found(format!("repository {id}")))?;
            if kind == "project" {
                return Ok(("project".into(), id, String::new(), repository.name));
            }
            let key = body.key.trim();
            if key.is_empty() || title.is_empty() {
                return Err(invalid(format!("a {kind} needs a key and a title")));
            }
            Ok((kind.into(), id, key.into(), title.into()))
        }
    }
}

/// Refuses, as if it did not exist, a place of a private project that `person` does not see, so that a pin or a visit
/// cannot tell that it is there.
fn ensure_seen(state: &ApiState, person: &Person, kept: &Kept) -> Result<(), ApiError> {
    let (kind, repository, key, title) = kept;
    let place = Place {
        id: 0,
        kind: kind.clone(),
        repository_id: *repository,
        key: key.clone(),
        title: title.clone(),
    };
    let sight = state.app.sight_of(Some(person.id))?;
    if seen_place(state, &sight, &place)? {
        Ok(())
    } else {
        Err(ApiError(StatusCode::NOT_FOUND, format!("{kind} {key}")))
    }
}

/// The title a place has now, or none when the person can no longer see it. Every person sees every repository
/// today, so a place is gone when its repository or its card is.
fn current(state: &ApiState, place: &Place) -> Result<Option<String>, ApiError> {
    let store = &state.app.store;
    let Some(repository) = store.repository(place.repository_id).map_err(internal)? else {
        return Ok(None);
    };
    Ok(match place.kind.as_str() {
        "project" => Some(repository.name),
        "card" => match place.key.parse().ok().map(|id| store.card(id)) {
            Some(card) => card
                .map_err(internal)?
                .filter(|card| card.repository_id == place.repository_id)
                .map(|card| card.title),
            None => None,
        },
        _ => Some(place.title.clone()),
    })
}

/// Whether `sight` sees `place`: its repository, and its card for a card.
fn seen_place(
    state: &ApiState,
    sight: &crate::app::Sight,
    place: &Place,
) -> Result<bool, ApiError> {
    if !sight.repository(place.repository_id) {
        return Ok(false);
    }
    match (place.kind.as_str(), place.key.parse::<i64>()) {
        ("card", Ok(card)) => Ok(state.app.sees_card(sight, card)?),
        _ => Ok(true),
    }
}

/// The lists of `person` as they are, after forgetting what they can no longer see.
fn lists(state: &ApiState, person: &Person) -> Result<Places, ApiError> {
    let store = &state.app.store;
    let sight = state.app.sight_of(Some(person.id))?;
    let visible = |places: Vec<Place>, pins: bool| -> Result<Vec<Place>, ApiError> {
        let mut shown = Vec::new();
        for mut place in places {
            // A place of a private project the person is no longer a member of is left out, and kept for when they are
            // again.
            if !seen_place(state, &sight, &place)? {
                continue;
            }
            match current(state, &place)? {
                Some(title) => {
                    place.title = title;
                    shown.push(place);
                }
                None if pins => {
                    store.unpin(person.id, place.id).map_err(internal)?;
                }
                None => {
                    store.forget_recent(person.id, place.id).map_err(internal)?;
                }
            }
        }
        Ok(shown)
    };
    Ok(Places {
        favourites: visible(store.favourites(person.id).map_err(internal)?, true)?,
        recent: visible(store.recent_places(person.id).map_err(internal)?, false)?,
    })
}

async fn places(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
) -> Result<Json<Places>, ApiError> {
    let person = asker(signed_in)?;
    Ok(Json(lists(&state, &person)?))
}

async fn pin(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    Json(body): Json<PlaceBody>,
) -> Result<Json<Places>, ApiError> {
    let person = asker(signed_in)?;
    let kept = kept(&state, &body)?;
    ensure_seen(&state, &person, &kept)?;
    let (kind, repository, key, title) = kept;
    state
        .app
        .store
        .pin(person.id, &kind, repository, &key, &title)
        .map_err(internal)?;
    Ok(Json(lists(&state, &person)?))
}

async fn reorder(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    Json(order): Json<Order>,
) -> Result<Json<Places>, ApiError> {
    let person = asker(signed_in)?;
    state
        .app
        .store
        .reorder_favourites(person.id, &order.ids)
        .map_err(internal)?;
    Ok(Json(lists(&state, &person)?))
}

async fn unpin(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Places>, ApiError> {
    let person = asker(signed_in)?;
    if !state.app.store.unpin(person.id, id).map_err(internal)? {
        // Another person's pin is not shown to exist.
        return Err(ApiError(StatusCode::NOT_FOUND, format!("favourite {id}")));
    }
    Ok(Json(lists(&state, &person)?))
}

async fn see(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    Json(body): Json<PlaceBody>,
) -> Result<Json<Places>, ApiError> {
    let person = asker(signed_in)?;
    let kept = kept(&state, &body)?;
    ensure_seen(&state, &person, &kept)?;
    let (kind, repository, key, title) = kept;
    state
        .app
        .store
        .see(person.id, &kind, repository, &key, &title)
        .map_err(internal)?;
    Ok(Json(lists(&state, &person)?))
}

async fn forget(
    State(state): State<ApiState>,
    signed_in: Option<Extension<SignedIn>>,
    UrlPath(id): UrlPath<i64>,
) -> Result<Json<Places>, ApiError> {
    let person = asker(signed_in)?;
    if !state
        .app
        .store
        .forget_recent(person.id, id)
        .map_err(internal)?
    {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            format!("recent place {id}"),
        ));
    }
    Ok(Json(lists(&state, &person)?))
}

#[cfg(test)]
mod tests {
    use crate::server::api::TOKEN_PREFIX;
    use crate::server::api::testing::start;
    use crate::store::Role;
    use reqwest::StatusCode as Status;
    use serde_json::{Value, json};

    fn http() -> reqwest::Client {
        reqwest::Client::new()
    }

    async fn call(
        method: &str,
        addr: &str,
        token: &str,
        path: &str,
        body: Option<Value>,
    ) -> (Status, Value) {
        let mut request = http()
            .request(method.parse().unwrap(), format!("http://{addr}{path}"))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    /// A person with a token, and a second token of theirs (another browser).
    fn person(state: &crate::server::api::ApiState, handle: &str, n: u64) -> (i64, String, String) {
        let store = &state.app.store;
        let p = store.add_person(handle, handle, Role::Member).unwrap();
        let one = format!("{TOKEN_PREFIX}{:064}", n);
        let two = format!("{TOKEN_PREFIX}{:064}", n + 1000);
        store.add_access_token(p.id, "one", &one).unwrap();
        store.add_access_token(p.id, "two", &two).unwrap();
        (p.id, one, two)
    }

    #[tokio::test]
    async fn a_pin_and_the_recent_list_follow_the_person_to_another_browser() {
        let (_d, state, addr) = start().await;
        let (_, one, two) = person(&state, "lea", 1);
        let repo = state.app.store.repositories().unwrap()[0].id;
        let card = state.app.store.cards(repo).unwrap()[0].id;
        let (status, _) = call(
            "POST",
            &addr,
            &one,
            "/v1/favourites",
            Some(json!({"kind": "project", "repository_id": repo})),
        )
        .await;
        assert_eq!(status, Status::OK);
        call(
            "POST",
            &addr,
            &one,
            "/v1/favourites",
            Some(json!({"kind": "page", "repository_id": repo, "key": "intro", "title": "Intro"})),
        )
        .await;
        call(
            "POST",
            &addr,
            &one,
            "/v1/recent",
            Some(json!({"kind": "card", "key": card.to_string()})),
        )
        .await;
        let (_, seen) = call("GET", &addr, &two, "/v1/places", None).await;
        assert_eq!(seen["favourites"][0]["title"], "radiance");
        assert_eq!(seen["favourites"][1]["key"], "intro");
        assert_eq!(seen["recent"][0]["kind"], "card");
        assert_eq!(seen["recent"][0]["title"], "First");
        // Another person has their own lists.
        let (_, _, other) = person(&state, "bo", 2);
        let (_, theirs) = call("GET", &addr, &other, "/v1/places", None).await;
        assert_eq!(theirs["favourites"], json!([]));
        assert_eq!(theirs["recent"], json!([]));
    }

    #[tokio::test]
    async fn a_pin_is_removed_and_reordered() {
        let (_d, state, addr) = start().await;
        let (_, token, _) = person(&state, "lea", 1);
        let repo = state.app.store.repositories().unwrap()[0].id;
        for key in ["a", "b", "c"] {
            call(
                "POST",
                &addr,
                &token,
                "/v1/favourites",
                Some(json!({"kind": "view", "repository_id": repo, "key": key, "title": key})),
            )
            .await;
        }
        let (_, all) = call("GET", &addr, &token, "/v1/places", None).await;
        let id = |i: usize| all["favourites"][i]["id"].as_i64().unwrap();
        let (status, reordered) = call(
            "PUT",
            &addr,
            &token,
            "/v1/favourites/order",
            Some(json!({"ids": [id(2), id(0), id(1)]})),
        )
        .await;
        assert_eq!(status, Status::OK);
        let keys: Vec<&str> = reordered["favourites"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["key"].as_str().unwrap())
            .collect();
        assert_eq!(keys, ["c", "a", "b"]);
        let (status, left) = call(
            "DELETE",
            &addr,
            &token,
            &format!("/v1/favourites/{}", id(0)),
            None,
        )
        .await;
        assert_eq!(status, Status::OK);
        assert_eq!(left["favourites"].as_array().unwrap().len(), 2);
        // Someone else's pin is not theirs to remove.
        let (_, _, other) = person(&state, "bo", 2);
        let (status, _) = call(
            "DELETE",
            &addr,
            &other,
            &format!("/v1/favourites/{}", id(1)),
            None,
        )
        .await;
        assert_eq!(status, Status::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_place_the_person_can_no_longer_see_leaves_both_lists() {
        let (_d, state, addr) = start().await;
        let (pid, token, _) = person(&state, "lea", 1);
        let store = &state.app.store;
        let doomed = store
            .add_repository("doomed", "/srv/doomed", "none", "main")
            .unwrap();
        let card = store
            .add_card(doomed.id, "Gone soon", "", "lea", "Backlog")
            .unwrap();
        let kept = store.repositories().unwrap()[0].id;
        for body in [
            json!({"kind": "project", "repository_id": doomed.id}),
            json!({"kind": "card", "key": card.id.to_string()}),
            json!({"kind": "project", "repository_id": kept}),
        ] {
            for path in ["/v1/favourites", "/v1/recent"] {
                let (status, _) = call("POST", &addr, &token, path, Some(body.clone())).await;
                assert_eq!(status, Status::OK);
            }
        }
        assert!(store.delete_repository(doomed.id).unwrap());
        let (_, lists) = call("GET", &addr, &token, "/v1/places", None).await;
        for list in ["favourites", "recent"] {
            let places = lists[list].as_array().unwrap();
            assert_eq!(places.len(), 1, "{list}: {places:?}");
            assert_eq!(places[0]["repository_id"], kept);
        }
        // They are forgotten for good, not only hidden.
        assert_eq!(store.favourites(pid).unwrap().len(), 1);
        assert_eq!(store.recent_places(pid).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_card_of_a_private_project_cannot_be_pinned_or_visited_by_a_non_member() {
        let (_d, state, addr) = start().await;
        let (pid, token, _) = person(&state, "lea", 1);
        let store = &state.app.store;
        let repo = store.repositories().unwrap()[0].id;
        let card = store
            .add_card(repo, "Secret plan", "", "lea", "Backlog")
            .unwrap();
        store
            .set_project_access(card.project_id, true, &[])
            .unwrap();
        let body = json!({"kind": "card", "key": card.id.to_string()});
        for path in ["/v1/favourites", "/v1/recent"] {
            let (status, _) = call("POST", &addr, &token, path, Some(body.clone())).await;
            assert_eq!(status, Status::NOT_FOUND, "{path}");
        }
        assert!(store.favourites(pid).unwrap().is_empty());
        assert!(store.recent_places(pid).unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_card_that_is_deleted_leaves_the_lists() {
        let (_d, state, addr) = start().await;
        let (_, token, _) = person(&state, "lea", 1);
        let repo = state.app.store.repositories().unwrap()[0].id;
        let card = state.app.store.cards(repo).unwrap()[0].id;
        let body = json!({"kind": "card", "key": card.to_string()});
        call("POST", &addr, &token, "/v1/favourites", Some(body.clone())).await;
        call("POST", &addr, &token, "/v1/recent", Some(body)).await;
        state.app.store.delete_card(card, false).unwrap();
        let (_, lists) = call("GET", &addr, &token, "/v1/places", None).await;
        assert_eq!(lists["favourites"], json!([]));
        assert_eq!(lists["recent"], json!([]));
    }

    #[tokio::test]
    async fn a_bad_place_is_refused() {
        let (_d, state, addr) = start().await;
        let (_, token, _) = person(&state, "lea", 1);
        let repo = state.app.store.repositories().unwrap()[0].id;
        for (body, want) in [
            (json!({"kind": "wiki"}), Status::UNPROCESSABLE_ENTITY),
            (json!({"kind": "project"}), Status::UNPROCESSABLE_ENTITY),
            (
                json!({"kind": "project", "repository_id": 999}),
                Status::NOT_FOUND,
            ),
            (
                json!({"kind": "card", "key": "x"}),
                Status::UNPROCESSABLE_ENTITY,
            ),
            (json!({"kind": "card", "key": "999"}), Status::NOT_FOUND),
            (
                json!({"kind": "page", "repository_id": repo}),
                Status::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let (status, _) = call("POST", &addr, &token, "/v1/favourites", Some(body)).await;
            assert_eq!(status, want);
        }
    }
}
