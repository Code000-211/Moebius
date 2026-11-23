//! The wiki (see [`crate::wiki`]): its pages listed, read, written, moved and deleted, and a page's history. A page is
//! named by its path without `.md`; every route answers `409` while the server has no `[wiki]`.
//!
//! - `GET /v1/wiki/pages`: every page, `[{path, name, labels}]`, sorted by path.
//! - `GET /v1/wiki/pages/{*path}`: a page, `?at=<commit>` for an older version of it.
//! - `PUT /v1/wiki/pages/{*path}` `{text, message?, base?}`: writes a page, making it if it is not there; with `base`
//!   (the commit the writer loaded), `409` and the page as it now is when it has changed since.
//! - `PUT /v1/wiki/labels/{*path}` `{labels, base?}`: sets a page's labels, kept in its front matter, as a commit.
//! - `DELETE /v1/wiki/pages/{*path}`: deletes a page; its history stays.
//! - `POST /v1/wiki/moves` `{from, to}`: moves or renames a page.
//! - `GET /v1/wiki/history/{*path}`: the commits that changed a page, newest first.
//! - `GET /v1/wiki/search?q=&project=&limit=`: the pages found by words of their title and text.
//! - `GET /v1/wiki/links/{*path}`: what a page links to and what links to it, pages and cards.
//! - `GET /v1/wiki/cards/{key}`: the pages that link to a card.
//! - `POST /v1/wiki/refresh`: follows `origin` now, rather than at the server's next refresh.
//! - `POST /v1/wiki/proposals` `{path, text, message?}`: an agent's edit, a branch that waits in the inbox.
//! - `POST /v1/wiki/proposals/{id}/merge` and `/dismiss` `{reason?}`: a person's decision about it.

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking};
use crate::app::{App, AppError};
use crate::decide::Actor;
use crate::wiki::{Author, PageInfo, PageLinks, PageView, SearchHit, Seen, Version};

/// Refuses with `404` the page `path` of a private project that `by` does not see, as if there were no such page.
fn visible(app: &App, by: &Actor, path: &str) -> crate::app::Result<()> {
    if Seen::of(app, by)?.page(app, path) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("page {path}")))
    }
}

/// The routes of the wiki.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/wiki/pages", get(pages))
        .route(
            "/v1/wiki/pages/{*path}",
            get(page).put(write_page).delete(delete_page),
        )
        .route("/v1/wiki/labels/{*path}", put(set_labels))
        .route("/v1/wiki/moves", post(move_page))
        .route("/v1/wiki/history/{*path}", get(history))
        .route("/v1/wiki/search", get(search))
        .route("/v1/wiki/links/{*path}", get(links))
        .route("/v1/wiki/cards/{key}", get(card_pages))
        .route("/v1/wiki/refresh", post(refresh))
        .route("/v1/wiki/proposals", post(propose))
        .route("/v1/wiki/proposals/{id}/merge", post(merge_proposal))
        .route("/v1/wiki/proposals/{id}/dismiss", post(dismiss_proposal))
}

/// What `POST /v1/wiki/proposals/{id}/merge` and `/dismiss` take.
#[derive(Debug, Default, Deserialize)]
pub struct Decision {
    #[serde(default)]
    pub reason: String,
}

/// The open inbox item `id` that carries a proposal for the wiki.
fn proposal(app: &crate::app::App, id: i64) -> crate::app::Result<crate::store::InboxItem> {
    app.store
        .inbox()?
        .into_iter()
        .find(|item| item.id == id && item.kind == crate::wiki::WIKI_PROPOSAL)
        .ok_or_else(|| crate::app::AppError::NotFound(format!("proposal {id} for the wiki")))
}

async fn propose(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    Json(new): Json<ProposedPage>,
) -> Result<Json<crate::store::InboxItem>, ApiError> {
    blocking(&state.app, move |app| {
        let wiki = app.wiki()?;
        visible(app, &by, &new.path)?;
        wiki.propose(app, &by, &new.path, &new.text, new.message.as_deref())
    })
    .await
}

async fn merge_proposal(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<Json<Decision>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let reason = body.map(|Json(d)| d.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        let item = proposal(app, id)?;
        let commit = app.wiki()?.merge_proposal(app, &by, &item, &reason)?;
        Ok(serde_json::json!({ "commit": commit }))
    })
    .await
}

