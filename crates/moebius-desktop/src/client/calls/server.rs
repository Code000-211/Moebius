//! The calls about what the server runs with: health, models, plugins, the first start, settings, secrets and the
//! restart.

use crate::client::{
    Available, Client, ClientError, ClientReport, Health, Login, Models, Plugins, Provider,
    SavedSettings, ServerSettings, Start, Webhook,
};

impl Client {
    pub async fn health(&self) -> Result<Health, ClientError> {
        self.get("/v1/health").await
    }

    /// Tells the server where this app runs; the server keeps the latest report for the Moebius agent.
    pub async fn report_client(&self, report: &ClientReport) -> Result<ClientReport, ClientError> {
        self.put("/v1/client", serde_json::json!(report)).await
    }

    /// What the server last heard from an app.
    pub async fn reported_client(&self) -> Result<ClientReport, ClientError> {
        self.get("/v1/client").await
    }

    /// The upstreams, agents, teacher and decision model of the server.
    pub async fn models(&self) -> Result<Models, ClientError> {
        self.get("/v1/models").await
    }

    /// The models the upstreams serve; `refresh` asks them again instead of using what the server kept, `decision`
    /// asks the decision model's endpoint too.
    pub async fn available_models(
        &self,
        refresh: bool,
        decision: bool,
    ) -> Result<Available, ClientError> {
        self.get(&format!(
            "/v1/models/available?refresh={refresh}&decision={decision}"
        ))
        .await
    }

    /// Starts the OAuth login of the upstream at `place`; the person has confirmed the warning of its terms.
    pub async fn start_login(&self, place: usize) -> Result<Login, ClientError> {
        self.post(
            &format!("/v1/models/upstreams/{place}/login"),
            serde_json::json!({ "accept_terms": true }),
        )
        .await
    }

    /// The providers the server has built in, whose subscription an upstream can log in to.
    pub async fn providers(&self) -> Result<Vec<Provider>, ClientError> {
        self.get("/v1/models/providers").await
    }

    /// Finishes the waiting login of the upstream at `place` with the code, or the address the browser ended on.
    pub async fn paste_login(&self, place: usize, pasted: &str) -> Result<Login, ClientError> {
        self.post(
            &format!("/v1/models/upstreams/{place}/login/code"),
            serde_json::json!({ "pasted": pasted }),
        )
        .await
    }

    /// Where the OAuth login of the upstream at `place` stands.
    pub async fn login(&self, place: usize) -> Result<Login, ClientError> {
        self.get(&format!("/v1/models/upstreams/{place}/login"))
            .await
    }

    /// The plugins the server has loaded.
    pub async fn plugins(&self) -> Result<Plugins, ClientError> {
        self.get("/v1/plugins").await
    }

    /// Whether the server is ready for a first card, step by step.
    pub async fn start(&self) -> Result<Start, ClientError> {
        self.get("/v1/start").await
    }

    /// What the server runs with.
    pub async fn server_settings(&self) -> Result<ServerSettings, ClientError> {
        self.get("/v1/settings").await
    }

    /// Changes `moebius.toml`: the dotted keys to set to a value and those to remove.
    pub async fn save_settings(
        &self,
        set: serde_json::Map<String, serde_json::Value>,
        remove: Vec<String>,
    ) -> Result<SavedSettings, ClientError> {
        let body = serde_json::json!({ "set": set, "remove": remove });
        self.put("/v1/settings", body).await
    }

    /// The webhooks of the forges the server is set up for, with their secrets.
    pub async fn webhooks(&self) -> Result<Vec<Webhook>, ClientError> {
        self.get("/v1/webhooks").await
    }

    /// Makes a new secret for the webhook of `forge`, replacing the old one.
    pub async fn make_webhook_secret(&self, forge: &str) -> Result<Webhook, ClientError> {
        self.post(
            &format!("/v1/webhooks/{forge}/secret"),
            serde_json::json!({}),
        )
        .await
    }

    /// Stores an API key under `name`, on the server only.
    pub async fn store_secret(&self, name: &str, key: &str) -> Result<Vec<String>, ClientError> {
        self.put(
            &format!("/v1/secrets/{name}"),
            serde_json::json!({ "value": key }),
        )
        .await
    }

    /// Moves the agents and rules of `moebius.toml` into a new harness configuration repository, which the server uses
    /// once it restarts.
    pub async fn set_up_harness(&self) -> Result<SavedSettings, ClientError> {
        self.post("/v1/harness/setup", serde_json::json!({})).await
    }

    /// Asks the server to restart, which it answers before it does.
    pub async fn restart_server(&self) -> Result<(), ClientError> {
        let request = self
            .http
            .post(format!("{}/v1/server/restart", self.settings.address))
            .json(&serde_json::json!({}))
            .bearer_auth(&self.settings.token);
        let response = request
            .send()
            .await
            .map_err(|_| ClientError::Unreachable(self.settings.address.clone()))?;
        match response.status() {
            reqwest::StatusCode::UNAUTHORIZED => Err(ClientError::Unauthorized),
            status if status.is_success() => Ok(()),
            status => Err(ClientError::Server {
                status: status.as_u16(),
                message: response.text().await.unwrap_or_default(),
            }),
        }
    }
}
