//! The automations: chats a schedule starts with an agent and a prompt (see [`crate::app::automations`]).
//!
//! - `GET /v1/automations`: the automations, by name, each with its next firing and its latest one. `POST
//!   /v1/automations` `{name, schedule, agent, prompt, repository?, harness?, enabled?}`: adds one.
//! - `GET /v1/automations/{id}`, `PUT /v1/automations/{id}` (the same body as `POST`, which replaces it) and `DELETE
//!   /v1/automations/{id}`.
//! - `POST /v1/automations/{id}/run`: fires it now. `GET /v1/automations/{id}/firings`: its firings, newest first, each
//!   with its chat.

use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};

use super::{ApiError, ApiState, blocking};
use crate::app::AutomationView;
use crate::store::{Firing, NewAutomation};

/// The routes of the automations.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/automations", get(automations).post(add_automation))
        .route(
            "/v1/automations/{id}",
            get(automation)
                .put(save_automation)
                .delete(delete_automation),
        )
        .route("/v1/automations/{id}/run", post(run_automation))
        .route("/v1/automations/{id}/firings", get(firings))
}

async fn automations(
    State(state): State<ApiState>,
) -> Result<axum::Json<Vec<AutomationView>>, ApiError> {
    blocking(&state.app, |app| app.automations()).await
}

async fn add_automation(
    State(state): State<ApiState>,
    axum::Json(new): axum::Json<NewAutomation>,
) -> Result<axum::Json<AutomationView>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.save_automation(&*runs, None, &new)
    })
    .await
}

async fn automation(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<AutomationView>, ApiError> {
    blocking(&state.app, move |app| app.automation(id)).await
}

async fn save_automation(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
    axum::Json(new): axum::Json<NewAutomation>,
) -> Result<axum::Json<AutomationView>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| {
        app.save_automation(&*runs, Some(id), &new)
    })
    .await
}

async fn delete_automation(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    blocking(&state.app, move |app| {
        app.delete_automation(id).map(|()| serde_json::json!({}))
    })
    .await
}

