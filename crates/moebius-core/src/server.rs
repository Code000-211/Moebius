//! The server put together (spec 2): the store and the core, the desktop API, the router, the Pi-Bolt harness, the
//! board engine, which runs every card's column script on a fixed tick, the poll of GitHub and, every 30 seconds,
//! the refresh of the clones of repositories without a forge and of the harness configuration repository, and, once
//! a minute, the check whether the Retro is due, a proposal of the Retro is to be tried or a research is to start. The
//! `moebius` skill is written to the data directory at start.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use moebius_plugin_api::Host;
use serde_json::json;
use tokio::net::TcpListener;

use crate::api::{self, ApiState};
use crate::app::{App, columns};
use crate::board::{Engine, Limits, Runs, Workflow};
use crate::bus::AppEvent;
use crate::check;
use crate::config::Config;
use crate::forge::GitHubForge;
use crate::github::GitHub;
use crate::guard::{self, Guard, Teacher};
use crate::harness_repo::HarnessRepo;
use crate::pibolt::PiBolt;
use crate::poll;
use crate::replay;
use crate::repos::git;
use crate::research;
use crate::retro;
use crate::router::{Router, Upstream};
use crate::skill;
use crate::store::Store;
use crate::trial;

/// How long a call into a plugin may take before it counts as refused.
/// How often the clones of repositories without a forge are brought up to date with where they were cloned from.
const PLAIN_REFRESH: std::time::Duration = std::time::Duration::from_secs(30);

/// How often the schedule looks at the clock to see whether the Retro is due or trials may start.
const RETRO_CHECK: std::time::Duration = std::time::Duration::from_secs(60);

const PLUGIN_TIME_LIMIT: std::time::Duration =
    std::time::Duration::from_secs(crate::config::PLUGIN_TIME_LIMIT_SECONDS);

