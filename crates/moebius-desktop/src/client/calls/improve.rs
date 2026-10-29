//! The calls of the Improve pages: usage, the Retro and its proposals, the decision model, exports and research.

use crate::client::{
    Client, ClientError, Export, KindCounts, Proposal, Research, RetroSummary, RetroView,
    Shadowing, UsageReport,
};

impl Client {
    /// What the model calls used in the last `days` days.
    pub async fn usage(&self, days: i64) -> Result<UsageReport, ClientError> {
        self.get(&format!("/v1/usage?days={days}")).await
    }

    /// The Retros, the latest day first.
    pub async fn retros(&self) -> Result<Vec<RetroSummary>, ClientError> {
        self.get("/v1/retros").await
    }

    pub async fn retro(&self, date: &str) -> Result<RetroView, ClientError> {
        self.get(&format!("/v1/retros/{date}")).await
    }

    /// Runs today's Retro now.
    pub async fn start_retro(&self) -> Result<RetroView, ClientError> {
        self.post("/v1/retros", serde_json::json!({})).await
    }

    /// The proposals of all Retros, the newest first.
    pub async fn proposals(&self) -> Result<Vec<Proposal>, ClientError> {
        self.get("/v1/proposals").await
    }

    /// How well the decision model agrees with the recorded choices, and its takeovers.
    pub async fn shadowing(&self) -> Result<Shadowing, ClientError> {
        self.get("/v1/decision-model").await
    }

    pub async fn judgement_kinds(&self) -> Result<Vec<KindCounts>, ClientError> {
        self.get("/v1/judgements/kinds").await
    }

    /// The training-set exports, the newest first.
    pub async fn exports(&self) -> Result<Vec<Export>, ClientError> {
        self.get("/v1/exports").await
    }

    /// Writes an export of every kind on the server.
    pub async fn start_export(&self) -> Result<Export, ClientError> {
        self.post("/v1/exports", serde_json::json!({})).await
    }

    /// All research, the newest first.
    pub async fn researches(&self) -> Result<Vec<Research>, ClientError> {
        self.get("/v1/research").await
    }

    /// Writes the brief of a draft anew.
    pub async fn edit_research(
        &self,
        research: i64,
        goal: &str,
        measure: &str,
        better: &str,
        token_budget: i64,
    ) -> Result<Research, ClientError> {
        let body = serde_json::json!({
            "goal": goal, "measure": measure, "better": better, "token_budget": token_budget,
        });
        self.put(&format!("/v1/research/{research}"), body).await
    }

    /// Stores a brief as a draft.
    pub async fn create_research(
        &self,
        repository: i64,
        goal: &str,
        measure: &str,
        better: &str,
        token_budget: i64,
    ) -> Result<Research, ClientError> {
        let body = serde_json::json!({
            "repository": repository, "goal": goal, "measure": measure,
            "better": better, "token_budget": token_budget,
        });
        self.post("/v1/research", body).await
    }

    /// What a person does with a research: `confirm`, `start` or `stop`.
    pub async fn act_on_research(
        &self,
        research: i64,
        action: &str,
    ) -> Result<Research, ClientError> {
        let path = format!("/v1/research/{research}/{action}");
        self.post(&path, serde_json::json!({})).await
    }
}