async fn run_automation(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Firing>, ApiError> {
    let runs = state.runs.clone();
    blocking(&state.app, move |app| app.run_automation(&*runs, id)).await
}

async fn firings(
    State(state): State<ApiState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<axum::Json<Vec<Firing>>, ApiError> {
    blocking(&state.app, move |app| app.firings(id)).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::app::{App, cron, on_automation_schedule};
    use crate::server::api::testing::{get, kinds, post, start, status, until};

    async fn add(state: &ApiState, addr: &str, body: serde_json::Value) -> reqwest::Response {
        post(state, addr, "/v1/automations".into(), body).await
    }

    fn triage(prompt: &str) -> serde_json::Value {
        json!({ "name": "Triage", "schedule": "*/5 * * * *", "agent": "Coder", "prompt": prompt })
    }

    /// The local minute a minute after it is on the server.
    fn a_minute_from_now(app: &App) -> time::PrimitiveDateTime {
        cron::parse_minute(&app.store.local_minute().unwrap()).unwrap() + time::Duration::minutes(1)
    }

    #[tokio::test]
    async fn an_automation_is_added_listed_changed_and_deleted() {
        let (_dir, state, addr) = start().await;
        let added = add(&state, &addr, triage("Triage the new issues.")).await;
        assert_eq!(added.status(), 200);
        let added: AutomationView = added.json().await.unwrap();
        let a = &added.automation;
        assert_eq!(
            (
                a.name.as_str(),
                a.agent.as_str(),
                a.enabled,
                a.repository_id
            ),
            ("Triage", "Coder", true, None)
        );
        // It fires next within five minutes, on a minute that divides by five.
        let next = added.next_firing.unwrap();
        assert!(next > crate::store::now() - 60 && next <= crate::store::now() + 5 * 60);
        assert_eq!(next % 300, 0);
        assert!(added.last_firing.is_none());

        let listed: Vec<AutomationView> = get(&state, &addr, "/v1/automations").await;
        assert_eq!(listed, std::slice::from_ref(&added));

        let client = reqwest::Client::new();
        let put = |body: serde_json::Value| {
            client
                .put(format!("http://{addr}/v1/automations/{}", a.id))
                .bearer_auth(&*state.token)
                .json(&body)
                .send()
        };
        let mut off = triage("Triage the new issues.");
        off["enabled"] = json!(false);
        off["repository"] = json!(1);
        let changed: AutomationView = put(off).await.unwrap().json().await.unwrap();
        assert_eq!(
            (changed.automation.enabled, changed.automation.repository_id),
            (false, Some(1))
        );
        // While it is off it does not fire.
        assert_eq!(changed.next_firing, None);

        // What is wrong is said, and nothing is stored.
        for (body, says) in [
            (
                json!({ "name": "x", "schedule": "* * *", "agent": "Coder", "prompt": "p" }),
                "five fields",
            ),
            (
                json!({ "name": "x", "schedule": "* * * * *", "agent": "Nobody", "prompt": "p" }),
                "no agent",
            ),
            (
                json!({ "name": " ", "schedule": "* * * * *", "agent": "Coder", "prompt": "p" }),
                "name",
            ),
            (
                json!({ "name": "x", "schedule": "* * * * *", "agent": "Coder", "prompt": "" }),
                "prompt",
            ),
            (
                json!({ "name": "x", "schedule": "* * * * *", "agent": "Coder", "prompt": "p", "repository": 9 }),
                "not found",
            ),
        ] {
            let refused = add(&state, &addr, body).await;
            assert_ne!(refused.status(), 200);
            assert!(refused.text().await.unwrap().contains(says), "{says}");
        }
        assert_eq!(state.app.automations().unwrap().len(), 1);

        let deleted = client
            .delete(format!("http://{addr}/v1/automations/{}", a.id))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap();
        assert_eq!(deleted.status(), 200);
        let gone = client
            .get(format!("http://{addr}/v1/automations/{}", a.id))
            .bearer_auth(&*state.token)
            .send()
            .await
            .unwrap();
        assert_eq!(gone.status(), 404);
    }

    #[tokio::test]
    async fn a_firing_starts_a_chat_with_the_agent_which_ends_once_it_has_answered() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let added: AutomationView = add(&state, &addr, triage("Triage the new issues."))
            .await
            .json()
            .await
            .unwrap();
        let id = added.automation.id;
        let mut events = app.bus.subscribe();

        // The schedule fires it when it comes due.
        on_automation_schedule(
            app,
            &*state.runs,
            a_minute_from_now(app) + time::Duration::minutes(5),
        )
        .unwrap();
        let firings: Vec<Firing> =
            get(&state, &addr, &format!("/v1/automations/{id}/firings")).await;
        assert_eq!(firings.len(), 1);
        let firing = &firings[0];
        assert_eq!((firing.status.as_str(), firing.manual), ("started", false));
        let chat = firing.run_id.unwrap();
        let run = app.run(chat).unwrap();
        assert_eq!(
            (
                run.agent.as_str(),
                run.purpose.as_str(),
                run.origin.as_str()
            ),
            ("Coder", "chat", "automation")
        );
        let fired = loop {
            let event = events.recv().await.unwrap();
            if event.kind == "automation_fired" {
                break event;
            }
        };
        assert_eq!(fired.data["run"], chat);
        until("the agent has answered", || status(app, chat) == "waiting").await;
        let said = app.store.events(chat).unwrap();
        assert_eq!(said[0].data["message"], "Triage the new issues.");
        assert!(kinds(app, chat).contains(&"agent_settled".to_string()));
        // It is listed with the chats.
        assert!(
            app.chats(false)
                .unwrap()
                .iter()
                .any(|c| c.id == chat && c.origin == "automation")
        );

        // The next check ends the chat that has answered: a run never waits for a person.
        let later = a_minute_from_now(app);
        on_automation_schedule(app, &*state.runs, later).unwrap();
        until("the chat has ended", || {
            app.run(chat).unwrap().ended_at.is_some()
        })
        .await;
        assert_eq!(status(app, chat), "succeeded");
        // The same minute again fires nothing.
        on_automation_schedule(app, &*state.runs, later).unwrap();
        assert_eq!(app.firings(id).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_firing_due_while_the_chat_before_still_runs_is_skipped() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let added: AutomationView = add(&state, &addr, triage("Take your time [hang]"))
            .await
            .json()
            .await
            .unwrap();
        let id = added.automation.id;
        let path = format!("/v1/automations/{id}/run");
        let run = || post(&state, &addr, path.clone(), json!({}));
        let first: Firing = run().await.json().await.unwrap();
        assert_eq!((first.status.as_str(), first.manual), ("started", true));
        let chat = first.run_id.unwrap();
        until("the agent works", || status(app, chat) == "running").await;

        let second: Firing = run().await.json().await.unwrap();
        assert_eq!(second.status, "skipped");
        assert_eq!(second.run_id, None);
        assert!(second.reason.contains("still runs"), "{}", second.reason);
        // The schedule skips it too.
        on_automation_schedule(
            app,
            &*state.runs,
            a_minute_from_now(app) + time::Duration::minutes(5),
        )
        .unwrap();
        let firings = app.firings(id).unwrap();
        assert_eq!(
            firings
                .iter()
                .map(|f| (f.status.as_str(), f.manual))
                .collect::<Vec<_>>(),
            [("skipped", false), ("skipped", true), ("started", true)]
        );
        assert_eq!(firings[2].chat_status.as_deref(), Some("running"));
        let shown: AutomationView = get(&state, &addr, &format!("/v1/automations/{id}")).await;
        assert_eq!(shown.last_firing.unwrap().status, "skipped");

        // Once the chat has ended, the automation fires again.
        app.end_chat(&*state.runs, chat).unwrap();
        until("the chat has ended", || {
            app.run(chat).unwrap().ended_at.is_some()
        })
        .await;
        let third: Firing = run().await.json().await.unwrap();
        assert_eq!(third.status, "started");
        app.end_chat(&*state.runs, third.run_id.unwrap()).unwrap();
        until("the chat has ended", || {
            app.run(third.run_id.unwrap()).unwrap().ended_at.is_some()
        })
        .await;
    }
}
