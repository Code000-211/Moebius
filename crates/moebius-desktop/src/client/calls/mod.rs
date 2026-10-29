//! The calls of the client, one module per part of the API, and what they share: the requests with the token, the
//! answers read as JSON or as an error, and the routes whose parts are encoded.

mod board;
mod harness;
mod improve;
mod runs;
mod server;

use serde::de::DeserializeOwned;

use super::{Client, ClientError};

impl Client {
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let request = self.http.get(format!("{}{path}", self.settings.address));
        self.send(request).await
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ClientError> {
        let request = self
            .http
            .post(format!("{}{path}", self.settings.address))
            .json(&body);
        self.send(request).await
    }

    async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ClientError> {
        let request = self
            .http
            .put(format!("{}{path}", self.settings.address))
            .json(&body);
        self.send(request).await
    }

    async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let request = self.http.delete(format!("{}{path}", self.settings.address));
        self.send(request).await
    }

    async fn send<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ClientError> {
        let response = request
            .bearer_auth(&self.settings.token)
            .send()
            .await
            .map_err(|_| ClientError::Unreachable(self.settings.address.clone()))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if !status.is_success() {
            let message = response.text().await.unwrap_or_default();
            return Err(ClientError::Server {
                status: status.as_u16(),
                message,
            });
        }
        response
            .json()
            .await
            .map_err(|e| ClientError::Decode(e.to_string()))
    }
}
