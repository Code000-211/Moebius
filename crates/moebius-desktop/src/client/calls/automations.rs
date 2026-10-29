//! The calls about automations: listing, saving and deleting them, firing one now and reading its firings.

use crate::client::{Automation, AutomationSettings, Client, ClientError, Firing};

impl Client {
    /// The automations, by name.
    pub async fn automations(&self) -> Result<Vec<Automation>, ClientError> {
        self.get("/v1/automations").await
    }

    /// Adds an automation, or replaces the automation `id`.
    pub async fn save_automation(
        &self,
        id: Option<i64>,
        settings: &AutomationSettings,
    ) -> Result<Automation, ClientError> {
        let body = serde_json::to_value(settings).expect("an automation is JSON");
        match id {
            Some(id) => self.put(&format!("/v1/automations/{id}"), body).await,
            None => self.post("/v1/automations", body).await,
        }
    }

    /// Deletes an automation; the chats it started stay.
    pub async fn delete_automation(&self, id: i64) -> Result<(), ClientError> {
        let _: serde_json::Value = self.delete(&format!("/v1/automations/{id}")).await?;
        Ok(())
    }

    /// Fires an automation now.
    pub async fn run_automation(&self, id: i64) -> Result<Firing, ClientError> {
        self.post(&format!("/v1/automations/{id}/run"), serde_json::json!({}))
            .await
    }

    /// The firings of an automation, newest first.
    pub async fn firings(&self, id: i64) -> Result<Vec<Firing>, ClientError> {
        self.get(&format!("/v1/automations/{id}/firings")).await
    }
}
