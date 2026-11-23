//! Templates for issues and wiki pages, kept in the harness configuration repository (see [`crate::app`]'s
//! `templates`). An issue template is of one kind; a page template has a name. Editing one is a commit there.
//!
//! - `GET /v1/templates`: `{editable, issues: [{kind, title, body, fields}], pages: [{name, text}]}`.
//! - `PUT /v1/templates/issues/{kind}` `{title?, body?, fields?}` and `DELETE`: set or delete the template of a kind.
//! - `PUT /v1/templates/pages/{name}` `{text}` and `DELETE`: set or delete a page template.
//! - `POST /v1/wiki/from-template` `{template, path}`: makes the wiki page `path` from a page template.
//!
//! An issue is made from its kind's template with `use_template` on `POST /v1/repositories/{id}/issues`.

use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post, put};
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking};
use crate::app::{IssueTemplateView, PageTemplateView, Templates};
use crate::decide::Actor;
use crate::harness::repo::IssueTemplate;
use crate::wiki::PageView;

/// The routes of templates.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/templates", get(templates))
        .route(
            "/v1/templates/issues/{kind}",
            put(save_issue).delete(delete_issue),
        )
        .route(
            "/v1/templates/pages/{name}",
            put(save_page).delete(delete_page),
        )
        .route("/v1/wiki/from-template", post(from_template))
}

/// What `PUT /v1/templates/pages/{name}` takes.
#[derive(Debug, Deserialize)]
pub struct PageTemplateText {
    pub text: String,
}

/// What `POST /v1/wiki/from-template` takes.
#[derive(Debug, Deserialize)]
pub struct FromTemplate {
    pub template: String,
    pub path: String,
}

/// What a `DELETE` answers: the commit it made.
#[derive(Debug, Serialize)]
pub struct Deleted {
    pub commit: Option<String>,
}

async fn templates(State(state): State<ApiState>) -> Result<Json<Templates>, ApiError> {
    blocking(&state.app, |app| Ok(app.templates())).await
}

async fn save_issue(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(kind): UrlPath<String>,
    Json(template): Json<IssueTemplate>,
) -> Result<Json<IssueTemplateView>, ApiError> {
    blocking(&state.app, move |app| {
        app.save_issue_template(&by, &kind, &template)
    })
    .await
}

async fn delete_issue(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(kind): UrlPath<String>,
) -> Result<Json<Deleted>, ApiError> {
    blocking(&state.app, move |app| {
        Ok(Deleted {
            commit: app.delete_issue_template(&by, &kind)?,
        })
    })
    .await
}

async fn save_page(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
    Json(new): Json<PageTemplateText>,
) -> Result<Json<PageTemplateView>, ApiError> {
    blocking(&state.app, move |app| {
        app.save_page_template(&by, &name, &new.text)
    })
    .await
}