async fn dismiss_proposal(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    body: Option<Json<Decision>>,
) -> Result<Json<()>, ApiError> {
    let reason = body.map(|Json(d)| d.reason).unwrap_or_default();
    blocking(&state.app, move |app| {
        let item = proposal(app, id)?;
        app.wiki()?.dismiss_proposal(app, &by, &item, &reason)
    })
    .await
}

/// The query of `GET /v1/wiki/pages/{*path}`.
#[derive(Debug, Deserialize)]
pub struct At {
    /// A commit of the page's history, to read the page as it was then.
    pub at: Option<String>,
}

/// What `PUT /v1/wiki/pages/{*path}` takes.
#[derive(Debug, Deserialize)]
pub struct PageText {
    pub text: String,
    /// The commit message; without one, it says which page was added or updated.
    #[serde(default)]
    pub message: Option<String>,
    /// The commit of the page the writer loaded, or `""` for a page that was not there. When the page's latest commit
    /// is another, nothing is written and the answer is `409` with the page as it now is.
    #[serde(default)]
    pub base: Option<String>,
}

/// What `POST /v1/wiki/proposals` takes.
#[derive(Debug, Deserialize)]
pub struct ProposedPage {
    pub path: String,
    pub text: String,
    #[serde(default)]
    pub message: Option<String>,
}

/// What `POST /v1/wiki/moves` takes.
#[derive(Debug, Deserialize)]
pub struct Move {
    pub from: String,
    pub to: String,
}

async fn pages(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
) -> Result<Json<Vec<PageInfo>>, ApiError> {
    blocking(&state.app, move |app| {
        let pages = app.wiki()?.pages()?;
        let seen = Seen::of(app, &by)?;
        Ok(pages
            .into_iter()
            .filter(|page| seen.page(app, &page.path))
            .collect())
    })
    .await
}

async fn page(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(path): UrlPath<String>,
    Query(at): Query<At>,
) -> Result<Json<PageView>, ApiError> {
    blocking(&state.app, move |app| {
        let wiki = app.wiki()?;
        visible(app, &by, &path)?;
        wiki.page(&path, at.at.as_deref())
    })
    .await
}

async fn write_page(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(path): UrlPath<String>,
    Json(new): Json<PageText>,
) -> Result<Response, ApiError> {
    let app = state.app.clone();
    let written = tokio::task::spawn_blocking(move || {
        let wiki = app.wiki()?;
        visible(&app, &by, &path)?;
        let author = Author::of(&app, &by)?;
        wiki.write_page_from(
            &app,
            &author,
            &path,
            &new.text,
            new.message.as_deref(),
            new.base.as_deref(),
        )
    })
    .await
    .map_err(|e| ApiError::from(AppError::Io(std::io::Error::other(e))))?;
    saved(written)
}

/// The answer to a save: the page as it now is, or for a stale save `409` and the page as it is, so the editor can
/// show it without another read.
fn saved(written: crate::app::Result<PageView>) -> Result<Response, ApiError> {
    match written {
        Ok(page) => Ok(Json(page).into_response()),
        Err(error) => match &error {
            AppError::Stale { current, .. } => {
                let body = serde_json::json!({ "error": error.to_string(), "current": current });
                Ok((StatusCode::CONFLICT, Json(body)).into_response())
            }
            _ => Err(ApiError::from(error)),
        },
    }
}

/// What `PUT /v1/wiki/labels/{*path}` takes.
#[derive(Debug, Deserialize)]
pub struct PageLabels {
    pub labels: Vec<String>,
    /// As for `PUT /v1/wiki/pages/{*path}`.
    #[serde(default)]
    pub base: Option<String>,
}

async fn set_labels(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(path): UrlPath<String>,
    Json(new): Json<PageLabels>,
) -> Result<Response, ApiError> {
    let app = state.app.clone();
    let written = tokio::task::spawn_blocking(move || {
        let wiki = app.wiki()?;
        visible(&app, &by, &path)?;
        let author = Author::of(&app, &by)?;
        wiki.set_labels(&app, &author, &path, &new.labels, new.base.as_deref())
    })
    .await
    .map_err(|e| ApiError::from(AppError::Io(std::io::Error::other(e))))?;
    saved(written)
}

async fn delete_page(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(path): UrlPath<String>,
) -> Result<Json<()>, ApiError> {
    blocking(&state.app, move |app| {
        let wiki = app.wiki()?;
        visible(app, &by, &path)?;
        wiki.delete_page(app, &Author::of(app, &by)?, &path)
    })
    .await
}

