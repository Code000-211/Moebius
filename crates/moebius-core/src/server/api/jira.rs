//! Importing from Jira (issue #390): a one-off copy of a Jira project into a Moebius project, which can be run again.
//!
//! - `POST /v1/projects/{id}/import/jira` `{url, project, token_secret, email?, users?, user_map?}`: reads the Jira
//!   project `project` of the site `url` with the token kept under `token_secret` in `secrets.toml` (stored first with
//!   `PUT /v1/secrets/{name}`), and imports it into the project; answers what it did (`JiraReport`).
//!
//! The secret's name starts with `jira-`, so that no other key of the server can be sent to a site. Only an owner and never an agent imports (it may make people). The token is read from `secrets.toml` for this one
//! request and is not stored, logged or answered.

use std::collections::BTreeMap;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiState, blocking, internal};
use crate::app::{JiraOptions, JiraReport, JiraUsers};
use crate::decide::Actor;
use crate::repos::jira::{self, Connection, JiraError};

/// The routes of the import.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/projects/{id}/import/jira", post(import))
}

/// What `POST /v1/projects/{id}/import/jira` takes.
#[derive(Debug, Deserialize)]
pub struct JiraImport {
    /// The address of the Jira site, such as `https://your-site.atlassian.net`.
    pub url: String,
    /// The key of the Jira project, such as `WEB`.
    pub project: String,
    /// The name the API token is kept under in `secrets.toml`.
    pub token_secret: String,
    /// The email of the account the token belongs to (Jira Cloud); without it the token is a personal access token.
    #[serde(default)]
    pub email: Option<String>,
    /// `people` (the default) makes a disabled person of each Jira user; `names` keeps names on what they wrote.
    #[serde(default)]
    pub users: JiraUsers,
    /// Jira users (by account id, email or name) who are people of Moebius already, with their handles.
    #[serde(default)]
    pub user_map: BTreeMap<String, String>,
}

fn bad_gateway(e: JiraError) -> ApiError {
    let status = match &e {
        JiraError::Address(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::BAD_GATEWAY,
    };
    ApiError(status, e.to_string())
}

async fn import(
    State(state): State<ApiState>,
    Extension(actor): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    Json(new): Json<JiraImport>,
) -> Result<Json<JiraReport>, ApiError> {
    if !jira::is_project_key(&new.project) {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{:?} is not a Jira project key", new.project),
        ));
    }
    // Only a key made for Jira is sent to a site: any other secret of the server stays where it is.
    if !new.token_secret.starts_with("jira-") {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "the token_secret of an import starts with jira-, such as jira-token".into(),
        ));
    }
    let site = jira::site_address(&new.url).map_err(bad_gateway)?;
    // The project first: nothing is asked of Jira for one that is not there or that the asker may not see.
    {
        let who = actor.clone();
        let _ = blocking(&state.app, move |app| {
            app.project(id)?;
            if app.may_see_project(&who, id) {
                Ok(())
            } else {
                Err(crate::app::AppError::NotFound(format!("project {id}")))
            }
        })
        .await?;
    }
    let token = state
        .config
        .secrets()
        .get(&new.token_secret)
        .map_err(internal)?
        .ok_or_else(|| {
            ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "no secret named {}: store the Jira token first with PUT /v1/secrets/{{name}}",
                    new.token_secret
                ),
            )
        })?;
    let connection = Connection {
        url: site.clone(),
        email: new.email.clone(),
        token,
    };
    let fetched = jira::fetch(&connection, &new.project)
        .await
        .map_err(bad_gateway)?;
    let options = JiraOptions {
        site,
        users: new.users,
        user_map: new.user_map,
    };
    blocking(&state.app, move |app| {
        app.import_jira(&actor, id, &options, &fetched)
    })
    .await
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode as Status;
    use serde_json::{Value, json};

    use crate::repos::jira_fake::{self, basic};
    use crate::server::api::testing::start;

    async fn post(addr: &str, token: &str, project: i64, body: Value) -> (Status, String) {
        let response = reqwest::Client::new()
            .post(format!("http://{addr}/v1/projects/{project}/import/jira"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        (response.status(), response.text().await.unwrap())
    }

    #[tokio::test]
    async fn a_project_is_imported_with_a_token_kept_in_the_secrets_and_imported_again_without_a_change()
     {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let project = state.app.add_project("Web shop", "WEB").unwrap().id;
        let (jira, _site) = jira_fake::serve(jira_fake::project(basic(
            "ada@example.com",
            "jira-token-xyz",
        )))
        .await;
        let request = json!({
            "url": jira, "project": "WEB", "email": "ada@example.com", "token_secret": "jira-token",
        });

        // The token has to be stored first; the server names what is missing.
        let (status, body) = post(&addr, &token, project, request.clone()).await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY, "{body}");
        assert!(body.contains("jira-token"), "{body}");

        state
            .config
            .secrets()
            .set("jira-token", "jira-token-xyz")
            .unwrap();
        let (status, body) = post(&addr, &token, project, request.clone()).await;
        assert_eq!(status, Status::OK, "{body}");
        let report: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(report["issues"]["created"], 5);
        assert_eq!(report["links"]["added"], 3);
        assert!(!body.contains("jira-token-xyz"));
        // What the import keeps holds no token.
        let kept = state
            .app
            .store
            .imported_of_kind(project, "jira", "issue")
            .unwrap();
        assert_eq!(kept.len(), 5);
        assert!(kept.iter().all(|(_, k)| !k.note.contains("jira-token-xyz")));
        for card in state.app.store.all_cards().unwrap() {
            assert!(!card.body.contains("jira-token-xyz"));
        }

        let (_, body) = post(&addr, &token, project, request.clone()).await;
        let again: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            again["issues"],
            json!({"created": 0, "updated": 0, "unchanged": 5})
        );

        // A token Jira refuses is a bad gateway that does not show it; an address that is none is refused.
        state
            .config
            .secrets()
            .set("jira-token", "jira-token-wrong")
            .unwrap();
        let (status, body) = post(&addr, &token, project, request.clone()).await;
        assert_eq!(status, Status::BAD_GATEWAY, "{body}");
        assert!(!body.contains("jira-token-wrong"));
        let (status, _) = post(
            &addr,
            &token,
            project,
            json!({"url": "jira", "project": "WEB", "token_secret": "jira-token"}),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let (status, _) = post(
            &addr,
            &token,
            project,
            json!({"url": jira, "project": "W\" OR 1=1", "token_secret": "jira-token"}),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let (status, _) = post(&addr, &token, 9999, request).await;
        assert_eq!(status, Status::NOT_FOUND);
    }
}
