//! The package manager (see [`crate::server::packages`] and `docs/reference/packages.md`).
//!
//! - `GET /v1/packages/search?q=`: the packages of the registries whose name, description or kind contains the text.
//! - `GET /v1/packages`: what is installed, with the newer version the nightly check found.
//! - `GET /v1/packages/{name}`: every version of a package, and what is installed of it.
//! - `POST /v1/packages` `{name, version?, registry?, repository?}`: installs a package. A plugin is installed by a
//!   person only; the Moebius agent may propose a workflow, rules or skills package.
//! - `POST /v1/packages/{name}/update` and `DELETE /v1/packages/{name}`: update or remove, by a person only.

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use serde::Deserialize;

use super::{ApiError, ApiState, internal, person_only};
use crate::decide::Actor;
use crate::server::packages::{Info, Installed, Outcome, PackageError, Packages, Request, Search};

/// The routes of the package manager.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/packages/search", get(search))
        .route("/v1/packages", get(list).post(install))
        .route("/v1/packages/{name}", get(info).delete(remove))
        .route("/v1/packages/{name}/update", post(update))
}

impl From<PackageError> for ApiError {
    fn from(e: PackageError) -> Self {
        let status = match &e {
            PackageError::NotFound(_) | PackageError::NotInstalled(_) => StatusCode::NOT_FOUND,
            PackageError::Installed(_) => StatusCode::CONFLICT,
            PackageError::PersonOnly => StatusCode::FORBIDDEN,
            PackageError::Download { .. } | PackageError::Registry { .. } => {
                StatusCode::BAD_GATEWAY
            }
            PackageError::Lock(_) | PackageError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            PackageError::App(app) => return ApiError::from_app(app),
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        Self(status, e.to_string())
    }
}

/// Runs `work` on the package manager off the async threads.
async fn run<T: Send + 'static>(
    state: &ApiState,
    work: impl FnOnce(&Packages) -> Result<T, PackageError> + Send + 'static,
) -> Result<axum::Json<T>, ApiError> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        work(&Packages {
            app: &state.app,
            live: &state.config,
            harness: state.harness_repo.as_deref(),
            moebius: crate::VERSION,
        })
    })
    .await
    .map_err(internal)?
    .map(axum::Json)
    .map_err(ApiError::from)
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
}