async fn move_page(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    Json(moved): Json<Move>,
) -> Result<Json<PageView>, ApiError> {
    blocking(&state.app, move |app| {
        let wiki = app.wiki()?;
        visible(app, &by, &moved.from)?;
        visible(app, &by, &moved.to)?;
        wiki.move_page(app, &Author::of(app, &by)?, &moved.from, &moved.to)
    })
    .await
}

async fn history(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(path): UrlPath<String>,
) -> Result<Json<Vec<Version>>, ApiError> {
    blocking(&state.app, move |app| {
        let wiki = app.wiki()?;
        visible(app, &by, &path)?;
        wiki.history(&path)
    })
    .await
}

/// The query of `GET /v1/wiki/search`.
#[derive(Debug, Deserialize)]
pub struct Search {
    /// The words to find; every one must be in the page, the last may be a beginning.
    #[serde(default)]
    pub q: String,
    /// A project's key, to search only its pages.
    pub project: Option<String>,
    pub limit: Option<usize>,
}

async fn search(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    Query(search): Query<Search>,
) -> Result<Json<Vec<SearchHit>>, ApiError> {
    blocking(&state.app, move |app| {
        app.wiki()?
            .search(app, &by, &search.q, search.project.as_deref(), search.limit)
    })
    .await
}

async fn links(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(path): UrlPath<String>,
) -> Result<Json<PageLinks>, ApiError> {
    blocking(&state.app, move |app| app.wiki()?.links(app, &by, &path)).await
}

async fn card_pages(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(key): UrlPath<String>,
) -> Result<Json<Vec<PageInfo>>, ApiError> {
    blocking(&state.app, move |app| {
        app.wiki()?.pages_linking_card(app, &by, &key)
    })
    .await
}

