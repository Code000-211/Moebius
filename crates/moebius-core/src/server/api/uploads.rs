//! Uploads: zip files a person hands the Moebius agent to import skills or MCP servers from.
//!
//! - `POST /v1/uploads`: stores a zip file (the raw body, at most [`UPLOAD_LIMIT`] bytes) under `<data_dir>/uploads/`;
//!   the answer names the file on the server.

use std::path::PathBuf;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{StatusCode, header};
use axum::routing::post;
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, internal};

/// The route of the uploads, with its own limit on the body.
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route(
        "/v1/uploads",
        post(upload).layer(DefaultBodyLimit::max(UPLOAD_LIMIT)),
    )
}

/// The largest file `POST /v1/uploads` takes: 20 MiB. A larger body is answered with 413.
pub const UPLOAD_LIMIT: usize = 20 * 1024 * 1024;

/// What `POST /v1/uploads` answers: the upload's id and where the file is on the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Upload {
    pub id: String,
    pub path: PathBuf,
}

/// Stores a zip file under `<data_dir>/uploads/<id>.zip`, where the Moebius agent reads it. Uploads are kept until
/// someone removes them.
async fn upload(
    State(state): State<ApiState>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<axum::Json<Upload>, ApiError> {
    let zip = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim() == "application/zip");
    if !zip {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "an upload is a zip file sent as application/zip".into(),
        ));
    }
    if body.is_empty() {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "the upload is empty".into(),
        ));
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(internal)?;
    let id = hex::encode(bytes);
    let dir = state.app.data_dir.join("uploads");
    let path = dir.join(format!("{id}.zip"));
    let written = path.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&written, &body)
    })
    .await
    .map_err(internal)?
    .map_err(internal)?;
    Ok(axum::Json(Upload { id, path }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::router;
    use crate::server::api::testing::start;

    #[tokio::test]
    async fn an_upload_is_stored_and_one_too_big_is_refused() {
        let (_dir, state, addr) = start().await;
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/v1/uploads");
        let send = |body: Vec<u8>, kind: &'static str| {
            client
                .post(&url)
                .bearer_auth(&*state.token)
                .header("content-type", kind)
                .body(body)
                .send()
        };
        let response = send(b"PK\x03\x04 a zip".to_vec(), "application/zip")
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let upload: Upload = response.json().await.unwrap();
        assert_eq!(
            upload.path,
            state
                .app
                .data_dir
                .join(format!("uploads/{}.zip", upload.id))
        );
        assert_eq!(std::fs::read(&upload.path).unwrap(), b"PK\x03\x04 a zip");
        // Too big: asked of the routes directly, since a server may close the connection before a client has sent
        // the whole body.
        let too_big = axum::http::Request::post("/v1/uploads")
            .header(header::AUTHORIZATION, format!("Bearer {}", state.token))
            .header(header::CONTENT_TYPE, "application/zip")
            .body(axum::body::Body::from(vec![0u8; UPLOAD_LIMIT + 1]))
            .unwrap();
        let response = tower::ServiceExt::oneshot(router(state.clone()), too_big)
            .await
            .unwrap();
        assert_eq!(response.status(), 413);
        let response = send(b"{}".to_vec(), "application/json").await.unwrap();
        assert_eq!(response.status(), 415);
        let files = std::fs::read_dir(state.app.data_dir.join("uploads")).unwrap();
        assert_eq!(files.count(), 1, "only the zip is stored");
    }
}
