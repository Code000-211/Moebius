//! The server put together (spec 2): the store and the core, the desktop API, the router, the Pi-Bolt harness, the
//! board engine, which runs every card's column script on a fixed tick, and the poll of GitHub's issues.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use moebius_plugin_api::Host;
use serde_json::json;
use tokio::net::TcpListener;

use crate::api::{self, ApiState};
use crate::app::{App, columns};
use crate::board::{Engine, Limits, Workflow};
use crate::bus::AppEvent;
use crate::config::Config;
use crate::github::GitHub;
use crate::pibolt::PiBolt;
use crate::repos::Worktree;
use crate::router::{Router, Upstream};
use crate::store::{Repository, Store};

/// How long a call into a plugin may take before it counts as refused.
const PLUGIN_TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Runs the last server left unended died with it: they are failed, so the board starts them again, and their
/// worktrees removed.
fn end_runs_of_a_previous_server(app: &App) -> Result<(), String> {
    let runs = app
        .store
        .fail_unended_runs("the server stopped while the run was going")
        .map_err(|e| format!("cannot end the runs of the last start: {e}"))?;
    for run in runs {
        let (Some(worktree), Some(card_id)) = (run.worktree, run.card_id) else {
            continue;
        };
        let repository = app
            .card(card_id)
            .ok()
            .and_then(|card| app.store.repository(card.repository_id).ok().flatten());
        if let Some(repository) = repository {
            let worktree = Worktree {
                path: worktree.into(),
                branch: format!("moebius/run-{}", run.id),
            };
            let _ = app
                .repos
                .remove_worktree(Path::new(&repository.path), &worktree);
        }
    }
    Ok(())
}

pub struct Server {
    pub config: Config,
    pub app: Arc<App>,
    pub router: Arc<Router>,
    pub harness: Arc<PiBolt>,
    pub engine: Arc<Engine>,
    /// The client for the GitHub App, if one is configured.
    pub github: Option<Arc<GitHub>>,
    pub token: Arc<str>,
}

impl Server {
    /// Opens the store, reads the app's token and sets up every part. The router must already be bound to
    /// `router_addr`, since each run is told where to find it.
    pub fn new(config: Config, router_addr: SocketAddr) -> Result<Self, String> {
        std::fs::create_dir_all(&config.data_dir)
            .map_err(|e| format!("cannot create {}: {e}", config.data_dir.display()))?;
        let store = Arc::new(
            Store::open(&config.data_dir).map_err(|e| format!("cannot open the store: {e}"))?,
        );
        let token = api::load_or_create_token(&config.token_file)
            .map_err(|e| format!("cannot read {}: {e}", config.token_file.display()))?;
        let app = Arc::new(App::new(
            store.clone(),
            Arc::new(Host::new(PLUGIN_TIME_LIMIT)),
            &config.data_dir,
        ));
        end_runs_of_a_previous_server(&app)?;

        let mut upstreams = Vec::new();
        for upstream in &config.upstreams {
            let api_key = match &upstream.api_key_env {
                Some(var) => Some(
                    std::env::var(var)
                        .map_err(|_| format!("the environment variable {var} is not set"))?,
                ),
                None => None,
            };
            upstreams.push(Upstream {
                protocol: upstream.protocol,
                base_url: upstream.base_url.clone(),
                api_key,
            });
        }
        let router = Arc::new(Router::new(store, upstreams));

        let harness = Arc::new(PiBolt::new(
            config.pi_bolt.clone(),
            format!("http://{router_addr}"),
            router.clone(),
            config.agents.clone(),
        ));
        let engine = Arc::new(Engine::new(
            app.clone(),
            harness.clone(),
            Workflow::default(),
            Limits {
                run_limit: config.run_limit,
                round_limit: config.round_limit,
                ..Limits::default()
            },
        ));
        let github = match &config.github {
            Some(g) => {
                let key = std::fs::read(&g.private_key_file)
                    .map_err(|e| format!("cannot read {}: {e}", g.private_key_file.display()))?;
                Some(Arc::new(
                    GitHub::new(&g.api_url, g.app_id, &key).map_err(|e| e.to_string())?,
                ))
            }
            None => None,
        };
        Ok(Self {
            config,
            app,
            router,
            harness,
            engine,
            github,
            token: token.into(),
        })
    }

