//! The few figures the desktop app's tray and rail show of a server, without the snapshot.
//!
//! - `GET /v1/status`: the cards with a run going, the open inbox items and the unread notices of the person asking, the
//!   tokens used today with the day's budget, and whether all agents are paused. They are what `GET /v1/snapshot`,
//!   `GET /v1/notices` and `GET /v1/usage` give for the same figures, from a few small reads instead of every board.

use std::collections::HashSet;

use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, blocking, privacy};
use crate::app::columns;
use crate::decide::Actor;

/// The route of the figures.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/v1/status", get(status))
}

/// The answer of `GET /v1/status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// The cards the person sees whose latest run is starting or running.
    pub working: usize,
    /// The inbox items the person sees that are not resolved yet.
    pub inbox: usize,
    /// The person's notices that are not read yet.
    pub unread_notices: usize,
    /// The tokens used since local midnight of the server.
    pub tokens: i64,
    /// `day_tokens` of `moebius.toml`, if it has one.
    pub day_budget: Option<i64>,
    /// Whether all agents are paused.
    pub paused: bool,
}

async fn status(
    axum::extract::State(state): axum::extract::State<ApiState>,
    Extension(by): Extension<Actor>,
) -> Result<Json<Status>, ApiError> {
    let day_budget = state.config.get().day_tokens;
    blocking(&state.app, move |app| {
        let sight = app.sight(&by)?;
        let mut cards = HashSet::new();
        for run in app.store.active_runs()? {
            let Some(id) = run.card_id else { continue };
            if matches!(run.purpose.as_str(), "attempt" | "trial")
                || !matches!(run.status.as_str(), "starting" | "running")
            {
                continue;
            }
            // A card deleted since the runs were read is no card working.
            let Ok(card) = app.card(id) else { continue };
            if card.archived_at.is_none()
                && card.column != columns::DONE
                && sight.repository(card.repository_id)
                && sight.card(&card)
            {
                cards.insert(id);
            }
        }
        let mut inbox = 0;
        for item in app.store.inbox()? {
            if privacy::sees_inbox_item(app, &sight, &item)? {
                inbox += 1;
            }
        }
        let mut unread_notices = 0;
        // Agents and scripts have no notices, and the figure is 0 for them.
        if matches!(by, Actor::Member { .. }) {
            for notice in app.notices(&by)? {
                if !notice.read && super::notices::sees_notice(app, &sight, &notice)? {
                    unread_notices += 1;
                }
            }
        }
        Ok(Status {
            working: cards.len(),
            inbox,
            unread_notices,
            tokens: app.store.tokens_today()?,
            day_budget,
            paused: app.store.runs_paused()?.is_some(),
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::testing::start;
    use crate::store::{Role, Usage};

    async fn read(addr: &str, token: &str) -> Status {
        reqwest::Client::new()
            .get(format!("http://{addr}/v1/status"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn the_status_counts_what_the_tray_and_the_rail_show() {
        let (_dir, state, addr) = start().await;
        let store = &state.app.store;
        assert_eq!(
            read(&addr, &state.token).await,
            Status {
                working: 0,
                inbox: 0,
                unread_notices: 0,
                tokens: 0,
                day_budget: Some(10000),
                paused: false
            }
        );

        let repo = store.add_repository("r", "/r", "none", "main").unwrap();
        let card = store
            .add_card(repo.id, "Fix it", "b", "person:lea", "Code")
            .unwrap();
        let run = store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        store.set_run_status(run.id, "running").unwrap();
        // A run that is not of a card, and a trial of the Retro, are not a card working.
        store.add_run(None, "Chatty", "test", "chat").unwrap();
        let trial = store
            .add_run(Some(card.id), "Coder", "test", "trial")
            .unwrap();
        store.set_run_status(trial.id, "running").unwrap();
        store
            .add_inbox_item("stuck", None, "Needs a person", &serde_json::json!({}))
            .unwrap();
        store
            .add_usage(&Usage {
                run_id: Some(run.id),
                model: "m".into(),
                input_tokens: 1000,
                output_tokens: 500,
                cached_tokens: 0,
                duration_ms: 10,
                tools: Vec::new(),
            })
            .unwrap();
        let lea = store.add_person("lea", "Lea", Role::Member).unwrap();
        let token = super::super::people::new_token().unwrap();
        store.add_access_token(lea.id, "t", &token).unwrap();
        state.app.set_assignees(card.id, &["lea".into()]).unwrap();
        state.app.store.pause_runs("person:owner").unwrap();

        let owner = read(&addr, &state.token).await;
        assert_eq!(
            (owner.working, owner.inbox, owner.tokens, owner.paused),
            (1, 1, 1500, true)
        );
        // The notice is Lea's alone.
        assert_eq!(owner.unread_notices, 0);
        assert_eq!(read(&addr, &token).await.unread_notices, 1);
    }
}