/// Runs the last server left unended died with it: they are failed, so the board starts them again, and their
/// worktrees removed.
fn end_runs_of_a_previous_server(app: &App) -> Result<(), String> {
    let runs = app
        .store
        .fail_unended_runs("the server stopped while the run was going")
        .map_err(|e| format!("cannot end the runs of the last start: {e}"))?;
    app.store
        .fail_analysing_retros("the server stopped while the analyst was going")
        .map_err(|e| format!("cannot end the Retros of the last start: {e}"))?;
    app.store
        .reconfirm_running_research()
        .map_err(|e| format!("cannot confirm the research of the last start again: {e}"))?;
    app.store
        .reopen_trying_proposals()
        .map_err(|e| format!("cannot reopen the proposals of the last start: {e}"))?;
    for run in runs {
        let Some(worktree) = run.worktree else {
            continue;
        };
        let repository_id = match run.card_id {
            Some(card_id) => app.card(card_id).ok().map(|card| card.repository_id),
            None => run.repository_id,
        };
        let repository = repository_id.and_then(|id| app.store.repository(id).ok().flatten());
        match repository {
            Some(repository) => {
                let _ = app
                    .repos
                    .remove_worktree(Path::new(&repository.path), Path::new(&worktree));
                if run.purpose == "trial" {
                    // The trial also had its branch, and a worktree of the proposal's branch in the harness clone.
                    let name = Path::new(&worktree).file_name().unwrap_or_default();
                    let _ = git(
                        Path::new(&repository.path),
                        &[
                            "branch",
                            "-D",
                            "--quiet",
                            &format!("moebius/{}", name.to_string_lossy()),
                        ],
                    );
                    let proposal = app.data_dir.join("worktrees").join("harness").join(name);
                    let _ = app
                        .repos
                        .remove_worktree(&app.data_dir.join("harness"), &proposal);
                }
            }
            // The Retro analyst worked in a worktree of the harness configuration clone.
            None if run.purpose == "retro" => {
                let _ = app
                    .repos
                    .remove_worktree(&app.data_dir.join("harness"), Path::new(&worktree));
            }
            // A chat without a repository worked in an empty directory.
            None if run.purpose == "chat" => {
                let _ = std::fs::remove_dir_all(&worktree);
            }
            None => {}
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
    /// The harness configuration repository, if one is configured.
    pub harness_repo: Option<Arc<HarnessRepo>>,
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
        skill::write(&config.data_dir)
            .map_err(|e| format!("cannot write the moebius skill: {e}"))?;
        let store = Arc::new(
            Store::open(&config.data_dir).map_err(|e| format!("cannot open the store: {e}"))?,
        );
        let token = api::load_or_create_token(&config.token_file)
            .map_err(|e| format!("cannot read {}: {e}", config.token_file.display()))?;
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
        let router =
            Arc::new(Router::new(store.clone(), upstreams).with_day_budget(config.day_tokens));

        let teacher = config
            .teacher
            .as_ref()
            .map(|teacher| Teacher::new(teacher, router.clone(), format!("http://{router_addr}")));
        let mut host = Host::new(PLUGIN_TIME_LIMIT);
        host.add(Guard::new(teacher));
        let mut app = App::new(store.clone(), Arc::new(host), &config.data_dir);
        app.checks_timeout = config.checks_timeout;
        app.decisions.route("tool_call", guard::NAME);
        let github = match &config.github {
            Some(g) => {
                let key = std::fs::read(&g.private_key_file)
                    .map_err(|e| format!("cannot read {}: {e}", g.private_key_file.display()))?;
                let github =
                    Arc::new(GitHub::new(&g.api_url, g.app_id, &key).map_err(|e| e.to_string())?);
                app.add_forge(Box::new(GitHubForge::new(
                    github.clone(),
                    app.repos.clone(),
                    tokio::runtime::Handle::current(),
                )));
                Some(github)
            }
            None => None,
        };
        let app = Arc::new(app);
        end_runs_of_a_previous_server(&app)?;

        let harness = Arc::new(PiBolt::new(
            config.pi_bolt.clone(),
            config.moebius.clone(),
            format!("http://{router_addr}"),
            router.clone(),
            config.agents.clone(),
        ));
        let harness_repo = match &config.harness_url {
            Some(url) => Some(Arc::new(HarnessRepo::open(
                &app,
                url,
                &config.harness_dir(),
                config.mcp_env.clone(),
                harness.agents.clone(),
            )?)),
            None => None,
        };
        let engine = Arc::new(Engine::new(
            app.clone(),
            harness.clone(),
            Workflow::default(),
            Limits {
                run_limit: config.run_limit,
                round_limit: config.round_limit,
                day_budget: config.day_tokens,
                ..Limits::default()
            },
        ));
        Ok(Self {
            config,
            app,
            router,
            harness,
            engine,
            harness_repo,
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
                    // Kept as the card's waiting reason until a script runs cleanly again.
                    let reason = format!("script error: {e}");
                    self.app.set_waiting(card.id, Some(reason));
                    self.app.bus.publish(AppEvent::new(
                        "board_error",
                        json!({ "card": card.id, "message": e.to_string() }),
                    ));
                }
            }
        }
    }

    /// Polls every GitHub repository once (see `poll`).
    pub async fn poll_github(&self) {
        if let Some(github) = &self.github {
            poll::poll_github(&self.app, github).await;
        }
    }

    /// Refreshes the clone of every repository without a forge from where it was cloned; a failure is reported on the
    /// bus. The GitHub repositories are refreshed by their poll.
    pub fn refresh_plain_repositories(&self) {
        let Ok(repositories) = self.app.store.repositories() else {
            return;
        };
        for repository in repositories.iter().filter(|r| r.forge == "none") {
            if let Err(e) = self.app.refresh(repository) {
                self.app.bus.publish(AppEvent::new(
                    "forge_error",
                    json!({ "repository": repository.name, "message": e.to_string() }),
                ));
            }
        }
    }

    /// Refreshes the harness configuration repository, if there is one; a failure is reported on the bus.
    pub fn refresh_harness_repository(&self) {
        if let Some(harness_repo) = &self.harness_repo
            && let Err(e) = harness_repo.refresh(&self.app)
        {
            self.app.bus.publish(AppEvent::new(
                "harness_error",
                json!({ "message": e.to_string() }),
            ));
        }
    }

    /// Serves the desktop API and the router, ticks the board and polls GitHub, until the future is dropped.
    pub async fn serve(
        self: Arc<Self>,
        api_listener: TcpListener,
        router_listener: TcpListener,
    ) -> std::io::Result<()> {
        let state = ApiState {
            app: self.app.clone(),
            runs: self.harness.clone(),
            harness_repo: self.harness_repo.clone(),
            token: self.token.clone(),
            round_limit: self.config.round_limit,
            day_budget: self.config.day_tokens,
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
        let refresh = {
            let server = self.clone();
            async move {
                let mut interval = tokio::time::interval(PLAIN_REFRESH);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    interval.tick().await;
                    let s = server.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        s.refresh_plain_repositories();
                        s.refresh_harness_repository();
                    })
                    .await;
                }
            }
        };
        let retro = {
            let server = self.clone();
            async move {
                let mut interval = tokio::time::interval(RETRO_CHECK);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    interval.tick().await;
                    let s = server.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        let runs: Arc<dyn Runs> = s.harness.clone();
                        let repo = s.harness_repo.as_ref();
                        let report = |result: crate::app::Result<()>| {
                            if let Err(e) = result {
                                s.app.bus.publish(AppEvent::new(
                                    "retro_error",
                                    json!({ "message": e.to_string() }),
                                ));
                            }
                        };
                        report(retro::on_schedule(&s.app, &runs, repo, s.config.retro_at));
                        let (window, limit) = (s.config.trial_window, s.config.run_limit);
                        report(trial::on_schedule(&s.app, &runs, repo, window, limit));
                        report(research::on_schedule(
                            &s.app,
                            &runs,
                            window,
                            research::MEASURE_LIMIT,
                        ));
                        report(replay::on_schedule(&s.app, window));
                    })
                    .await;
                }
            }
        };
        let router_routes = self.router.clone().routes().merge(check::routes(
            self.app.clone(),
            self.router.clone(),
            self.config.rules_path(),
        ));
        tokio::select! {
            served = api::serve(api_listener, state) => served,
            served = axum::serve(router_listener, router_routes) => served,
            () = board => Ok(()),
            () = poll => Ok(()),
            () = refresh => Ok(()),
            () = retro => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::extract::{Path as UrlPath, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use serde_json::Value;

    use super::*;
    use crate::board::{Runs, Step};
    use crate::forge::Forge;
    use crate::store::{Card, Change, Repository, Run};

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
        let chat = first
            .app
            .store
            .add_run(None, "Coder", "pi-bolt", "chat")
            .unwrap();
        let empty = first.app.data_dir.join(format!("chats/{}", chat.id));
        std::fs::create_dir_all(&empty).unwrap();
        first
            .app
            .store
            .set_run_worktree(chat.id, &empty.to_string_lossy())
            .unwrap();
        first.app.store.set_run_status(chat.id, "waiting").unwrap();
        drop(first);

        let second = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let run = second.app.store.run(run.id).unwrap().unwrap();
        assert_eq!(run.status, "failed");
        assert!(second.app.store.active_runs().unwrap().is_empty());
        assert!(!worktree.path.exists());
        assert!(!empty.exists());
        assert_eq!(
            second.app.store.run(chat.id).unwrap().unwrap().status,
            "failed"
        );
    }

    #[test]
    fn the_moebius_skill_is_written_at_start_from_the_guide_and_the_reference() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::parse("", dir.path(), &dir.path().join("moebius.toml")).unwrap();
        Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let skill = dir.path().join(".moebius/skills/moebius");
        let skill_md = std::fs::read_to_string(skill.join("SKILL.md")).unwrap();
        assert!(skill_md.starts_with("---\nname: moebius\n"));
        let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        for directory in ["guide", "reference"] {
            for page in std::fs::read_dir(docs.join(directory)).unwrap() {
                let name = page.unwrap().file_name();
                let written = std::fs::read(skill.join(directory).join(&name)).unwrap();
                assert_eq!(
                    written,
                    std::fs::read(docs.join(directory).join(&name)).unwrap()
                );
            }
        }
    }

    /// A GitHub with one repository, `acme/widgets`: its issues, the ETag they are served with and the
    /// `If-None-Match` of every request for them.
    struct FakeGitHub {
        issues: Mutex<(String, Vec<Value>)>,
        asked: Mutex<Vec<Option<String>>>,
        pulls: Mutex<Pulls>,
    }

    /// The pull requests of the fake: what it was asked to open and merge, and what it answers about pull request 7.
    struct Pulls {
        opened: Vec<Value>,
        merged: Vec<Value>,
        pull: Value,
        check_runs: Vec<Value>,
        reviews: Vec<Value>,
    }

    impl FakeGitHub {
        fn set_issues(&self, etag: &str, issues: Vec<Value>) {
            *self.issues.lock().unwrap() = (etag.to_string(), issues);
        }

        fn asked(&self) -> Vec<Option<String>> {
            self.asked.lock().unwrap().clone()
        }

        /// Makes pull request 7 look as `state` (`open` or `closed`) and `merged` say.
        fn set_pull(&self, state: &str, merged: bool) {
            self.pulls.lock().unwrap().pull = pull(state, merged);
        }

        /// Makes the head of pull request 7 the commit `sha`, as a push does.
        fn set_head(&self, sha: &str) {
            self.pulls.lock().unwrap().pull["head"]["sha"] = json!(sha);
        }
    }

    fn pull(state: &str, merged: bool) -> Value {
        json!({ "number": 7, "state": state, "merged": merged,
                "merge_commit_sha": merged.then_some("c0ffee"), "head": { "sha": "deadbeef" } })
    }

    fn check(name: &str, status: &str, conclusion: &str) -> Value {
        json!({ "name": name, "status": status, "conclusion": conclusion })
    }

    fn issue(number: u64, state: &str, login: &str, body: &str) -> Value {
        json!({ "number": number, "title": format!("Issue {number}"), "body": body,
                "state": state, "user": { "login": login } })
    }

    async fn fake_github() -> (String, Arc<FakeGitHub>) {
        let fake = Arc::new(FakeGitHub {
            issues: Mutex::new((String::new(), Vec::new())),
            asked: Mutex::new(Vec::new()),
            pulls: Mutex::new(Pulls {
                opened: Vec::new(),
                merged: Vec::new(),
                pull: pull("open", false),
                check_runs: Vec::new(),
                reviews: Vec::new(),
            }),
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
            .route(
                "/repos/acme/widgets/pulls",
                post(
                    |State(fake): State<Arc<FakeGitHub>>, axum::Json(body): axum::Json<Value>| async move {
                        fake.pulls.lock().unwrap().opened.push(body);
                        (StatusCode::CREATED, axum::Json(json!({ "number": 7 })))
                    },
                ),
            )
            .route(
                "/repos/acme/widgets/pulls/{number}",
                get(
                    |State(fake): State<Arc<FakeGitHub>>, UrlPath(number): UrlPath<u64>| async move {
                        assert_eq!(number, 7);
                        axum::Json(fake.pulls.lock().unwrap().pull.clone())
                    },
                ),
            )
            .route(
                "/repos/acme/widgets/commits/{sha}/check-runs",
                get(
                    |State(fake): State<Arc<FakeGitHub>>, UrlPath(sha): UrlPath<String>| async move {
                        let pulls = fake.pulls.lock().unwrap();
                        assert_eq!(
                            pulls.pull["head"]["sha"], sha,
                            "the checks of the pull request's head"
                        );
                        let runs = pulls.check_runs.clone();
                        axum::Json(json!({ "total_count": runs.len(), "check_runs": runs }))
                    },
                ),
            )
            .route(
                "/repos/acme/widgets/pulls/{number}/reviews",
                get(|State(fake): State<Arc<FakeGitHub>>| async move {
                    axum::Json(fake.pulls.lock().unwrap().reviews.clone())
                }),
            )
            .route(
                "/repos/acme/widgets/pulls/{number}/merge",
                axum::routing::put(
                    |State(fake): State<Arc<FakeGitHub>>, axum::Json(body): axum::Json<Value>| async move {
                        fake.pulls.lock().unwrap().merged.push(body);
                        axum::Json(json!({ "sha": "c0ffee", "merged": true }))
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
            _: Option<&str>,
        ) -> Result<Run, String> {
            app.store
                .add_run(Some(card.id), agent, "idle", purpose)
                .map_err(|e| e.to_string())
        }

        fn has_agent(&self, _: &str) -> bool {
            false
        }

        fn steer(&self, _: i64, _: &str) -> bool {
            false
        }

        fn stop(&self, _: i64) -> bool {
            false
        }
    }

    /// A server polling the fake GitHub for the repository `acme/widgets`, whose trusted author is `leon`.
    struct Fixture {
        _dir: tempfile::TempDir,
        server: Server,
        fake: Arc<FakeGitHub>,
        engine: Arc<Engine>,
        repository: Repository,
        /// The bare repository the clone's `origin` is.
        remote: std::path::PathBuf,
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
            let remote = dir.path().join("remote.git");
            git(
                dir.path(),
                &["clone", "--quiet", "--bare", "origin", "remote.git"],
            )
            .unwrap();
            let repository = server
                .app
                .add_github_repository(
                    "widgets",
                    remote.to_str().unwrap(),
                    "acme/widgets",
                    &["leon".to_string()],
                )
                .unwrap();
            let engine = Arc::new(Engine::new(
                server.app.clone(),
                Arc::new(IdleRuns),
                crate::board::Workflow::default(),
                crate::board::Limits::default(),
            ));
            Self {
                _dir: dir,
                server,
                fake,
                engine,
                repository,
                remote,
            }
        }

        /// Polls issue 1 into a card, which waits in review with a succeeded `code` run that added `feature.txt`.
        async fn card_in_review(&self) -> Card {
            self.fake
                .set_issues("\"v1\"", vec![issue(1, "open", "leon", "Do it.")]);
            self.server.poll_github().await;
            let card = self.card(1).unwrap();
            self.code(&card, "feature.txt").await;
            self.server
                .app
                .store
                .move_card(card.id, "In review")
                .unwrap();
            card
        }

        /// Has a succeeded `code` run of the card add `file` on top of its work so far, as a round of the Coder does.
        async fn code(&self, card: &Card, file: &'static str) {
            let (app, repository, card) = (
                self.server.app.clone(),
                self.repository.clone(),
                card.clone(),
            );
            tokio::task::spawn_blocking(move || {
                let store = &app.store;
                let run = store
                    .add_run(Some(card.id), "Coder", "test", "code")
                    .unwrap();
                let clone = Path::new(&repository.path);
                let base = store
                    .code_branch(card.id)
                    .unwrap()
                    .unwrap_or_else(|| repository.base_ref());
                let worktree = app
                    .repos
                    .add_worktree("widgets", clone, run.id, &base)
                    .unwrap();
                std::fs::write(worktree.path.join(file), "done\n").unwrap();
                git(&worktree.path, &["add", "."]).unwrap();
                git(&worktree.path, &["commit", "--quiet", "-m", file]).unwrap();
                app.repos.remove_worktree(clone, &worktree.path).unwrap();
                store.set_run_status(run.id, "succeeded").unwrap();
            })
            .await
            .unwrap();
        }

        /// Runs the card's column script until it leaves the card, on a thread that may block, as the board does.
        async fn settle(&self, card: &Card) -> Step {
            let (engine, id) = (self.engine.clone(), card.id);
            tokio::task::spawn_blocking(move || engine.settle(id).unwrap())
                .await
                .unwrap()
        }

        /// Polls issue 1 into a card and has its work offered as a change.
        async fn offered_change(&self) -> Change {
            let card = self.card_in_review().await;
            let app = self.server.app.clone();
            tokio::task::spawn_blocking(move || app.offer_change(card.id).unwrap())
                .await
                .unwrap()
        }

        fn change(&self, id: i64) -> Change {
            self.server.app.store.change(id).unwrap().unwrap()
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
    async fn a_poll_brings_the_clone_up_to_date_so_new_work_starts_from_the_merged_default_branch()
    {
        let f = Fixture::new().await;
        f.fake.set_issues("\"v1\"", vec![]);
        // A pull request merged on GitHub: the default branch moves on without the clone.
        let elsewhere = f._dir.path().join("elsewhere");
        git(
            f._dir.path(),
            &["clone", "--quiet", f.remote.to_str().unwrap(), "elsewhere"],
        )
        .unwrap();
        git(
            &elsewhere,
            &[
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "merged on GitHub",
            ],
        )
        .unwrap();
        git(&elsewhere, &["push", "--quiet", "origin", "main"]).unwrap();
        let merged = git(&elsewhere, &["rev-parse", "HEAD"]).unwrap();

        f.server.poll_github().await;
        let clone = Path::new(&f.repository.path);
        assert_eq!(f.repository.base_ref(), "origin/main");
        assert_eq!(git(clone, &["rev-parse", "origin/main"]).unwrap(), merged);
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

    #[tokio::test(flavor = "multi_thread")]
    async fn an_offered_change_is_pushed_and_opened_as_a_pull_request() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;

        assert!(git(&f.remote, &["rev-parse", "--verify", "moebius/card-1"]).is_ok());
        assert_eq!(change.pull_request, Some(7));
        assert_eq!(f.change(change.id).pull_request, Some(7));
        let opened = f.fake.pulls.lock().unwrap().opened.clone();
        assert_eq!(opened.len(), 1);
        assert_eq!(
            (
                opened[0]["title"].as_str(),
                opened[0]["head"].as_str(),
                opened[0]["base"].as_str()
            ),
            (Some("Issue 1"), Some("moebius/card-1"), Some("main"))
        );
        assert!(
            opened[0]["body"]
                .as_str()
                .unwrap()
                .starts_with("Closes #1\n")
        );
        assert_eq!(f.card(1).unwrap().column, "Ready to merge");
        assert!(
            f.server
                .app
                .store
                .has_open_inbox_item(f.card(1).unwrap().id, "ready_to_merge")
                .unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pull_request_merged_on_github_moves_its_card_to_done() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;

        f.fake.set_pull("closed", true);
        f.server.poll_github().await;

        let change = f.change(change.id);
        assert_eq!(change.state, "merged");
        assert_eq!(f.card(1).unwrap().column, "Done");
        assert!(f.server.app.store.inbox().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pull_request_closed_without_a_merge_closes_the_change_and_tells_the_person_once() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;

        f.fake.set_pull("closed", false);
        f.server.poll_github().await;
        f.server.poll_github().await;

        assert_eq!(f.change(change.id).state, "closed");
        let inbox = f.server.app.store.inbox().unwrap();
        assert_eq!(
            inbox.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
            ["change_closed"]
        );
        assert_eq!(f.card(1).unwrap().column, "Ready to merge");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_repository_without_a_forge_gets_the_commits_of_where_it_was_cloned() {
        let f = Fixture::new().await;
        let origin = f._dir.path().join("origin");
        let plain = f
            .server
            .app
            .add_repository("plain", origin.to_str().unwrap())
            .unwrap();
        let clone = Path::new(&plain.path);
        let mut events = f.server.app.bus.subscribe();

        std::fs::write(origin.join("rules.md"), "new\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "rules"]).unwrap();
        f.server.refresh_plain_repositories();
        assert_eq!(
            std::fs::read_to_string(clone.join("rules.md")).unwrap(),
            "new\n"
        );

        // Moebius merged a change into the clone: the default branch is left, and nothing is wrong.
        git(clone, &["checkout", "--quiet", "-b", "feature"]).unwrap();
        git(
            clone,
            &["commit", "--quiet", "--allow-empty", "-m", "feature"],
        )
        .unwrap();
        git(clone, &["checkout", "--quiet", "main"]).unwrap();
        f.server.app.repos.merge(clone, "feature", "merge").unwrap();
        git(
            &origin,
            &["commit", "--quiet", "--allow-empty", "-m", "more"],
        )
        .unwrap();
        f.server.refresh_plain_repositories();
        assert!(
            git(clone, &["log", "--format=%s"])
                .unwrap()
                .contains("merge")
        );
        assert!(events.try_recv().is_err(), "no error was published");

        // A clone whose origin is gone is reported.
        std::fs::remove_dir_all(&origin).unwrap();
        f.server.refresh_plain_repositories();
        let event = events.try_recv().unwrap();
        assert_eq!(
            (event.kind.as_str(), event.data["repository"].as_str()),
            ("forge_error", Some("plain"))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_script_error_is_the_cards_waiting_reason_until_the_script_runs_cleanly() {
        let f = Fixture::new().await;
        let origin = f._dir.path().join("origin");
        let plain = f
            .server
            .app
            .add_repository("plain", origin.to_str().unwrap())
            .unwrap();
        let card = f
            .server
            .app
            .create_issue(plain.id, "Anything", "", "person")
            .unwrap();
        let clone = Path::new(&plain.path);
        let override_with = |script: &str| {
            std::fs::create_dir_all(clone.join(".moebius")).unwrap();
            std::fs::write(clone.join(".moebius/backlog.luau"), script).unwrap();
            git(clone, &["add", "."]).unwrap();
            git(clone, &["commit", "--quiet", "-m", "override"]).unwrap();
        };
        let waiting = || f.server.app.waiting(std::slice::from_ref(&card));

        override_with("function on_card(card) error('boom') end");
        f.server.tick();
        let reason = waiting().remove(&card.id).unwrap();
        assert!(reason.starts_with("script error: backlog.luau"), "{reason}");
        assert!(reason.contains("boom"), "{reason}");

        override_with("function on_card(card) end");
        f.server.tick();
        assert!(waiting().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_retry_after_a_closed_pull_request_opens_a_new_one() {
        let f = Fixture::new().await;
        let closed = f.offered_change().await;
        f.fake.set_pull("closed", false);
        f.server.poll_github().await;

        let card = f.card(1).unwrap();
        let app = f.server.app.clone();
        let retried = tokio::task::spawn_blocking(move || app.retry_card(card.id, "Try again."))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retried.column, "In progress");
        assert_eq!(
            retried.note.as_deref(),
            Some("The pull request was closed without a merge.")
        );
        assert!(f.server.app.store.inbox().unwrap().is_empty());

        f.code(&card, "second.txt").await;
        let app = f.server.app.clone();
        let change = tokio::task::spawn_blocking(move || app.publish_change(card.id).unwrap())
            .await
            .unwrap();
        assert_ne!(change.id, closed.id);
        assert_eq!(change.state, "open");
        assert_eq!(f.change(closed.id).state, "closed");
        assert_eq!(f.fake.pulls.lock().unwrap().opened.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn checks_and_the_latest_review_are_recorded_when_they_change() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;
        let run = |status: &str, conclusion: Option<&str>| {
            json!({ "name": "ci", "status": status, "conclusion": conclusion,
                    "details_url": "https://ci/1", "output": { "title": "3 tests failed" } })
        };

        {
            let mut pulls = f.fake.pulls.lock().unwrap();
            pulls.check_runs = vec![run("completed", Some("success")), run("in_progress", None)];
            pulls.reviews = vec![
                json!({ "state": "COMMENTED" }),
                json!({ "state": "APPROVED" }),
            ];
        }
        let mut events = f.server.app.bus.subscribe();
        f.server.poll_github().await;
        let stored = f.change(change.id);
        assert_eq!(
            (stored.checks.as_deref(), stored.review.as_deref()),
            (Some("pending"), Some("approved"))
        );
        assert_eq!(events.try_recv().unwrap().kind, "change_updated");

        f.fake.pulls.lock().unwrap().check_runs = vec![
            run("completed", Some("success")),
            run("completed", Some("failure")),
        ];
        f.server.poll_github().await;
        let stored = f.change(change.id);
        assert_eq!(stored.checks.as_deref(), Some("failure"));
        assert_eq!(stored.checks_head.as_deref(), Some("deadbeef"));
        assert_eq!(
            stored.failed_checks.as_deref(),
            Some("ci: failure, 3 tests failed (https://ci/1)")
        );
        assert_eq!(events.try_recv().unwrap().kind, "change_updated");

        f.server.poll_github().await;
        assert!(events.try_recv().is_err(), "nothing changed");
        assert_eq!(f.change(change.id).state, "open");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_person_merges_a_pull_request_from_the_app() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;

        let app = f.server.app.clone();
        let commit = tokio::task::spawn_blocking(move || app.merge_change(change.id, "looks good"))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(commit, "c0ffee");
        let merged = f.fake.pulls.lock().unwrap().merged.clone();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["commit_title"], "Merge #1: Issue 1");
        assert_eq!(f.change(change.id).state, "merged");
        assert_eq!(f.card(1).unwrap().column, "Done");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn git_reaches_a_web_address_with_the_installation_token_and_a_local_one_without() {
        let f = Fixture::new().await;
        let (repos, github) = (f.server.app.repos.clone(), f.server.github.clone().unwrap());
        let (web, local) = tokio::task::spawn_blocking(move || {
            let forge = GitHubForge::new(github, repos, tokio::runtime::Handle::current());
            let web = forge.auth("https://github.com/acme/widgets.git", "acme/widgets");
            let local = forge.auth("/srv/widgets.git", "acme/widgets");
            (web.unwrap(), local.unwrap())
        })
        .await
        .unwrap();
        assert_eq!(
            web.as_deref(),
            Some("Authorization: Basic eC1hY2Nlc3MtdG9rZW46Z2hzX3Rva2Vu"),
            "x-access-token:ghs_token"
        );
        assert_eq!(local, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_waits_for_the_checks_of_its_head_and_goes_back_when_they_fail() {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;

        // The change is published and, with no checks recorded for it yet, waits for them.
        let step = f.settle(&card).await;
        assert_eq!(step.waiting.as_deref(), Some("waiting for the checks"));
        let change = f.server.app.store.open_change(card.id).unwrap().unwrap();
        assert_eq!(change.pull_request, Some(7));
        let head = change.head.clone().unwrap();

        // Checks of an older commit do not count.
        f.fake.set_head("older");
        f.fake.pulls.lock().unwrap().check_runs = vec![check("build", "completed", "success")];
        f.server.poll_github().await;
        let step = f.settle(&card).await;
        assert_eq!(step.waiting.as_deref(), Some("waiting for the checks"));

        // The head moves to the published commit and its checks fail: the card goes back, told which.
        f.fake.set_head(&head);
        f.fake.pulls.lock().unwrap().check_runs = vec![
            check("build", "completed", "success"),
            check("lint", "completed", "failure"),
        ];
        f.server.poll_github().await;
        f.settle(&card).await;
        let card = f.server.app.card(card.id).unwrap();
        assert_eq!(card.column, "In progress");
        let note = card.note.unwrap();
        assert!(
            note.starts_with("The checks failed:\nlint: failure"),
            "{note}"
        );
        assert!(!note.contains("build"), "{note}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_without_checks_goes_on_once_the_grace_period_is_over() {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;
        f.settle(&card).await;
        assert_eq!(f.card(1).unwrap().column, "In review");

        let store = &f.server.app.store;
        let change = store.open_change(card.id).unwrap().unwrap();
        let long_ago = crate::store::now() - 3 * 60;
        store
            .set_change_head(change.id, change.head.as_deref().unwrap(), long_ago)
            .unwrap();
        f.settle(&card).await;
        assert_eq!(f.card(1).unwrap().column, "Ready to merge");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_card_keeps_one_branch_and_one_pull_request_across_rounds() {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;
        let app = f.server.app.clone();
        let publish = || {
            let app = app.clone();
            async move {
                tokio::task::spawn_blocking(move || app.publish_change(card.id).unwrap())
                    .await
                    .unwrap()
            }
        };
        let first = publish().await;
        f.code(&card, "second.txt").await;
        let second = publish().await;

        assert_eq!(f.fake.pulls.lock().unwrap().opened.len(), 1);
        assert_eq!(second.id, first.id);
        assert_eq!(second.pull_request, Some(7));
        assert_eq!(second.branch, "moebius/card-1");
        // The branch on GitHub moved forward from the first round's commit to the second's.
        let (first_head, second_head) = (first.head.unwrap(), second.head.unwrap());
        assert_ne!(first_head, second_head);
        assert_eq!(
            git(&f.remote, &["rev-parse", "moebius/card-1"]).unwrap(),
            second_head
        );
        assert!(
            git(
                &f.remote,
                &["merge-base", "--is-ancestor", &first_head, &second_head]
            )
            .is_ok()
        );

        // Checks recorded for the first commit say nothing about the second.
        let stored = f.change(first.id);
        app.store
            .set_change_status(stored.id, Some("success"), &first_head, None, None)
            .unwrap();
        assert_eq!(app.checks_of(&f.change(first.id)).unwrap(), "pending");
        app.store
            .set_change_status(stored.id, Some("success"), &second_head, None, None)
            .unwrap();
        assert_eq!(app.checks_of(&f.change(first.id)).unwrap(), "success");
    }

    #[tokio::test]
    async fn a_github_repository_cannot_offer_a_change_without_the_github_app() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::parse("", dir.path(), &dir.path().join("moebius.toml")).unwrap();
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let store = &server.app.store;
        let repository = store
            .add_github_repository("widgets", "/srv/widgets", "main", "acme/widgets", &[])
            .unwrap();
        let card = store
            .add_numbered_card(repository.id, 1, "Card", "", "leon", "In progress")
            .unwrap();
        let error = server.app.offer_change(card.id).unwrap_err();
        assert!(error.to_string().contains("forge github"), "{error}");
        assert!(store.changes(repository.id).unwrap().is_empty());
    }
}