    /// Runs every card's column script once; a script that fails is reported on the bus and the card stays put.
    pub fn tick(&self) {
        let Ok(repositories) = self.app.store.repositories() else {
            return;
        };
        for repository in repositories {
            let Ok(cards) = self.app.store.cards(repository.id) else {
                continue;
            };
            for card in cards {
                if card.column == columns::DONE
                    && self
                        .app
                        .store
                        .judgements_for_card(card.id)
                        .is_ok_and(|js| js.iter().all(|j| j.outcome.is_some()))
                {
                    continue;
                }
                if let Err(e) = self.engine.settle(card.id) {
                    self.app.bus.publish(AppEvent::new(
                        "board_error",
                        json!({ "card": card.id, "message": e.to_string() }),
                    ));
                }
            }
        }
    }

    /// Polls the issues of every GitHub repository once; a repository that fails is reported on the bus and tried
    /// again at the next poll.
    pub async fn poll_github(&self) {
        let Some(github) = &self.github else { return };
        let Ok(repositories) = self.app.store.repositories() else {
            return;
        };
        for repository in repositories {
            let Some(name) = repository.github.clone() else {
                continue;
            };
            if let Err(message) = self.poll_issues(github, repository, &name).await {
                self.app.bus.publish(AppEvent::new(
                    "forge_error",
                    json!({ "repository": name, "message": message }),
                ));
            }
        }
    }

    /// Lists the issues of `repository` on GitHub as `name` and syncs them into the board, unless nothing changed.
    async fn poll_issues(
        &self,
        github: &GitHub,
        repository: Repository,
        name: &str,
    ) -> Result<(), String> {
        let Some(issues) = github.issues(name).await.map_err(|e| e.to_string())? else {
            return Ok(());
        };
        let app = self.app.clone();
        let synced = tokio::task::spawn_blocking(move || app.sync_issues(&repository, &issues))
            .await
            .map_err(|e| e.to_string())?;
        // GitHub would answer "not modified" next time; the issues are listed again so the sync is retried.
        synced
            .inspect_err(|_| github.forget_issues_etag(name))
            .map_err(|e| e.to_string())
    }

