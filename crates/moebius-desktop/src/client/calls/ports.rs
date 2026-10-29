//! The ports of the server.

use crate::client::{Client, ClientError, Port};

impl Client {
    /// The ports that runs, chats and terminals listen on (`GET /v1/ports`).
    pub async fn ports(&self) -> Result<Vec<Port>, ClientError> {
        self.get("/v1/ports").await
    }
}
