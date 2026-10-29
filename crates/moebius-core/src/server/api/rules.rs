//! The rules page (see [`crate::decide::rules`]).
//!
//! - `GET /v1/rules`: the built-in patterns, the rules in prose, those of each repository and the refusals of the last
//!   week. `PUT /v1/rules` `{text}` replaces the rules in prose of the harness configuration repository, or the server's
//!   `rules_file` without one.

use std::path::PathBuf;

use axum::Extension;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::app::AppError;
use crate::decide::Actor;
use crate::decide::rules;

/// The route of the rules.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/rules", get(get_rules).put(put_rules))
}

/// What `PUT /v1/rules` takes.
#[derive(Debug, Deserialize)]
struct NewRules {
    text: String,
}

/// What `PUT /v1/rules` answers: the rules page and the commit the change made.
#[derive(Debug, Serialize)]
struct SavedRules {
    #[serde(flatten)]
    view: rules::View,
    /// The short id of the commit; left out when the text changed nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
}

/// The rules file the server reads, and whether it is the harness configuration repository's.
fn rules_source(state: &ApiState) -> (Option<PathBuf>, bool) {
    match &state.harness_repo {
        Some(repo) => (Some(repo.path().join("rules.md")), true),
        None => (state.config.get().rules_file.clone(), false),
    }
}

async fn get_rules(State(state): State<ApiState>) -> Result<axum::Json<rules::View>, ApiError> {
    let (file, harness) = rules_source(&state);
    blocking(&state.app, move |app| {
        rules::view(app, file.as_deref(), harness)
    })
    .await
}

async fn put_rules(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    axum::Json(new): axum::Json<NewRules>,
) -> Result<axum::Json<SavedRules>, ApiError> {
    let (file, harness) = rules_source(&state);
    let Some(repo) = state.harness_repo.clone() else {
        // Without a harness configuration repository the rules are the server's own file.
        let Some(target) = file.clone() else {
            return Err(AppError::Conflict("the server has no rules file".into()).into());
        };
        return blocking(&state.app, move |app| {
            rules::save_file(app, &by, &target, &new.text)?;
            let view = rules::view(app, file.as_deref(), harness)?;
            Ok(SavedRules { view, commit: None })
        })
        .await;
    };
    blocking(&state.app, move |app| {
        let commit = repo.save_rules(app, &by, &new.text)?;
        let view = rules::view(app, file.as_deref(), harness)?;
        Ok(SavedRules { view, commit })
    })
    .await
}

#[cfg(test)]
mod tests {
    use crate::server::api::testing::start;

    #[tokio::test]
    async fn without_a_harness_repository_the_rules_are_a_file_next_to_the_configuration() {
        let (dir, state, addr) = start().await;
        let client = reqwest::Client::new();
        let put = |text: &str| {
            client
                .put(format!("http://{addr}/v1/rules"))
                .bearer_auth(&state.token)
                .json(&serde_json::json!({ "text": text }))
                .send()
        };
        let rules: serde_json::Value = client
            .get(format!("http://{addr}/v1/rules"))
            .bearer_auth(&state.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(rules["prose"]["editable"], true, "{rules}");

        let text = "## no-migrations\n\nNever edit the migrations.";
        assert_eq!(put(text).await.unwrap().status(), 200);
        let file = dir.path().join("rules.md");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("{text}\n"));
        let decided = state.app.store.judgements("edit_rules").unwrap();
        assert_eq!(decided.len(), 1);

        // Text the rules parser refuses leaves the file as it was.
        assert_eq!(put("## a\n## a\n").await.unwrap().status(), 422);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("{text}\n"));
    }
}