    /// Serves the desktop API and the router, ticks the board and polls GitHub, until the future is dropped.
    pub async fn serve(
        self: Arc<Self>,
        api_listener: TcpListener,
        router_listener: TcpListener,
    ) -> std::io::Result<()> {
        let state = ApiState {
            app: self.app.clone(),
            token: self.token.clone(),
        };
        let board = {
            let server = self.clone();
            async move {
                let mut interval = tokio::time::interval(server.config.board_tick);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    interval.tick().await;
                    let s = server.clone();
                    let _ = tokio::task::spawn_blocking(move || s.tick()).await;
                }
            }
        };
        let poll = {
            let server = self.clone();
            async move {
                let Some(github) = &server.config.github else {
                    return std::future::pending().await;
                };
                let mut interval = tokio::time::interval(github.poll);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    interval.tick().await;
                    server.poll_github().await;
                }
            }
        };
        tokio::select! {
            served = api::serve(api_listener, state) => served,
            served = self.router.clone().serve(router_listener) => served,
            () = board => Ok(()),
            () = poll => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use serde_json::Value;

    use super::*;
    use crate::board::Runs;
    use crate::repos::git;
    use crate::store::{Card, Run};

    #[test]
    fn runs_left_going_by_a_previous_server_are_failed_and_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::parse("", dir.path(), &dir.path().join("moebius.toml")).unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        git(
            &origin,
            &["commit", "--quiet", "--allow-empty", "-m", "first"],
        )
        .unwrap();

        // A server that dies while a run is going.
        let first = Server::new(config.clone(), "127.0.0.1:1".parse().unwrap()).unwrap();
        let repo = first
            .app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        let card = first
            .app
            .create_issue(repo.id, "Card", "", crate::app::PERSON)
            .unwrap();
        let run = first
            .app
            .store
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        let worktree = first
            .app
            .repos
            .add_worktree("demo", Path::new(&repo.path), run.id, "main")
            .unwrap();
        first
            .app
            .store
            .set_run_worktree(run.id, &worktree.path.to_string_lossy())
            .unwrap();
        first.app.store.set_run_status(run.id, "running").unwrap();
        drop(first);

        let second = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let run = second.app.store.run(run.id).unwrap().unwrap();
        assert_eq!(run.status, "failed");
        assert!(second.app.store.active_runs().unwrap().is_empty());
        assert!(!worktree.path.exists());
    }

    /// A GitHub with one repository, `acme/widgets`: its issues, the ETag they are served with and the
    /// `If-None-Match` of every request for them.
    struct FakeGitHub {
        issues: Mutex<(String, Vec<Value>)>,
        asked: Mutex<Vec<Option<String>>>,
    }

    impl FakeGitHub {
        fn set_issues(&self, etag: &str, issues: Vec<Value>) {
            *self.issues.lock().unwrap() = (etag.to_string(), issues);
        }

        fn asked(&self) -> Vec<Option<String>> {
            self.asked.lock().unwrap().clone()
        }
    }

    fn issue(number: u64, state: &str, login: &str, body: &str) -> Value {
        json!({ "number": number, "title": format!("Issue {number}"), "body": body,
                "state": state, "user": { "login": login } })
    }

    async fn fake_github() -> (String, Arc<FakeGitHub>) {
        let fake = Arc::new(FakeGitHub {
            issues: Mutex::new((String::new(), Vec::new())),
            asked: Mutex::new(Vec::new()),
        });
        let app = axum::Router::new()
            .route(
                "/repos/acme/widgets/installation",
                get(|| async { axum::Json(json!({ "id": 77 })) }),
            )
            .route(
                "/app/installations/77/access_tokens",
                post(|| async {
                    let expires =
                        time::OffsetDateTime::now_utc() + std::time::Duration::from_secs(3600);
                    (
                        StatusCode::CREATED,
                        axum::Json(json!({
                            "token": "ghs_token",
                            "expires_at": expires
                                .format(&time::format_description::well_known::Rfc3339)
                                .unwrap(),
                        })),
                    )
                }),
            )
            .route(
                "/repos/acme/widgets/issues",
                get(
                    |State(fake): State<Arc<FakeGitHub>>, headers: HeaderMap| async move {
                        let asked = headers
                            .get("if-none-match")
                            .map(|v| v.to_str().unwrap().to_string());
                        fake.asked.lock().unwrap().push(asked.clone());
                        let (etag, issues) = fake.issues.lock().unwrap().clone();
                        if asked.as_deref() == Some(etag.as_str()) {
                            return StatusCode::NOT_MODIFIED.into_response();
                        }
                        ([("etag", etag)], axum::Json(issues)).into_response()
                    },
                ),
            )
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        (url, fake)
    }

    /// Starts a run that never does anything.
    struct IdleRuns;

    impl Runs for IdleRuns {
        fn start(
            &self,
            app: &Arc<App>,
            card: &Card,
            agent: &str,
            purpose: &str,
        ) -> Result<Run, String> {
            app.store
                .add_run(Some(card.id), agent, "idle", purpose)
                .map_err(|e| e.to_string())
        }
    }

    /// A server polling the fake GitHub for the repository `acme/widgets`, whose trusted author is `leon`.
    struct Fixture {
        _dir: tempfile::TempDir,
        server: Server,
        fake: Arc<FakeGitHub>,
        engine: Engine,
        repository: Repository,
    }

    impl Fixture {
        async fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let (url, fake) = fake_github().await;
            let key =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/github-app-test-key.pem");
            let text = format!(
                "[github]\napp_id = 4242\nprivate_key_file = {:?}\napi_url = {url:?}\n",
                key.display().to_string()
            );
            let config =
                Config::parse(&text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
            let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
            let origin = dir.path().join("origin");
            std::fs::create_dir_all(&origin).unwrap();
            git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
            git(
                &origin,
                &["commit", "--quiet", "--allow-empty", "-m", "first"],
            )
            .unwrap();
            let repository = server
                .app
                .add_github_repository(
                    "widgets",
                    origin.to_str().unwrap(),
                    "acme/widgets",
                    &["leon".to_string()],
                )
                .unwrap();
            let engine = Engine::new(
                server.app.clone(),
                Arc::new(IdleRuns),
                crate::board::Workflow::default(),
                crate::board::Limits::default(),
            );
            Self {
                _dir: dir,
                server,
                fake,
                engine,
                repository,
            }
        }

        fn card(&self, number: i64) -> Option<Card> {
            self.server
                .app
                .store
                .card_by_number(self.repository.id, number)
                .unwrap()
        }
    }

    #[tokio::test]
    async fn the_server_needs_the_key_of_the_github_app() {
        let dir = tempfile::tempdir().unwrap();
        let text = "[github]\napp_id = 1\nprivate_key_file = \"missing.pem\"\n";
        let config = Config::parse(text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
        let error = Server::new(config, "127.0.0.1:1".parse().unwrap())
            .err()
            .unwrap();
        assert!(error.contains("missing.pem"), "{error}");
    }

    #[tokio::test]
    async fn issues_become_cards_and_an_unchanged_list_changes_nothing() {
        let f = Fixture::new().await;
        f.fake.set_issues(
            "\"v1\"",
            vec![
                issue(1, "open", "leon", "Do it."),
                issue(2, "open", "mallory", "Do it my way."),
                issue(3, "closed", "leon", "Long done."),
            ],
        );
        f.server.poll_github().await;

        assert!(f.card(3).is_none(), "a closed issue gets no card");
        let (trusted, outside) = (f.card(1).unwrap(), f.card(2).unwrap());
        assert_eq!(
            (
                trusted.title.as_str(),
                trusted.body.as_str(),
                trusted.author.as_str()
            ),
            ("Issue 1", "Do it.", "leon")
        );
        f.engine.settle(trusted.id).unwrap();
        f.engine.settle(outside.id).unwrap();
        let app = &f.server.app;
        assert_eq!(app.card(trusted.id).unwrap().column, "In progress");
        assert_eq!(app.card(outside.id).unwrap().column, "Backlog");
        assert!(
            app.store
                .has_open_inbox_item(outside.id, "approve_issue")
                .unwrap()
        );

        let mut events = app.bus.subscribe();
        f.server.poll_github().await;
        assert_eq!(f.fake.asked(), [None, Some("\"v1\"".to_string())]);
        assert!(events.try_recv().is_err(), "a 304 changes nothing");
        assert_eq!(app.store.cards(f.repository.id).unwrap().len(), 2);
        assert_eq!(app.card(trusted.id).unwrap().column, "In progress");
    }

    #[tokio::test]
    async fn a_closed_issue_moves_its_card_to_done_and_a_new_title_is_copied() {
        let f = Fixture::new().await;
        f.fake.set_issues(
            "\"v1\"",
            vec![
                issue(1, "open", "leon", ""),
                issue(2, "open", "mallory", ""),
            ],
        );
        f.server.poll_github().await;
        let id = f.card(1).unwrap().id;
        f.engine.settle(id).unwrap();

        let mut retitled = issue(2, "open", "mallory", "");
        retitled["title"] = json!("Better title");
        f.fake
            .set_issues("\"v2\"", vec![issue(1, "closed", "leon", ""), retitled]);
        f.server.poll_github().await;

        assert_eq!(f.fake.asked().last().unwrap().as_deref(), Some("\"v1\""));
        assert_eq!(f.card(1).unwrap().column, "Done");
        let outside = f.card(2).unwrap();
        assert_eq!(
            (outside.title.as_str(), outside.column.as_str()),
            ("Better title", "Backlog")
        );
    }

    #[tokio::test]
    async fn blocked_by_in_a_body_becomes_a_dependency() {
        let f = Fixture::new().await;
        f.fake.set_issues(
            "\"v1\"",
            vec![
                issue(5, "open", "leon", "Blocked by: #3, #4"),
                issue(6, "open", "leon", "Nothing in the way."),
            ],
        );
        f.server.poll_github().await;
        let store = &f.server.app.store;
        assert_eq!(store.dependencies(f.card(5).unwrap().id).unwrap(), [3, 4]);
        assert!(
            store
                .dependencies(f.card(6).unwrap().id)
                .unwrap()
                .is_empty()
        );

        f.fake
            .set_issues("\"v2\"", vec![issue(5, "open", "leon", "blocked by #4")]);
        f.server.poll_github().await;
        assert_eq!(store.dependencies(f.card(5).unwrap().id).unwrap(), [4]);
    }

    #[tokio::test]
    async fn a_repository_that_cannot_be_polled_is_reported_and_the_others_still_are() {
        let f = Fixture::new().await;
        let app = &f.server.app;
        let clone = &f.repository.path;
        app.store
            .add_github_repository("missing", clone, "main", "acme/missing", &[])
            .unwrap();
        f.fake
            .set_issues("\"v1\"", vec![issue(1, "open", "leon", "")]);
        let mut events = app.bus.subscribe();
        f.server.poll_github().await;

        assert!(f.card(1).is_some());
        assert_eq!(events.try_recv().unwrap().kind, "card_added");
        let error = events.try_recv().unwrap();
        assert_eq!(error.kind, "forge_error");
        assert_eq!(error.data["repository"], "acme/missing");
    }
}