async fn search(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Result<axum::Json<Search>, ApiError> {
    run(&state, move |p| p.search(&query.q)).await
}

async fn list(State(state): State<ApiState>) -> Result<axum::Json<Vec<Installed>>, ApiError> {
    run(&state, |p| p.list()).await
}

async fn info(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<Info>, ApiError> {
    run(&state, move |p| p.info(&name)).await
}

async fn install(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    axum::Json(request): axum::Json<Request>,
) -> Result<axum::Json<Outcome>, ApiError> {
    run(&state, move |p| p.install(&by, &request)).await
}

async fn update(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<Outcome>, ApiError> {
    person_only(&by)?;
    run(&state, move |p| p.update(&by, &name)).await
}

async fn remove(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<Outcome>, ApiError> {
    person_only(&by)?;
    run(&state, move |p| p.remove(&by, &name)).await
}

// The plugin scripts of these tests are Python scripts started by their `#!` line, so the tests that run one are Unix
// only; what only they use is unused on Windows.
#[cfg_attr(not(unix), allow(dead_code, unused_imports))]
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, RwLock};

    use axum::response::IntoResponse;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    use crate::config::{AgentSet, Config};
    use crate::decide::Question;
    use crate::harness::repo::HarnessRepo;
    use crate::repos::git;
    use crate::server::api::testing::{origin, start};
    use crate::server::api::{ApiState, serve};
    use crate::server::packages::{Packages, current_target};
    use crate::server::settings::Live;

    type Files = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

    /// A plugin that decides with the first option and says which version it is.
    fn plugin_script(version: &str) -> Vec<u8> {
        format!(
            "#!/usr/bin/env python3\nimport json, sys\n\
             def send(m):\n    sys.stdout.write(json.dumps(m) + \"\\n\")\n    sys.stdout.flush()\n\
             send({{\"type\": \"manifest\", \"manifest\": {{\"name\": \"gate\", \"version\": \"{version}\", \"kinds\": [\"decider\"]}}}})\n\
             for line in sys.stdin:\n    m = json.loads(line)\n    \
             send({{\"type\": \"reply\", \"id\": m[\"id\"], \"result\": {{\"choice\": m[\"call\"][\"params\"][\"options\"][0], \"reason\": \"gate {version}\"}}}})\n"
        )
        .into_bytes()
    }

    /// A server of files on port 0 on a thread of its own, since the downloads block.
    fn serve_files(files: Files) -> String {
        let (sent, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                sent.send(listener.local_addr().unwrap()).unwrap();
                let app = axum::Router::new().route(
                    "/{*path}",
                    axum::routing::get(
                        move |axum::extract::Path(path): axum::extract::Path<String>| {
                            let files = files.clone();
                            async move {
                                match files.lock().unwrap().get(&path) {
                                    Some(bytes) => bytes.clone().into_response(),
                                    None => axum::http::StatusCode::NOT_FOUND.into_response(),
                                }
                            }
                        },
                    ),
                );
                axum::serve(listener, app).await.unwrap();
            });
        });
        received.recv().unwrap().to_string()
    }

    fn sha256(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    /// A server, a registry as a local git repository and a server of files for the downloads.
    struct Fx {
        dir: tempfile::TempDir,
        state: ApiState,
        addr: String,
        registry: PathBuf,
        files: Files,
        base: String,
    }

    impl Fx {
        async fn new() -> Self {
            Self::with_extra("").await
        }

        async fn with_extra(extra: &str) -> Self {
            let (dir, mut state, _) = start().await;
            let registry = dir.path().join("registry");
            std::fs::create_dir_all(&registry).unwrap();
            git(&registry, &["init", "--quiet", "-b", "main"]).unwrap();
            let files: Files = Files::default();
            let base = serve_files(files.clone());
            // The default registry is replaced by the local one, so that nothing reaches for the network.
            let text = format!(
                "[[registries]]\nname = \"moebius-plugins\"\nurl = {:?}\n{extra}",
                registry.display().to_string()
            );
            let file = dir.path().join("moebius.toml");
            std::fs::write(&file, &text).unwrap();
            let config = Config::parse(&text, dir.path(), &file).unwrap();
            state.config = Arc::new(Live::new(config));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
            tokio::spawn(serve(listener, state.clone()));
            let fx = Self {
                dir,
                state,
                addr,
                registry,
                files,
                base,
            };
            fx.publish(&[]);
            fx
        }

        /// Serves `bytes` at `path` and gives the sha256 an index names it by.
        fn host(&self, path: &str, bytes: &[u8]) -> String {
            self.files
                .lock()
                .unwrap()
                .insert(path.into(), bytes.to_vec());
            sha256(bytes)
        }

        /// The index entry of the plugin `gate` at `version`.
        fn gate(&self, version: &str, moebius: &str) -> String {
            let bytes = plugin_script(version);
            let sha = self.host(&format!("gate-{version}"), &bytes);
            self.gate_with(version, moebius, &sha)
        }

        fn gate_with(&self, version: &str, moebius: &str, sha: &str) -> String {
            format!(
                "[[package]]\nname = \"gate\"\nversion = \"{version}\"\nkind = \"plugin\"\n\
                 description = \"Holds cards marked wip.\"\nmoebius = \"{moebius}\"\n\
                 [package.plugin]\nkind = \"decider\"\nprogram = \"gate\"\ndecides = [\"start_card\"]\n\
                 [[package.downloads]]\ntarget = \"{}\"\nurl = \"http://{}/gate-{version}\"\nsha256 = \"{sha}\"\n",
                current_target(),
                self.base
            )
        }

        /// The entry of a package of `kind` with one `file` named `to`, served with `text`.
        fn files_package(&self, name: &str, kind: &str, to: &str, text: &str) -> String {
            self.files_package_at(name, "1.0.0", kind, to, text)
        }

        fn files_package_at(
            &self,
            name: &str,
            version: &str,
            kind: &str,
            to: &str,
            text: &str,
        ) -> String {
            let sha = self.host(&format!("{name}/{to}"), text.as_bytes());
            format!(
                "[[package]]\nname = \"{name}\"\nversion = \"{version}\"\nkind = \"{kind}\"\n\
                 description = \"A {kind} package.\"\nmoebius = \">=0.1\"\n\
                 [[package.files]]\nto = \"{to}\"\nurl = \"http://{}/{name}/{to}\"\nsha256 = \"{sha}\"\n",
                self.base
            )
        }

        /// Commits an index of `entries` to the registry.
        fn publish(&self, entries: &[String]) {
            let index = format!("format = 1\n{}", entries.join("\n"));
            std::fs::write(self.registry.join("index.toml"), index).unwrap();
            git(&self.registry, &["add", "."]).unwrap();
            git(
                &self.registry,
                &["commit", "--quiet", "--allow-empty", "-m", "index"],
            )
            .unwrap();
        }

        fn url(&self, path: &str) -> String {
            format!("http://{}{path}", self.addr)
        }

        async fn send(
            &self,
            method: reqwest::Method,
            path: &str,
            body: Option<Value>,
        ) -> (u16, Value) {
            self.send_as(&self.state.token, method, path, body).await
        }

        async fn send_as(
            &self,
            token: &str,
            method: reqwest::Method,
            path: &str,
            body: Option<Value>,
        ) -> (u16, Value) {
            let mut request = reqwest::Client::new()
                .request(method, self.url(path))
                .bearer_auth(token);
            if let Some(body) = body {
                request = request.json(&body);
            }
            let response = request.send().await.unwrap();
            let status = response.status().as_u16();
            let text = response.text().await.unwrap();
            (
                status,
                serde_json::from_str(&text).unwrap_or(Value::String(text)),
            )
        }

        async fn get(&self, path: &str) -> (u16, Value) {
            self.send(reqwest::Method::GET, path, None).await
        }

        async fn install(&self, body: Value) -> (u16, Value) {
            self.send(reqwest::Method::POST, "/v1/packages", Some(body))
                .await
        }

        /// The token of a run of the Moebius agent.
        fn agent_token(&self) -> String {
            let run = self
                .state
                .app
                .store
                .add_run(Some(1), "Moebius", "test", "chat")
                .unwrap();
            self.state.router.issue_token(crate::router::Grant {
                run_id: run.id,
                card_id: None,
                budget: None,
            })
        }

        fn config_text(&self) -> String {
            std::fs::read_to_string(self.dir.path().join("moebius.toml")).unwrap()
        }

        fn data_dir(&self) -> PathBuf {
            self.dir.path().join(".moebius")
        }

        /// The answer of the gate plugin to a `start_card` decision: its reason says which version answered.
        fn gate_says(&self) -> (String, String) {
            let decision = self
                .state
                .app
                .decisions
                .decide(&Question {
                    kind: "start_card".into(),
                    context: BTreeMap::from([("title".to_string(), "Add a menu".to_string())]),
                    options: vec!["start".into(), "hold".into()],
                    fail_closed: "hold".into(),
                    ..Question::default()
                })
                .unwrap();
            (decision.choice, decision.reason)
        }

        fn lock(&self) -> String {
            std::fs::read_to_string(self.data_dir().join("packages.lock")).unwrap_or_default()
        }

        fn kinds(
            &self,
            events: &mut tokio::sync::broadcast::Receiver<crate::bus::AppEvent>,
        ) -> Vec<String> {
            let mut kinds = vec![];
            while let Ok(event) = events.try_recv() {
                kinds.push(event.kind);
            }
            kinds
        }
    }

    #[tokio::test]
    async fn search_finds_packages_by_text_and_a_registry_that_fails_is_reported_beside_the_rest() {
        let broken = "[[registries]]\nname = \"broken\"\nurl = \"/nowhere/at/all\"\n";
        let fx = Fx::with_extra(broken).await;
        let notes = fx.files_package("review-flow", "workflow", "Review.luau", "-- review\n");
        fx.publish(&[
            fx.gate("1.0.0", ">=0.1"),
            fx.gate("1.1.0", ">=0.1"),
            fx.gate("2.0.0", ">=99"),
            notes,
        ]);

        let (status, found) = fx.get("/v1/packages/search?q=GATE").await;
        assert_eq!(status, 200, "{found}");
        let packages = found["packages"].as_array().unwrap();
        assert_eq!(packages.len(), 1, "{found}");
        // The newest version that works with this Moebius is the one offered.
        assert_eq!(packages[0]["version"], "1.1.0");
        assert_eq!(packages[0]["compatible"], true);
        assert_eq!(packages[0]["registry"], "moebius-plugins");
        assert_eq!(packages[0]["installed"], Value::Null);
        assert_eq!(found["problems"][0]["registry"], "broken");

        let (_, all) = fx.get("/v1/packages/search").await;
        assert_eq!(all["packages"].as_array().unwrap().len(), 2);
        let (_, by_kind) = fx.get("/v1/packages/search?q=workflow").await;
        assert_eq!(by_kind["packages"][0]["name"], "review-flow");

        let (status, info) = fx.get("/v1/packages/gate").await;
        assert_eq!(status, 200, "{info}");
        let versions: Vec<(&str, bool)> = info["versions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["version"].as_str().unwrap(),
                    v["compatible"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            versions,
            [("2.0.0", false), ("1.1.0", true), ("1.0.0", true)]
        );
        assert_eq!(info["versions"][1]["plugin"]["decides"][0], "start_card");
        // With a registry that cannot be read, a package that is nowhere cannot be told from one that is in it.
        assert_eq!(fx.get("/v1/packages/nope").await.0, 502);
    }

    #[tokio::test]
    async fn a_package_that_is_in_no_registry_is_not_found() {
        let fx = Fx::new().await;
        assert_eq!(fx.get("/v1/packages/nope").await.0, 404);
        assert_eq!(fx.install(json!({"name": "nope"})).await.0, 404);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_plugin_is_installed_with_its_entry_written_and_started() {
        let fx = Fx::new().await;
        fx.publish(&[fx.gate("1.0.0", ">=0.1")]);
        let mut events = fx.state.app.bus.subscribe();

        let (status, done) = fx.install(json!({"name": "gate"})).await;
        assert_eq!(status, 200, "{done}");
        assert_eq!(
            (done["action"].as_str(), done["version"].as_str()),
            (Some("installed"), Some("1.0.0"))
        );
        assert_eq!(done["plugin"]["state"], "running");
        assert_eq!(done["restart_needed"], false);

        // The program is in its folder, and the entry names it.
        let program = fx.data_dir().join("packages/gate/1.0.0/gate");
        assert_eq!(std::fs::read(&program).unwrap(), plugin_script("1.0.0"));
        let text = fx.config_text();
        assert!(
            text.contains("[[plugins]]") && text.contains("name = \"gate\""),
            "{text}"
        );
        assert!(text.contains(&program.display().to_string()), "{text}");
        assert!(text.contains("decides = [\"start_card\"]"), "{text}");
        // The server's start reads that entry as it reads any other.
        let config =
            Config::parse(&text, fx.dir.path(), &fx.dir.path().join("moebius.toml")).unwrap();
        assert_eq!(config.plugins[0].path, program);

        // The plugin is in the host and decides its kind at once.
        assert_eq!(
            fx.gate_says(),
            ("start".to_string(), "gate 1.0.0".to_string())
        );

        // The lock keeps where it came from and its sha256.
        let lock = fx.lock();
        assert!(lock.contains("registry = \"moebius-plugins\""), "{lock}");
        assert!(
            lock.contains(&format!("sha256 = \"{}\"", sha256(&plugin_script("1.0.0")))),
            "{lock}"
        );
        assert!(lock.contains("/gate-1.0.0"), "{lock}");
        assert!(
            fx.kinds(&mut events)
                .contains(&"package_installed".to_string())
        );

        let (_, installed) = fx.get("/v1/packages").await;
        assert_eq!(installed[0]["name"], "gate");
        assert_eq!(installed[0]["version"], "1.0.0");
        assert_eq!(installed[0]["update_available"], Value::Null);
        let (_, found) = fx.get("/v1/packages/search?q=gate").await;
        assert_eq!(found["packages"][0]["installed"], "1.0.0");
        // Once is enough.
        assert_eq!(fx.install(json!({"name": "gate"})).await.0, 409);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_download_that_does_not_match_its_sha256_installs_nothing() {
        let fx = Fx::new().await;
        fx.publish(&[fx.gate_with("1.0.0", ">=0.1", &"0".repeat(64))]);
        fx.host("gate-1.0.0", &plugin_script("1.0.0"));
        let (status, refused) = fx.install(json!({"name": "gate"})).await;
        assert_eq!(status, 422, "{refused}");
        let message = refused.as_str().unwrap();
        assert!(
            message.contains("sha256") && message.contains("nothing was installed"),
            "{message}"
        );
        assert!(!fx.data_dir().join("packages").exists());
        assert!(!fx.config_text().contains("[[plugins]]"));
        assert_eq!(fx.lock(), "");
        assert!(fx.state.app.decisions.host().status("gate").is_none());
        // A file that is not there is a failed download, not an install.
        fx.files.lock().unwrap().clear();
        assert_eq!(fx.install(json!({"name": "gate"})).await.0, 502);
    }

    #[tokio::test]
    async fn a_package_that_does_not_work_with_this_moebius_is_refused_with_a_sentence() {
        let fx = Fx::new().await;
        fx.publish(&[fx.gate("1.0.0", ">=0.1"), fx.gate("2.0.0", ">=99")]);
        // Asked for by version, the incompatible one is refused and says what it needs.
        let (status, refused) = fx
            .install(json!({"name": "gate", "version": "2.0.0"}))
            .await;
        assert_eq!(status, 422, "{refused}");
        assert_eq!(
            refused.as_str().unwrap(),
            format!(
                "gate 2.0.0 works with Moebius >=99, and this is Moebius {}",
                crate::VERSION
            )
        );
        assert_eq!(fx.lock(), "");
        assert!(!fx.data_dir().join("packages").exists());
        assert_eq!(
            fx.install(json!({"name": "gate", "version": "9.9.9"}))
                .await
                .0,
            422
        );
        assert_eq!(
            fx.install(json!({"name": "gate", "registry": "elsewhere"}))
                .await
                .0,
            422
        );
        // With only incompatible versions, there is nothing to pick.
        fx.publish(&[fx.gate("2.0.0", ">=99")]);
        let (status, refused) = fx.install(json!({"name": "gate"})).await;
        assert_eq!(status, 422);
        assert!(refused.as_str().unwrap().contains(">=99"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_update_swaps_the_program_and_restarts_it_and_a_removal_takes_everything_away() {
        let fx = Fx::new().await;
        fx.publish(&[fx.gate("1.0.0", ">=0.1")]);
        assert_eq!(fx.install(json!({"name": "gate"})).await.0, 200);
        assert_eq!(fx.gate_says().1, "gate 1.0.0");
        let mut events = fx.state.app.bus.subscribe();

        // Nothing newer: nothing changes.
        let (status, same) = fx
            .send(reqwest::Method::POST, "/v1/packages/gate/update", None)
            .await;
        assert_eq!((status, same["action"].as_str()), (200, Some("unchanged")));

        fx.publish(&[fx.gate("1.0.0", ">=0.1"), fx.gate("1.1.0", ">=0.1")]);
        let (status, updated) = fx
            .send(reqwest::Method::POST, "/v1/packages/gate/update", None)
            .await;
        assert_eq!(status, 200, "{updated}");
        assert_eq!(
            (updated["action"].as_str(), updated["version"].as_str()),
            (Some("updated"), Some("1.1.0"))
        );
        assert_eq!(updated["plugin"]["state"], "running");
        assert_eq!(fx.gate_says().1, "gate 1.1.0");
        let new_program = fx.data_dir().join("packages/gate/1.1.0/gate");
        assert!(new_program.is_file());
        assert!(!fx.data_dir().join("packages/gate/1.0.0").exists());
        let text = fx.config_text();
        assert!(text.contains(&new_program.display().to_string()), "{text}");
        assert_eq!(text.matches("[[plugins]]").count(), 1);
        assert!(fx.lock().contains("version = \"1.1.0\""));
        assert!(
            fx.kinds(&mut events)
                .contains(&"package_installed".to_string())
        );

        // A new version that does not start does not replace the one that runs.
        let broken = b"#!/bin/sh\nexit 3\n";
        let sha = fx.host("gate-1.2.0", broken);
        fx.publish(&[fx.gate_with("1.2.0", ">=0.1", &sha)]);
        let (status, refused) = fx
            .send(reqwest::Method::POST, "/v1/packages/gate/update", None)
            .await;
        assert_eq!(status, 422, "{refused}");
        assert!(
            refused.as_str().unwrap().contains("1.1.0 stays"),
            "{refused}"
        );
        assert_eq!(fx.gate_says().1, "gate 1.1.0");
        assert!(!fx.data_dir().join("packages/gate/1.2.0").exists());
        assert!(fx.lock().contains("version = \"1.1.0\""));

        // Removing takes the entry, the files and the lock entry, and stops the plugin.
        let (status, removed) = fx
            .send(reqwest::Method::DELETE, "/v1/packages/gate", None)
            .await;
        assert_eq!((status, removed["action"].as_str()), (200, Some("removed")));
        assert!(
            !fx.config_text().contains("[[plugins]]"),
            "{}",
            fx.config_text()
        );
        assert!(!fx.data_dir().join("packages/gate").exists());
        assert!(!fx.lock().contains("gate"));
        assert!(fx.state.app.decisions.host().status("gate").is_none());
        assert!(!fx.state.app.decisions.decides("start_card"));
        assert!(
            fx.kinds(&mut events)
                .contains(&"package_removed".to_string())
        );
        assert_eq!(
            fx.send(reqwest::Method::DELETE, "/v1/packages/gate", None)
                .await
                .0,
            404
        );
        assert_eq!(
            fx.send(reqwest::Method::POST, "/v1/packages/gate/update", None)
                .await
                .0,
            404
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_update_writes_what_the_new_version_decides_to_the_file() {
        let fx = Fx::new().await;
        fx.publish(&[fx.gate("1.0.0", ">=0.1")]);
        assert_eq!(fx.install(json!({"name": "gate"})).await.0, 200);
        let wider = fx.gate("1.1.0", ">=0.1").replace(
            "decides = [\"start_card\"]",
            "decides = [\"start_card\", \"stop_card\"]",
        );
        fx.publish(&[fx.gate("1.0.0", ">=0.1"), wider]);
        assert_eq!(
            fx.send(reqwest::Method::POST, "/v1/packages/gate/update", None)
                .await
                .0,
            200
        );
        // A restart routes the same kinds as the running server does.
        assert!(fx.state.app.decisions.decides("stop_card"));
        assert!(
            fx.config_text().contains("stop_card"),
            "{}",
            fx.config_text()
        );
        let narrower = fx
            .gate("1.2.0", ">=0.1")
            .replace("decides = [\"start_card\"]", "decides = [\"stop_card\"]");
        fx.publish(&[narrower]);
        assert_eq!(
            fx.send(reqwest::Method::POST, "/v1/packages/gate/update", None)
                .await
                .0,
            200
        );
        let text = fx.config_text();
        assert!(!text.contains("start_card"), "{text}");
        assert!(text.contains("stop_card"), "{text}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn installs_at_the_same_time_all_reach_the_lock_and_the_file() {
        let fx = Fx::new().await;
        let names: Vec<String> = (0..4).map(|n| format!("gate{n}")).collect();
        let entries: Vec<String> = names
            .iter()
            .map(|name| {
                fx.gate("1.0.0", ">=0.1")
                    .replace("name = \"gate\"", &format!("name = \"{name}\""))
                    .replace(
                        "decides = [\"start_card\"]",
                        &format!("decides = [\"{name}\"]"),
                    )
            })
            .collect();
        fx.publish(&entries);
        let answers = futures_util::future::join_all(
            names.iter().map(|name| fx.install(json!({ "name": name }))),
        )
        .await;
        assert!(
            answers.iter().all(|(status, _)| *status == 200),
            "{answers:?}"
        );
        let (lock, config) = (fx.lock(), fx.config_text());
        for name in &names {
            assert!(lock.contains(&format!("name = \"{name}\"")), "{lock}");
        }
        assert_eq!(
            config.matches("[[plugins]]").count(),
            names.len(),
            "{config}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_package_cannot_take_the_name_of_a_plugin_that_is_part_of_moebius() {
        let fx = Fx::new().await;
        let lens = fx
            .gate("1.0.0", ">=0.1")
            .replace("name = \"gate\"", "name = \"lens\"")
            .replace("decides = [\"start_card\"]", "decides = [\"lint_report\"]");
        fx.publish(&[lens]);
        // The server holds the Lens in its host, as it does when it runs.
        fx.state
            .app
            .decisions
            .host()
            .clone()
            .add(crate::harness::lens::Lens);
        let before = fx.state.app.decisions.host().manifests().len();
        let (status, refused) = fx.install(json!({"name": "lens"})).await;
        assert_eq!(status, 422, "{refused}");
        assert!(
            refused.as_str().unwrap().contains("part of Moebius"),
            "{refused}"
        );
        let host = fx.state.app.decisions.host();
        assert_eq!(host.manifests().len(), before);
        assert!(host.manifests().iter().any(|m| m.name == "lens"));
        assert!(
            host.status("lens").is_none(),
            "the compiled Lens is still there"
        );
        assert!(!fx.data_dir().join("packages").exists());
        assert!(!fx.config_text().contains("[[plugins]]"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_removal_whose_files_cannot_be_deleted_is_done_all_the_same() {
        let fx = Fx::new().await;
        fx.publish(&[fx.gate("1.0.0", ">=0.1")]);
        assert_eq!(fx.install(json!({"name": "gate"})).await.0, 200);
        // A file where the folder was cannot be deleted as a folder.
        let folder = fx.data_dir().join("packages/gate");
        std::fs::remove_dir_all(&folder).unwrap();
        std::fs::write(&folder, "in the way").unwrap();
        let (status, removed) = fx
            .send(reqwest::Method::DELETE, "/v1/packages/gate", None)
            .await;
        assert_eq!(status, 200, "{removed}");
        assert_eq!(removed["leftover"], folder.display().to_string());
        assert!(!fx.lock().contains("gate"));
        assert!(!fx.config_text().contains("[[plugins]]"));
        assert_eq!(
            fx.send(reqwest::Method::DELETE, "/v1/packages/gate", None)
                .await
                .0,
            404
        );
    }

    #[tokio::test]
    async fn a_workflow_and_rules_are_proposed_as_changes_a_person_merges() {
        let fx = Fx::new().await;
        let repo = fx
            .state
            .app
            .add_repository("demo", origin(fx.dir.path()).to_str().unwrap())
            .unwrap();
        let workflow = fx.files_package(
            "review-flow",
            "workflow",
            "Review.luau",
            "-- the review column\n",
        );
        let rules = fx.files_package(
            "careful",
            "rules",
            "rules.md",
            "Intro.\n\n## no-secrets\nNever print secrets.\n",
        );
        fx.publish(&[workflow, rules]);
        let clone = Path::new(&repo.path);

        // Without a repository named, a server with two repositories cannot tell which (here there is one more).
        fx.state
            .app
            .store
            .add_repository("other", "/srv/other", "none", "main")
            .unwrap();
        let (status, refused) = fx.install(json!({"name": "review-flow"})).await;
        assert_eq!(status, 422);
        assert!(refused.as_str().unwrap().contains("--repository"));
        assert_eq!(
            fx.install(json!({"name": "review-flow", "repository": "gone"}))
                .await
                .0,
            422
        );

        let (status, done) = fx
            .install(json!({"name": "review-flow", "repository": "demo"}))
            .await;
        assert_eq!(status, 200, "{done}");
        let change = done["change"].as_i64().unwrap();
        // The files are on a branch, offered in the inbox; the default branch has none of them until it is merged.
        let branch = "moebius/package-review-flow";
        assert_eq!(
            git(clone, &["show", &format!("{branch}:.moebius/Review.luau")]).unwrap(),
            "-- the review column"
        );
        assert!(git(clone, &["show", "main:.moebius/Review.luau"]).is_err());
        let inbox = fx.state.app.store.inbox().unwrap();
        assert!(
            inbox
                .iter()
                .any(|i| i.kind == "ready_to_merge" && i.data["change"] == change),
            "{inbox:?}"
        );
        assert!(fx.lock().contains("repository = \"demo\""));
        assert!(fx.lock().contains(&format!("change = {change}")));

        // Rules are added to the repository's own rules file, after what is there.
        let (status, done) = fx
            .install(json!({"name": "careful", "repository": "demo"}))
            .await;
        assert_eq!(status, 200, "{done}");
        assert_eq!(
            git(
                clone,
                &["show", "moebius/package-careful:.moebius/rules.md"]
            )
            .unwrap(),
            "## no-secrets\nNever print secrets."
        );
        assert!(
            fx.lock().contains("rules = [\"no-secrets\"]"),
            "{}",
            fx.lock()
        );

        // An update proposes the new files and deleting those the new version no longer has.
        fx.state
            .app
            .merge_change(&crate::decide::Actor::Person, change, "")
            .unwrap();
        let newer = fx.files_package_at(
            "review-flow",
            "1.1.0",
            "workflow",
            "Check.luau",
            "-- the check column\n",
        );
        fx.publish(&[newer]);
        let (status, updated) = fx
            .send(
                reqwest::Method::POST,
                "/v1/packages/review-flow/update",
                None,
            )
            .await;
        assert_eq!(status, 200, "{updated}");
        assert_eq!(updated["version"], "1.1.0");
        assert_eq!(
            git(clone, &["show", &format!("{branch}:.moebius/Check.luau")]).unwrap(),
            "-- the check column"
        );
        assert!(git(clone, &["show", &format!("{branch}:.moebius/Review.luau")]).is_err());
        assert!(git(clone, &["show", "main:.moebius/Review.luau"]).is_ok());
        fx.state
            .app
            .merge_change(
                &crate::decide::Actor::Person,
                updated["change"].as_i64().unwrap(),
                "",
            )
            .unwrap();
        fx.publish(&[fx.files_package_at(
            "review-flow",
            "1.1.0",
            "workflow",
            "Check.luau",
            "-- the check column\n",
        )]);

        // Removing proposes deleting the files once they are in the repository.
        let (status, removed) = fx
            .send(reqwest::Method::DELETE, "/v1/packages/review-flow", None)
            .await;
        assert_eq!(status, 200, "{removed}");
        assert!(removed["change"].as_i64().is_some());
        assert!(git(clone, &["show", &format!("{branch}:.moebius/Check.luau")]).is_err());
        assert!(git(clone, &["show", "main:.moebius/Check.luau"]).is_ok());
        assert!(!fx.lock().contains("review-flow"));
    }

    #[tokio::test]
    async fn skills_are_proposed_as_a_change_of_the_harness_configuration_repository() {
        let mut fx = Fx::new().await;
        let skill = fx.files_package(
            "tdd",
            "skills",
            "tdd/SKILL.md",
            "---\nname: tdd\ndescription: Test first.\n---\nWrite the test first.\n",
        );
        fx.publish(&[skill]);
        // Without a harness configuration repository there is nowhere to propose them.
        let (status, refused) = fx.install(json!({"name": "tdd"})).await;
        assert_eq!(status, 422, "{refused}");
        assert!(
            refused
                .as_str()
                .unwrap()
                .contains("harness configuration repository")
        );

        let origin = fx.dir.path().join("harness-origin");
        crate::harness::repo::create(&origin, &BTreeMap::new(), None).unwrap();
        let clone = fx.dir.path().join("harness-clone");
        let repo = HarnessRepo::open(
            &fx.state.app,
            origin.to_str().unwrap(),
            &clone,
            vec![],
            Arc::new(RwLock::new(AgentSet::default())),
        )
        .unwrap();
        fx.state.harness_repo = Some(Arc::new(repo));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        fx.addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        tokio::spawn(serve(listener, fx.state.clone()));

        let (status, done) = fx.install(json!({"name": "tdd"})).await;
        assert_eq!(status, 200, "{done}");
        let change = done["change"].as_i64().unwrap();
        assert_eq!(
            git(&clone, &["show", "moebius/package-tdd:skills/tdd/SKILL.md"])
                .unwrap()
                .lines()
                .last(),
            Some("Write the test first.")
        );
        let default = fx
            .state
            .harness_repo
            .as_ref()
            .unwrap()
            .default_branch()
            .unwrap();
        assert!(git(&clone, &["show", &format!("{default}:skills/tdd/SKILL.md")]).is_err());
        let change = fx.state.app.store.change(change).unwrap().unwrap();
        assert_eq!(
            (change.repository_id, change.branch.as_str()),
            (None, "moebius/package-tdd")
        );
        assert!(
            fx.state
                .app
                .store
                .inbox()
                .unwrap()
                .iter()
                .any(|i| i.kind == "ready_to_merge")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_nightly_check_puts_one_update_in_the_inbox_for_each_version() {
        let fx = Fx::new().await;
        fx.publish(&[fx.gate("1.0.0", ">=0.1")]);
        assert_eq!(fx.install(json!({"name": "gate"})).await.0, 200);
        let packages = Packages {
            app: &fx.state.app,
            live: &fx.state.config,
            harness: None,
            moebius: crate::VERSION,
        };
        let mut events = fx.state.app.bus.subscribe();
        assert!(packages.check_updates().unwrap().is_empty());

        // A newer version that needs a newer Moebius is not offered; one that fits is.
        fx.publish(&[
            fx.gate("1.0.0", ">=0.1"),
            fx.gate("1.1.0", ">=0.1"),
            fx.gate("3.0.0", ">=99"),
        ]);
        let found = packages.check_updates().unwrap();
        assert_eq!(found, [("gate".to_string(), "1.1.0".to_string())]);
        let open = fx.state.app.store.inbox().unwrap();
        let items: Vec<_> = open.iter().filter(|i| i.kind == "package_update").collect();
        assert_eq!(items.len(), 1, "{open:?}");
        assert_eq!(items[0].card_id, None);
        assert_eq!(
            items[0].data,
            json!({"name": "gate", "installed": "1.0.0", "version": "1.1.0"})
        );
        assert_eq!(fx.kinds(&mut events), ["package_update_available"]);
        // The next night finds the same and adds nothing.
        assert!(packages.check_updates().unwrap().is_empty());
        // A still newer version replaces the offer: there is one item for the package.
        fx.publish(&[fx.gate("1.1.0", ">=0.1"), fx.gate("1.2.0", ">=0.1")]);
        assert_eq!(
            packages.check_updates().unwrap(),
            [("gate".to_string(), "1.2.0".to_string())]
        );
        let open = fx.state.app.store.inbox().unwrap();
        let items: Vec<_> = open.iter().filter(|i| i.kind == "package_update").collect();
        assert_eq!(items.len(), 1, "{open:?}");
        assert_eq!(items[0].data["version"], "1.2.0");
        fx.kinds(&mut events);
        let (_, installed) = fx.get("/v1/packages").await;
        assert_eq!(installed[0]["update_available"], "1.2.0");

        // Updating resolves the item.
        let (status, _) = fx
            .send(reqwest::Method::POST, "/v1/packages/gate/update", None)
            .await;
        assert_eq!(status, 200);
        assert!(
            fx.state
                .app
                .store
                .inbox()
                .unwrap()
                .iter()
                .all(|i| i.kind != "package_update")
        );
        let (_, installed) = fx.get("/v1/packages").await;
        assert_eq!(installed[0]["update_available"], Value::Null);
        fx.kinds(&mut events);

        // A registry that cannot be read is reported on the event stream, and nothing else changes.
        std::fs::remove_dir_all(fx.data_dir().join("registries")).unwrap();
        std::fs::remove_dir_all(&fx.registry).unwrap();
        assert!(packages.check_updates().unwrap().is_empty());
        assert_eq!(fx.kinds(&mut events), ["package_check_error"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn installing_updating_and_removing_a_plugin_are_for_a_person_only() {
        let fx = Fx::new().await;
        let flow = fx.files_package("review-flow", "workflow", "Review.luau", "-- review\n");
        fx.publish(&[fx.gate("1.0.0", ">=0.1"), flow]);
        let token = fx.agent_token();
        let as_agent = |method, path: &'static str, body| fx.send_as(&token, method, path, body);

        let (status, refused) = as_agent(
            reqwest::Method::POST,
            "/v1/packages",
            Some(json!({"name": "gate"})),
        )
        .await;
        assert_eq!(status, 403, "{refused}");
        assert!(!fx.data_dir().join("packages").exists());
        assert!(!fx.config_text().contains("[[plugins]]"));

        assert_eq!(fx.install(json!({"name": "gate"})).await.0, 200);
        for (method, path) in [
            (reqwest::Method::POST, "/v1/packages/gate/update"),
            (reqwest::Method::DELETE, "/v1/packages/gate"),
        ] {
            assert_eq!(as_agent(method, path, None).await.0, 403, "{path}");
        }
        assert!(fx.data_dir().join("packages/gate/1.0.0/gate").is_file());

        // The Moebius agent can look, and can propose files, which a person merges.
        assert_eq!(
            as_agent(reqwest::Method::GET, "/v1/packages/search?q=gate", None)
                .await
                .0,
            200
        );
        assert_eq!(
            as_agent(reqwest::Method::GET, "/v1/packages", None).await.0,
            200
        );
        fx.state
            .app
            .add_repository("demo", origin(fx.dir.path()).to_str().unwrap())
            .unwrap();
        let (status, done) = as_agent(
            reqwest::Method::POST,
            "/v1/packages",
            Some(json!({"name": "review-flow", "repository": "demo"})),
        )
        .await;
        assert_eq!(status, 200, "{done}");
        // Without a token nothing is answered.
        let none = reqwest::Client::new()
            .get(fx.url("/v1/packages"))
            .send()
            .await
            .unwrap();
        assert_eq!(none.status(), 401);
    }
}
