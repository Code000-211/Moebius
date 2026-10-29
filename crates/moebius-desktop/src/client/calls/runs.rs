//! The calls about runs and chats: a run's events and decisions, steering and stopping, and every step of a chat.

use crate::client::{
    Change, Chat, Client, ClientError, Judgement, NewChat, Run, RunEvent, runs::ASSISTANT,
};

/// How many events one request for a run's events asks for.
const EVENT_PAGE: usize = 500;

impl Client {
    pub async fn run(&self, id: i64) -> Result<Run, ClientError> {
        self.get(&format!("/v1/runs/{id}")).await
    }

    /// Every event of a run with a `seq` above `after`, read in pages.
    pub async fn run_events(&self, id: i64, after: i64) -> Result<Vec<RunEvent>, ClientError> {
        let mut events: Vec<RunEvent> = Vec::new();
        loop {
            let from = events.last().map_or(after, |event| event.seq);
            let page: Vec<RunEvent> = self
                .get(&format!(
                    "/v1/runs/{id}/events?after={from}&limit={EVENT_PAGE}"
                ))
                .await?;
            let last = page.len() < EVENT_PAGE;
            events.extend(page);
            if last {
                return Ok(events);
            }
        }
    }

    /// The decisions recorded for a card, oldest first.
    pub async fn judgements(&self, card: i64) -> Result<Vec<Judgement>, ClientError> {
        self.oldest_first(&format!("card={card}")).await
    }

    /// The decisions recorded for the tool calls of a run, oldest first.
    pub async fn tool_judgements(&self, run: i64) -> Result<Vec<Judgement>, ClientError> {
        self.oldest_first(&format!("run={run}&kind=tool_call"))
            .await
    }

    async fn oldest_first(&self, filter: &str) -> Result<Vec<Judgement>, ClientError> {
        let mut judgements: Vec<Judgement> = self
            .get(&format!("/v1/judgements?{filter}&limit=1000"))
            .await?;
        judgements.reverse();
        Ok(judgements)
    }

    pub async fn steer(&self, run: i64, message: &str) -> Result<Run, ClientError> {
        self.post(
            &format!("/v1/runs/{run}/steer"),
            serde_json::json!({ "message": message }),
        )
        .await
    }

    pub async fn stop(&self, run: i64) -> Result<Run, ClientError> {
        self.post(&format!("/v1/runs/{run}/stop"), serde_json::json!({}))
            .await
    }

    /// The chats, newest first, those of the floating window included.
    pub async fn chats(&self) -> Result<Vec<Chat>, ClientError> {
        self.get("/v1/chats?all=true").await
    }

    /// Starts a chat (see `NewChat`).
    pub async fn start_chat(&self, chat: &NewChat) -> Result<Run, ClientError> {
        self.post("/v1/chats", serde_json::json!(chat)).await
    }

    /// Changes the model or the reasoning level of a waiting chat; what is not given stays.
    pub async fn change_chat_settings(
        &self,
        chat: i64,
        model: Option<&str>,
        thinking: Option<&str>,
    ) -> Result<Run, ClientError> {
        self.post(
            &format!("/v1/chats/{chat}/settings"),
            serde_json::json!({ "model": model, "thinking": thinking }),
        )
        .await
    }

    /// Starts a chat with the Moebius agent in `repository`, or in none, that the floating chat shows: the agent is
    /// also told `context` (blank for none), where the person is in the app, which the chat does not show. The chat's
    /// origin is `assistant`, so the Chat page leaves it out.
    pub async fn ask(
        &self,
        message: &str,
        repository: Option<i64>,
        context: &str,
    ) -> Result<Run, ClientError> {
        let body = serde_json::json!({
            "agent": crate::screens::chat::DEFAULT_AGENT,
            "message": message,
            "repository": repository,
            "context": context,
            "origin": ASSISTANT,
        });
        self.post("/v1/chats", body).await
    }

    /// Starts a chat that continues the ended chat `chat`: its agent and repository, told what was said in it.
    pub async fn continue_chat(&self, chat: i64, message: &str) -> Result<Run, ClientError> {
        self.post(
            "/v1/chats",
            serde_json::json!({ "message": message, "continues": chat }),
        )
        .await
    }

    /// Says that the person has read the chat up to the event `seq`.
    pub async fn mark_seen(&self, chat: i64, seq: i64) -> Result<(), ClientError> {
        let _: serde_json::Value = self
            .post(
                &format!("/v1/chats/{chat}/seen"),
                serde_json::json!({ "seq": seq }),
            )
            .await?;
        Ok(())
    }

    /// Writes to a chat; a working agent gets the message as a follow-up. The agent is also told `context`, where the
    /// person is in the app, when there is one.
    pub async fn say(
        &self,
        chat: i64,
        message: &str,
        context: Option<&str>,
    ) -> Result<Run, ClientError> {
        self.post(
            &format!("/v1/chats/{chat}/messages"),
            serde_json::json!({ "message": message, "context": context }),
        )
        .await
    }

    /// Offers what a chat's worktree changes as a change; the chat goes on.
    pub async fn offer_chat(&self, chat: i64) -> Result<Change, ClientError> {
        self.post(&format!("/v1/chats/{chat}/offer"), serde_json::json!({}))
            .await
    }

    /// Ends a chat: its work is committed and offered as a change.
    pub async fn end_chat(&self, chat: i64) -> Result<Run, ClientError> {
        self.post(&format!("/v1/chats/{chat}/end"), serde_json::json!({}))
            .await
    }
}