async fn refresh(State(state): State<ApiState>) -> Result<Json<()>, ApiError> {
    blocking(&state.app, |app| app.wiki()?.refresh(app)).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode as Status;
    use serde_json::{Value, json};

    use crate::server::api::testing::start;
    use crate::store::Role;
    use crate::wiki::Wiki;
    use crate::wiki::testing::{bare_origin, push_from_elsewhere};

    async fn call(
        method: &str,
        addr: &str,
        token: &str,
        path: &str,
        body: Option<Value>,
    ) -> (Status, Value) {
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), format!("http://{addr}{path}"))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    #[tokio::test]
    async fn a_server_without_a_wiki_says_so() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let (status, body) = call("GET", &addr, &token, "/v1/wiki/pages", None).await;
        assert_eq!(status, Status::CONFLICT);
        assert!(body.to_string().contains("[wiki]"), "{body}");
    }

    #[tokio::test]
    async fn links_and_backlinks_are_read_and_a_rename_keeps_them() {
        let (dir, state, addr) = start().await;
        let origin = bare_origin(dir.path());
        let wiki = Wiki::open(
            &state.app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        let lea = state
            .app
            .store
            .add_person("lea", "Lea Roth", Role::Member)
            .unwrap();
        let token = format!("{}{:064}", super::super::TOKEN_PREFIX, 4);
        state
            .app
            .store
            .add_access_token(lea.id, "test", &token)
            .unwrap();
        let repository = state
            .app
            .store
            .add_repository("web", "/srv/web", "none", "main")
            .unwrap();
        state
            .app
            .store
            .add_card(repository.id, "Fix it", "See [[Runbook]]", "lea", "Backlog")
            .unwrap();

        for (path, text) in [
            ("Runbook", "Steps\n"),
            ("Index", "Read [[Runbook]] for WEB-1\n"),
        ] {
            let (status, _) = call(
                "PUT",
                &addr,
                &token,
                &format!("/v1/wiki/pages/{path}"),
                Some(json!({ "text": text })),
            )
            .await;
            assert_eq!(status, Status::OK);
        }
        let (status, links) = call("GET", &addr, &token, "/v1/wiki/links/Runbook", None).await;
        assert_eq!(status, Status::OK, "{links}");
        assert_eq!(
            links["backlinks"],
            json!({
                "pages": [{"path": "Index", "name": "Index", "labels": []}],
                "cards": [{"key": "WEB-1", "title": "Fix it"}]
            })
        );
        let (_, index) = call("GET", &addr, &token, "/v1/wiki/links/Index", None).await;
        assert_eq!(
            index["links"],
            json!({
                "pages": [{"link": "Runbook", "path": "Runbook"}],
                "cards": [{"key": "WEB-1", "title": "Fix it"}]
            })
        );
        let (status, pages) = call("GET", &addr, &token, "/v1/wiki/cards/WEB-1", None).await;
        assert_eq!(status, Status::OK);
        assert_eq!(
            pages,
            json!([{"path": "Index", "name": "Index", "labels": []}])
        );
        let (status, _) = call("GET", &addr, &token, "/v1/wiki/cards/WEB-9", None).await;
        assert_eq!(status, Status::NOT_FOUND);
        let (status, _) = call("GET", &addr, &token, "/v1/wiki/cards/web", None).await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);

        let (status, _) = call(
            "POST",
            &addr,
            &token,
            "/v1/wiki/moves",
            Some(json!({"from": "Runbook", "to": "Ops/Handbook"})),
        )
        .await;
        assert_eq!(status, Status::OK);
        let (_, index) = call("GET", &addr, &token, "/v1/wiki/pages/Index", None).await;
        assert_eq!(index["text"], "Read [[Handbook]] for WEB-1\n");
        let (_, links) = call("GET", &addr, &token, "/v1/wiki/links/Ops/Handbook", None).await;
        assert_eq!(
            links["backlinks"]["pages"],
            json!([{"path": "Index", "name": "Index", "labels": []}])
        );
    }

    #[tokio::test]
    async fn a_save_built_on_an_old_version_is_refused_with_the_page_as_it_is() {
        let (dir, state, addr) = start().await;
        let origin = bare_origin(dir.path());
        let wiki = Wiki::open(
            &state.app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        let token = state.token.to_string();
        let put = |text: &'static str, base: Option<Value>| {
            let mut body = json!({ "text": text });
            if let Some(base) = base {
                body["base"] = base;
            }
            call("PUT", &addr, &token, "/v1/wiki/pages/Handbook", Some(body))
        };

        // A page that is not there is made by a save that expects none.
        let (status, made) = put("Start\n", Some(json!(""))).await;
        assert_eq!(status, Status::OK, "{made}");
        let first = made["version"]["commit"].as_str().unwrap().to_string();
        let (status, again) = put("Other start\n", Some(json!(""))).await;
        assert_eq!(status, Status::CONFLICT, "{again}");
        assert_eq!(again["current"]["text"], "Start\n");

        // Two editors load the page; the first saves, the second is refused and nothing is written.
        let (status, owner) = put("OWNER-EDIT-1\n", Some(json!(first))).await;
        assert_eq!(status, Status::OK, "{owner}");
        let (status, refused) = put("TOM-EDIT-1\n", Some(json!(first))).await;
        assert_eq!(status, Status::CONFLICT, "{refused}");
        assert!(refused["error"].as_str().unwrap().contains("newer version"));
        assert_eq!(refused["current"]["text"], "OWNER-EDIT-1\n");
        assert_eq!(refused["current"]["version"], owner["version"]);
        let (_, now) = call("GET", &addr, &token, "/v1/wiki/pages/Handbook", None).await;
        assert_eq!(now["text"], "OWNER-EDIT-1\n");
        let (_, history) = call("GET", &addr, &token, "/v1/wiki/history/Handbook", None).await;
        assert_eq!(history.as_array().unwrap().len(), 2);

        // Built on the commit it has, a save goes through, also by its first 7 characters; without a base it always does.
        let latest = owner["version"]["commit"].as_str().unwrap()[..7].to_string();
        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/pages/Handbook",
            Some(json!({ "text": "TOM-EDIT-2\n", "base": latest })),
        )
        .await;
        assert_eq!(status, Status::OK);
        let (status, _) = put("NO-BASE\n", None).await;
        assert_eq!(status, Status::OK);

        // A page that is gone has no current version to show.
        let (status, _) = call("DELETE", &addr, &token, "/v1/wiki/pages/Handbook", None).await;
        assert_eq!(status, Status::OK);
        let (status, gone) = put("Back\n", Some(json!(first))).await;
        assert_eq!(status, Status::CONFLICT, "{gone}");
        assert_eq!(gone["current"], Value::Null);
    }

    #[tokio::test]
    async fn labels_are_set_listed_read_and_found_by_search() {
        let (dir, state, addr) = start().await;
        let origin = bare_origin(dir.path());
        let wiki = Wiki::open(
            &state.app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        let lea = state
            .app
            .store
            .add_person("lea", "Lea Roth", Role::Member)
            .unwrap();
        let token = format!("{}{:064}", super::super::TOKEN_PREFIX, 3);
        state
            .app
            .store
            .add_access_token(lea.id, "test", &token)
            .unwrap();
        let text = "---\nowner: ada\n---\n# Deploy\n\nShip it.\n";
        let (status, made) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/pages/Runbooks/Deploy",
            Some(json!({ "text": text })),
        )
        .await;
        assert_eq!(status, Status::OK, "{made}");
        assert_eq!(made["labels"], json!([]));
        assert_eq!(made["body"], "# Deploy\n\nShip it.\n");
        let base = made["version"]["commit"].as_str().unwrap().to_string();

        let (status, labelled) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/labels/Runbooks/Deploy",
            Some(json!({ "labels": ["Runbook", " On call "], "base": base })),
        )
        .await;
        assert_eq!(status, Status::OK, "{labelled}");
        assert_eq!(labelled["labels"], json!(["Runbook", "On call"]));
        assert_eq!(
            labelled["text"],
            "---\nowner: ada\nlabels: [Runbook, On call]\n---\n# Deploy\n\nShip it.\n"
        );
        assert_eq!(labelled["body"], "# Deploy\n\nShip it.\n");
        assert_eq!(
            labelled["version"]["message"],
            "Label Runbooks/Deploy: Runbook, On call"
        );

        let (_, listed) = call("GET", &addr, &token, "/v1/wiki/pages", None).await;
        assert_eq!(listed[0]["labels"], json!(["Runbook", "On call"]));
        let (_, hits) = call("GET", &addr, &token, "/v1/wiki/search?q=runbook", None).await;
        assert_eq!(hits[0]["path"], "Runbooks/Deploy", "{hits}");
        assert!(!hits[0]["snippet"].as_str().unwrap().contains("---"));

        // A save built on the page before the labels were set is refused, and says what the page is now.
        let (status, stale) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/labels/Runbooks/Deploy",
            Some(json!({ "labels": [], "base": base })),
        )
        .await;
        assert_eq!(status, Status::CONFLICT, "{stale}");
        assert_eq!(stale["current"]["labels"], json!(["Runbook", "On call"]));

        let (status, bad) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/labels/Runbooks/Deploy",
            Some(json!({ "labels": ["a,b"] })),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY, "{bad}");
        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/labels/Nothing",
            Some(json!({ "labels": ["A"] })),
        )
        .await;
        assert_eq!(status, Status::NOT_FOUND);

        let (status, cleared) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/labels/Runbooks/Deploy",
            Some(json!({ "labels": [] })),
        )
        .await;
        assert_eq!(status, Status::OK, "{cleared}");
        assert_eq!(
            cleared["text"],
            "---\nowner: ada\n---\n# Deploy\n\nShip it.\n"
        );
    }

    #[tokio::test]
    async fn a_page_is_made_edited_moved_read_with_its_history_and_deleted() {
        let (dir, state, addr) = start().await;
        let origin = bare_origin(dir.path());
        let wiki = Wiki::open(
            &state.app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        let lea = state
            .app
            .store
            .add_person("lea", "Lea Roth", Role::Member)
            .unwrap();
        let token = format!("{}{:064}", super::super::TOKEN_PREFIX, 3);
        state
            .app
            .store
            .add_access_token(lea.id, "test", &token)
            .unwrap();

        let (status, made) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/pages/Team/On%20call",
            Some(json!({"text": "# On call\n\nWho answers."})),
        )
        .await;
        assert_eq!(status, Status::OK, "{made}");
        assert_eq!(made["path"], "Team/On call");
        assert_eq!(made["name"], "On call");
        assert_eq!(made["version"]["author"], "Lea Roth");
        assert_eq!(made["version"]["handle"], "lea");
        let first = made["version"]["commit"].as_str().unwrap().to_string();

        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/pages/Team/On%20call",
            Some(json!({"text": "# On call\n\nThe person of the week.\n", "message": "Say who"})),
        )
        .await;
        assert_eq!(status, Status::OK);
        let (_, listed) = call("GET", &addr, &token, "/v1/wiki/pages", None).await;
        assert_eq!(
            listed,
            json!([{"path": "Team/On call", "name": "On call", "labels": []}])
        );

        let (status, moved) = call(
            "POST",
            &addr,
            &token,
            "/v1/wiki/moves",
            Some(json!({"from": "Team/On call", "to": "Handbook/On call"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{moved}");
        let (_, history) = call(
            "GET",
            &addr,
            &token,
            "/v1/wiki/history/Handbook/On%20call",
            None,
        )
        .await;
        let messages: Vec<&str> = history
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["message"].as_str().unwrap())
            .collect();
        assert_eq!(
            messages,
            [
                "Move Team/On call to Handbook/On call",
                "Say who",
                "Add Team/On call"
            ]
        );
        let (status, old) = call(
            "GET",
            &addr,
            &token,
            &format!("/v1/wiki/pages/Handbook/On%20call?at={first}"),
            None,
        )
        .await;
        assert_eq!(status, Status::OK, "{old}");
        assert_eq!(old["text"], "# On call\n\nWho answers.\n");

        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            "/v1/wiki/pages/..%2Fescape",
            Some(json!({"text": "x"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);

        push_from_elsewhere(dir.path(), &origin, "Elsewhere.md", "From outside\n");
        let (status, _) = call("POST", &addr, &token, "/v1/wiki/refresh", None).await;
        assert_eq!(status, Status::OK);
        let (_, outside) = call("GET", &addr, &token, "/v1/wiki/pages/Elsewhere", None).await;
        assert_eq!(outside["text"], "From outside\n");

        let (status, _) = call(
            "DELETE",
            &addr,
            &token,
            "/v1/wiki/pages/Handbook/On%20call",
            None,
        )
        .await;
        assert_eq!(status, Status::OK);
        let (status, _) = call(
            "GET",
            &addr,
            &token,
            "/v1/wiki/pages/Handbook/On%20call",
            None,
        )
        .await;
        assert_eq!(status, Status::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_agents_proposal_waits_in_the_inbox_until_a_member_merges_it() {
        let (dir, state, addr) = start().await;
        let origin = bare_origin(dir.path());
        push_from_elsewhere(dir.path(), &origin, "Setup.md", "# Setup\n");
        let wiki = Wiki::open(
            &state.app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        let lea = state
            .app
            .store
            .add_person("lea", "Lea Roth", Role::Member)
            .unwrap();
        let member = format!("{}{:064}", super::super::TOKEN_PREFIX, 3);
        state
            .app
            .store
            .add_access_token(lea.id, "test", &member)
            .unwrap();
        let agent = crate::server::api::testing::agent_token(&state);

        // The agent reads, but its direct edit is refused.
        let (status, _) = call("GET", &addr, &agent, "/v1/wiki/pages/Setup", None).await;
        assert_eq!(status, Status::OK);
        let (status, _) = call(
            "PUT",
            &addr,
            &agent,
            "/v1/wiki/pages/Setup",
            Some(json!({"text": "x"})),
        )
        .await;
        assert_eq!(status, Status::FORBIDDEN);

        // Its proposal is an inbox item and changes no page.
        let (status, item) = call(
            "POST",
            &addr,
            &agent,
            "/v1/wiki/proposals",
            Some(json!({"path": "Runbook", "text": "# Runbook", "message": "Add the runbook"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{item}");
        assert_eq!(item["kind"], "wiki_proposal");
        assert_eq!(item["title"], "Wiki: Add the runbook");
        let id = item["id"].as_i64().unwrap();
        let (status, _) = call("GET", &addr, &member, "/v1/wiki/pages/Runbook", None).await;
        assert_eq!(status, Status::NOT_FOUND);

        // An agent decides nothing; a member merges.
        let merge = format!("/v1/wiki/proposals/{id}/merge");
        let (status, _) = call("POST", &addr, &agent, &merge, Some(json!({}))).await;
        assert_eq!(status, Status::FORBIDDEN);
        let (status, merged) = call(
            "POST",
            &addr,
            &member,
            &merge,
            Some(json!({"reason": "ok"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{merged}");
        let (_, page) = call("GET", &addr, &member, "/v1/wiki/pages/Runbook", None).await;
        assert_eq!(page["text"], "# Runbook\n");
        assert!(state.app.store.inbox().unwrap().is_empty());
        let (status, _) = call("POST", &addr, &member, &merge, Some(json!({}))).await;
        assert_eq!(status, Status::NOT_FOUND);

        // A dismissed proposal changes nothing.
        let (_, second) = call(
            "POST",
            &addr,
            &agent,
            "/v1/wiki/proposals",
            Some(json!({"path": "Setup", "text": "Changed"})),
        )
        .await;
        let id = second["id"].as_i64().unwrap();
        let (status, _) = call(
            "POST",
            &addr,
            &member,
            &format!("/v1/wiki/proposals/{id}/dismiss"),
            None,
        )
        .await;
        assert_eq!(status, Status::OK);
        let (_, page) = call("GET", &addr, &member, "/v1/wiki/pages/Setup", None).await;
        assert_eq!(page["text"], "# Setup\n");
    }

    #[tokio::test]
    async fn the_wiki_is_searched_by_words_of_a_title_or_text() {
        let (dir, state, addr) = start().await;
        let origin = bare_origin(dir.path());
        let wiki = Wiki::open(
            &state.app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        let token = state.token.to_string();
        state.app.store.add_project("Web", "WEB").unwrap();
        for (path, text) in [
            ("Runbook", "refunds refunds refunds refunds"),
            ("Refunds", "money back"),
            ("projects/WEB/Plans", "refunds for the shop"),
        ] {
            let (status, made) = call(
                "PUT",
                &addr,
                &token,
                &format!("/v1/wiki/pages/{}", path.replace(' ', "%20")),
                Some(json!({"text": text})),
            )
            .await;
            assert_eq!(status, Status::OK, "{made}");
        }
        let (status, found) = call("GET", &addr, &token, "/v1/wiki/search?q=refunds", None).await;
        assert_eq!(status, Status::OK, "{found}");
        let paths: Vec<&str> = found
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths[0], "Refunds");
        assert_eq!(paths.len(), 3);
        assert_eq!(found[0]["name"], "Refunds");
        let (_, web) = call(
            "GET",
            &addr,
            &token,
            "/v1/wiki/search?q=refunds&project=WEB",
            None,
        )
        .await;
        assert_eq!(web.as_array().unwrap().len(), 1);
        assert_eq!(web[0]["project"], "WEB");
        let (status, none) = call("GET", &addr, &token, "/v1/wiki/search", None).await;
        assert_eq!(status, Status::OK);
        assert_eq!(none, json!([]));
    }

    #[tokio::test]
    async fn the_pages_of_a_private_project_are_its_members_and_nobody_elses() {
        let (dir, state, addr) = start().await;
        let origin = bare_origin(dir.path());
        let wiki = Wiki::open(
            &state.app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        let store = &state.app.store;
        let repository = store.repositories().unwrap()[0].clone();
        let project = store.project(repository.home_project_id).unwrap().unwrap();
        let key = project.key.clone();
        let mut tokens = Vec::new();
        for (n, (handle, role)) in [
            ("mia", Role::Member),
            ("lea", Role::Member),
            ("olga", Role::Owner),
        ]
        .into_iter()
        .enumerate()
        {
            let person = store.add_person(handle, handle, role).unwrap();
            let token = format!("{}{:064}", super::super::TOKEN_PREFIX, n + 3);
            store.add_access_token(person.id, "test", &token).unwrap();
            tokens.push((person.id, token));
        }
        let (mia, lea, olga) = (&tokens[0].1, &tokens[1].1, &tokens[2].1);
        let secret = format!("projects/{key}/Plans");
        let url = format!("/v1/wiki/pages/{}", secret.replace(' ', "%20"));
        for (path, text) in [
            (secret.as_str(), "the secret launch plan"),
            ("Home", "hello"),
        ] {
            let (status, made) = call(
                "PUT",
                &addr,
                mia,
                &format!("/v1/wiki/pages/{path}"),
                Some(json!({"text": text})),
            )
            .await;
            assert_eq!(status, Status::OK, "{made}");
        }
        let (_, history) = call(
            "GET",
            &addr,
            mia,
            &format!("/v1/wiki/history/{secret}"),
            None,
        )
        .await;
        let commit = history[0]["commit"].as_str().unwrap().to_string();
        let (status, proposed) = call(
            "POST",
            &addr,
            mia,
            "/v1/wiki/proposals",
            Some(json!({"path": secret, "text": "a secret edit"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{proposed}");
        let proposal = proposed["id"].as_i64().unwrap();
        store
            .set_project_access(project.id, true, &[tokens[0].0])
            .unwrap();

        // A member sees everything of the project.
        for token in [mia] {
            let (status, page) = call("GET", &addr, token, &url, None).await;
            assert_eq!(status, Status::OK, "{page}");
            let (_, listed) = call("GET", &addr, token, "/v1/wiki/pages", None).await;
            assert_eq!(listed.as_array().unwrap().len(), 2, "{listed}");
            let (_, found) = call("GET", &addr, token, "/v1/wiki/search?q=secret", None).await;
            assert_eq!(found.as_array().unwrap().len(), 1, "{found}");
        }

        // Neither another member nor an owner who is no member sees it, whichever route they ask.
        for token in [lea, olga] {
            let (status, _) = call("GET", &addr, token, &url, None).await;
            assert_eq!(status, Status::NOT_FOUND);
            let (status, _) = call("GET", &addr, token, &format!("{url}?at={commit}"), None).await;
            assert_eq!(status, Status::NOT_FOUND);
            let (status, _) = call(
                "GET",
                &addr,
                token,
                &format!("/v1/wiki/history/{secret}"),
                None,
            )
            .await;
            assert_eq!(status, Status::NOT_FOUND);
            let (status, _) = call("PUT", &addr, token, &url, Some(json!({"text": "x"}))).await;
            assert_eq!(status, Status::NOT_FOUND);
            let (status, _) = call(
                "PUT",
                &addr,
                token,
                &format!("/v1/wiki/pages/projects/{key}/New"),
                Some(json!({"text": "x"})),
            )
            .await;
            assert_eq!(status, Status::NOT_FOUND);
            let (status, _) = call("DELETE", &addr, token, &url, None).await;
            assert_eq!(status, Status::NOT_FOUND);
            let moves = [
                json!({"from": secret, "to": "Stolen"}),
                json!({"from": "Home", "to": format!("projects/{key}/Home")}),
            ];
            for moved in moves {
                let (status, _) = call("POST", &addr, token, "/v1/wiki/moves", Some(moved)).await;
                assert_eq!(status, Status::NOT_FOUND);
            }
            let (status, _) = call(
                "POST",
                &addr,
                token,
                "/v1/wiki/proposals",
                Some(json!({"path": secret, "text": "x"})),
            )
            .await;
            assert_eq!(status, Status::NOT_FOUND);
            for action in ["merge", "dismiss"] {
                let (status, _) = call(
                    "POST",
                    &addr,
                    token,
                    &format!("/v1/wiki/proposals/{proposal}/{action}"),
                    Some(json!({})),
                )
                .await;
                assert_eq!(status, Status::NOT_FOUND, "{action}");
            }
            let (status, _) = call(
                "POST",
                &addr,
                token,
                "/v1/wiki/from-template",
                Some(json!({"template": "any", "path": secret})),
            )
            .await;
            assert_eq!(status, Status::NOT_FOUND);
            let (_, listed) = call("GET", &addr, token, "/v1/wiki/pages", None).await;
            assert_eq!(
                listed,
                json!([{"path": "Home", "name": "Home", "labels": []}])
            );
            for query in ["q=secret", "q=secret&project=".to_string().as_str()] {
                let (status, found) = call(
                    "GET",
                    &addr,
                    token,
                    &format!(
                        "/v1/wiki/search?{query}{}",
                        if query.ends_with('=') {
                            key.as_str()
                        } else {
                            ""
                        }
                    ),
                    None,
                )
                .await;
                assert_eq!(status, Status::OK);
                assert_eq!(found, json!([]));
            }
            let (_, snapshot) = call("GET", &addr, token, "/v1/snapshot", None).await;
            assert!(
                !snapshot["inbox"].to_string().contains("secret"),
                "{}",
                snapshot["inbox"]
            );
        }
        // The proposal waits for a member, and the page is as it was.
        let (_, snapshot) = call("GET", &addr, mia, "/v1/snapshot", None).await;
        assert_eq!(snapshot["inbox"].as_array().unwrap().len(), 1);
        let (_, page) = call("GET", &addr, mia, &url, None).await;
        assert_eq!(page["text"], "the secret launch plan\n");

        // An agent without a person reads none of it.
        let agent = crate::server::api::testing::agent_token(&state);
        let (status, _) = call("GET", &addr, &agent, &url, None).await;
        assert_eq!(status, Status::NOT_FOUND);
        let (_, listed) = call("GET", &addr, &agent, "/v1/wiki/pages", None).await;
        assert_eq!(
            listed,
            json!([{"path": "Home", "name": "Home", "labels": []}])
        );
    }
}