async fn delete_page(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<Json<Deleted>, ApiError> {
    blocking(&state.app, move |app| {
        Ok(Deleted {
            commit: app.delete_page_template(&by, &name)?,
        })
    })
    .await
}

async fn from_template(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    Json(new): Json<FromTemplate>,
) -> Result<Json<PageView>, ApiError> {
    blocking(&state.app, move |app| {
        app.page_from_template(&by, &new.template, &new.path)
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reqwest::StatusCode as Status;
    use serde_json::{Value, json};

    use crate::harness::repo::testing::use_harness;
    use crate::repos::git;
    use crate::server::api::testing::start;
    use crate::wiki::Wiki;
    use crate::wiki::testing::bare_origin;

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
    async fn an_issue_made_from_a_template_has_its_text_and_fields_and_an_edit_is_a_commit() {
        let (dir, state, addr) = start().await;
        let token = state.token.to_string();
        let harness = use_harness(&state.app, dir.path());

        // Without a template there is nothing to start from.
        let (_, none) = call("GET", &addr, &token, "/v1/templates", None).await;
        assert_eq!(none["issues"], json!([]));
        assert_eq!(none["editable"], true);

        let (status, kind) = call(
            "POST",
            &addr,
            &token,
            "/v1/kinds",
            Some(json!({"name": "Bug", "fields": [
                {"name": "Severity", "type": "choice", "options": ["low", "high"]}
            ]})),
        )
        .await;
        assert_eq!(status, Status::OK, "{kind}");
        let id = kind["id"].as_i64().unwrap();
        let path = harness.path().join(format!("templates/issues/{id}.toml"));
        let repo = state.app.store.repositories().unwrap()[0].clone();
        let (status, picked) = call(
            "PUT",
            &addr,
            &token,
            &format!("/v1/projects/{}/kinds", repo.home_project_id),
            Some(json!({"kinds": ["Bug"]})),
        )
        .await;
        assert_eq!(status, Status::OK, "{picked}");

        // A template for a field the kind lacks, or a kind that does not exist, is refused.
        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            "/v1/templates/issues/bug",
            Some(json!({"fields": {"Colour": "red"}})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            "/v1/templates/issues/nothing",
            Some(json!({})),
        )
        .await;
        assert_eq!(status, Status::NOT_FOUND);

        let (status, saved) = call(
            "PUT",
            &addr,
            &token,
            "/v1/templates/issues/bug",
            Some(json!({
                "title": "Bug: ",
                "body": "## Steps\n",
                "fields": {"severity": "high"}
            })),
        )
        .await;
        assert_eq!(status, Status::OK, "{saved}");
        assert_eq!(saved["kind"], "Bug");
        assert!(saved["commit"].is_string(), "{saved}");
        let log = git(harness.path(), &["log", "--format=%s", "-1"]).unwrap();
        assert_eq!(log, format!("Add the issue template {id}"));
        let file = std::fs::read_to_string(&path).unwrap();
        assert!(file.contains("severity = \"high\""), "{file}");

        // Saving it again unchanged commits nothing.
        let (_, again) = call(
            "PUT",
            &addr,
            &token,
            "/v1/templates/issues/Bug",
            Some(json!({"title": "Bug: ", "body": "## Steps\n", "fields": {"severity": "high"}})),
        )
        .await;
        assert!(again.get("commit").is_none_or(Value::is_null), "{again}");

        let (_, listed) = call("GET", &addr, &token, "/v1/templates", None).await;
        assert_eq!(listed["issues"][0]["kind"], "Bug");
        assert_eq!(listed["issues"][0]["fields"]["severity"], "high");

        // An issue made from it has the text and the fields; what the person gives wins.
        let (status, made) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/repositories/{}/issues", repo.id),
            Some(json!({"title": "", "kind": "Bug", "use_template": true})),
        )
        .await;
        assert_eq!(status, Status::OK, "{made}");
        assert_eq!(made["title"], "Bug: ");
        assert_eq!(made["body"], "## Steps\n");
        assert_eq!(made["kind"], "Bug");
        assert_eq!(made["fields"]["Severity"], "high");
        let (_, own) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/repositories/{}/issues", repo.id),
            Some(json!({"title": "Crash", "body": "mine", "kind": "Bug",
                "fields": {"Severity": "low"}, "use_template": true})),
        )
        .await;
        assert_eq!(own["title"], "Crash");
        assert_eq!(own["body"], "mine");
        assert_eq!(own["fields"]["Severity"], "low");

        // A kind with a template is needed, and a plain issue can still have a kind and fields.
        let (status, _) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/repositories/{}/issues", repo.id),
            Some(json!({"title": "x", "use_template": true})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let (status, _) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/repositories/{}/issues", repo.id),
            Some(json!({"title": "x", "fields": {"Severity": "low"}})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
        let (status, plain) = call(
            "POST",
            &addr,
            &token,
            &format!("/v1/repositories/{}/issues", repo.id),
            Some(json!({"title": "Slow", "kind": "Bug", "fields": {"Severity": "high"}})),
        )
        .await;
        assert_eq!(status, Status::OK, "{plain}");
        assert_eq!(plain["fields"]["Severity"], "high");

        let (status, deleted) =
            call("DELETE", &addr, &token, "/v1/templates/issues/bug", None).await;
        assert_eq!(status, Status::OK, "{deleted}");
        assert!(!path.exists());
        let (status, _) = call("DELETE", &addr, &token, "/v1/templates/issues/bug", None).await;
        assert_eq!(status, Status::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_page_made_from_a_template_has_its_text() {
        let (dir, state, addr) = start().await;
        let token = state.token.to_string();
        let harness = use_harness(&state.app, dir.path());
        let origin = bare_origin(dir.path());
        let wiki = Wiki::open(
            &state.app,
            Some(&origin.to_string_lossy()),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        state.app.set_wiki(Arc::new(wiki));
        // The owner's token is not a person's; a member's is.
        let lea = state
            .app
            .store
            .add_person("lea", "Lea Roth", crate::store::Role::Member)
            .unwrap();
        let member = format!("{}{:064}", super::super::TOKEN_PREFIX, 5);
        state
            .app
            .store
            .add_access_token(lea.id, "test", &member)
            .unwrap();

        for bad in ["../x", "a b", ""] {
            let (status, _) = call(
                "PUT",
                &addr,
                &member,
                &format!("/v1/templates/pages/{}", bad.replace(' ', "%20")),
                Some(json!({"text": "x"})),
            )
            .await;
            assert!(status.is_client_error(), "{bad}: {status}");
        }
        let (status, saved) = call(
            "PUT",
            &addr,
            &member,
            "/v1/templates/pages/runbook",
            Some(json!({"text": "# Runbook\n\nSteps.\n"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{saved}");
        assert_eq!(
            git(harness.path(), &["log", "--format=%s", "-1"]).unwrap(),
            "Add the page template runbook"
        );
        let (_, listed) = call("GET", &addr, &token, "/v1/templates", None).await;
        assert_eq!(listed["pages"][0]["name"], "runbook");

        let (status, page) = call(
            "POST",
            &addr,
            &member,
            "/v1/wiki/from-template",
            Some(json!({"template": "runbook", "path": "Team/Deploys"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{page}");
        assert_eq!(page["text"], "# Runbook\n\nSteps.\n");
        assert_eq!(page["version"]["handle"], "lea");
        let (status, _) = call(
            "POST",
            &addr,
            &member,
            "/v1/wiki/from-template",
            Some(json!({"template": "runbook", "path": "Team/Deploys"})),
        )
        .await;
        assert_eq!(status, Status::CONFLICT);
        let (status, _) = call(
            "POST",
            &addr,
            &member,
            "/v1/wiki/from-template",
            Some(json!({"template": "missing", "path": "Team/Other"})),
        )
        .await;
        assert_eq!(status, Status::NOT_FOUND);

        let (status, _) = call(
            "DELETE",
            &addr,
            &member,
            "/v1/templates/pages/runbook",
            None,
        )
        .await;
        assert_eq!(status, Status::OK);
        assert!(!harness.path().join("templates/pages/runbook.md").exists());
    }

    #[tokio::test]
    async fn a_server_without_a_harness_repository_lists_no_templates_and_refuses_edits() {
        let (_d, state, addr) = start().await;
        let token = state.token.to_string();
        let (_, listed) = call("GET", &addr, &token, "/v1/templates", None).await;
        assert_eq!(listed["editable"], false);
        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            "/v1/templates/pages/x",
            Some(json!({"text": "x"})),
        )
        .await;
        assert_eq!(status, Status::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn a_kind_with_any_name_has_a_template_that_survives_a_rename_and_a_viewer_cannot_edit() {
        let (dir, state, addr) = start().await;
        let token = state.token.to_string();
        let harness = use_harness(&state.app, dir.path());
        let (_, kind) = call(
            "POST",
            &addr,
            &token,
            "/v1/kinds",
            Some(json!({"name": "Bug report"})),
        )
        .await;
        let (status, saved) = call(
            "PUT",
            &addr,
            &token,
            "/v1/templates/issues/Bug%20report",
            Some(json!({"body": "text"})),
        )
        .await;
        assert_eq!(status, Status::OK, "{saved}");
        let (status, _) = call(
            "PUT",
            &addr,
            &token,
            &format!("/v1/kinds/{}", kind["id"]),
            Some(json!({"name": "Defect"})),
        )
        .await;
        assert_eq!(status, Status::OK);
        let (_, listed) = call("GET", &addr, &token, "/v1/templates", None).await;
        assert_eq!(listed["issues"][0]["kind"], "Defect");
        assert_eq!(listed["issues"][0]["body"], "text");
        assert_eq!(harness.issue_template_names().len(), 1);

        let viewer = state
            .app
            .store
            .add_person("vi", "Vi", crate::store::Role::Viewer)
            .unwrap();
        let vt = format!("{}{:064}", super::super::TOKEN_PREFIX, 6);
        state
            .app
            .store
            .add_access_token(viewer.id, "t", &vt)
            .unwrap();
        let (status, _) = call("GET", &addr, &vt, "/v1/templates", None).await;
        assert_eq!(status, Status::OK);
        let (status, _) = call(
            "PUT",
            &addr,
            &vt,
            "/v1/templates/pages/x",
            Some(json!({"text": "x"})),
        )
        .await;
        assert_eq!(status, Status::FORBIDDEN);
    }
}
