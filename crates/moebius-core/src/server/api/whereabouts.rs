//! Where Moebius runs, for the Moebius agent to find its way (see `app::whereabouts`):
//!
//! - `GET /v1/server`: the server's program, version, files, addresses, host and system.
//! - `GET /v1/client`: what the desktop app last reported about itself; 404 until it has.
//! - `PUT /v1/client`: the app reports itself when it connects and when that changes. A person's token only.

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;

use super::{ApiError, ApiState, person_only};
use crate::app::{ClientInfo, ServerInfo};
use crate::decide::Actor;

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/server", get(server))
        .route("/v1/client", get(client).put(report))
}

async fn server(State(state): State<ApiState>) -> Result<axum::Json<ServerInfo>, ApiError> {
    state.app.server_info().map(axum::Json).ok_or_else(|| {
        ApiError(
            StatusCode::NOT_FOUND,
            "this server did not record where it runs".into(),
        )
    })
}

async fn client(State(state): State<ApiState>) -> Result<axum::Json<ClientInfo>, ApiError> {
    state.app.client_info().map(axum::Json).ok_or_else(|| {
        ApiError(
            StatusCode::NOT_FOUND,
            "no desktop app has reported yet".into(),
        )
    })
}

async fn report(
    State(state): State<ApiState>,
    axum::Extension(by): axum::Extension<Actor>,
    axum::Json(info): axum::Json<ClientInfo>,
) -> Result<axum::Json<ClientInfo>, ApiError> {
    person_only(&by)?;
    Ok(axum::Json(state.app.set_client_info(info)))
}

#[cfg(test)]
mod tests {
    use super::super::testing::start;
    use super::*;

    fn info(address: &str) -> ClientInfo {
        ClientInfo {
            program: "/opt/moebius-desktop".into(),
            version: "9.9.9".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            host: "laptop".into(),
            server_address: address.into(),
            ssh_host: None,
            reported_at: 0,
        }
    }

    #[tokio::test]
    async fn the_app_reports_itself_and_the_latest_report_is_kept() {
        let (_dir, state, addr) = start().await;
        let http = reqwest::Client::new();
        let url = format!("http://{addr}/v1/client");
        let get = || http.get(&url).bearer_auth(&state.token).send();
        assert_eq!(get().await.unwrap().status(), 404);
        for address in ["http://127.0.0.1:1", "http://127.0.0.1:2"] {
            let sent = http
                .put(&url)
                .bearer_auth(&state.token)
                .json(&info(address))
                .send()
                .await
                .unwrap();
            assert_eq!(sent.status(), 200);
        }
        let read: ClientInfo = get().await.unwrap().json().await.unwrap();
        assert_eq!(read.server_address, "http://127.0.0.1:2");
        assert_eq!(read.host, "laptop");
        assert!(read.reported_at > 0);
    }

    #[tokio::test]
    async fn the_server_tells_where_it_runs() {
        let (_dir, state, addr) = start().await;
        let get = || {
            reqwest::Client::new()
                .get(format!("http://{addr}/v1/server"))
                .bearer_auth(&state.token)
                .send()
        };
        assert_eq!(get().await.unwrap().status(), 404);
        state.app.set_server_info(ServerInfo::detect(
            std::path::Path::new("/srv/moebius.toml"),
            std::path::Path::new("/srv/.moebius"),
            "127.0.0.1:7".into(),
        ));
        let read: ServerInfo = get().await.unwrap().json().await.unwrap();
        assert_eq!(read.config_file, "/srv/moebius.toml");
        assert_eq!(read.version, crate::VERSION);
    }

    #[test]
    fn an_agent_may_not_report() {
        let agent = Actor::Agent {
            name: "Moebius".into(),
            run: 1,
        };
        assert!(person_only(&agent).is_err());
    }
}
