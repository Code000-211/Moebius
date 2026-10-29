//! The server put together (spec 2): the store and the core, the desktop API, the router with the check and the MCP
//! gateway, the Pi-Bolt harness, the board engine and the loops that keep them going. The built-in skills are written
//! to the data directory at start.
//!
//! This module wires every part from the configuration and serves them; `leftovers` ends what a server that stopped
//! left going, `loops` holds the board's tick, the poll of GitHub and the refresh of the clones, and `schedule` what is
//! looked at once a minute. `api` is the desktop API, `settings` edits `moebius.toml` while the server runs, and
//! `setup` is what the Models, Plugins and Get started pages show. `plugins` starts the plugins in their own process.

pub mod api;
#[cfg(test)]
mod fake_github;
mod leftovers;
mod loops;
mod plugins;
mod schedule;
pub mod settings;
pub mod setup;

use std::net::SocketAddr;
use std::sync::Arc;

use moebius_plugin_api::Host;
use tokio::net::TcpListener;

use crate::app::App;
use crate::board::{Engine, Limits, Workflow};
use crate::config::Config;
use crate::decide::check;
use crate::decide::guard::{self, Guard, Teacher};
use crate::decide::model::{self, DecisionModel, Routed};
use crate::harness::gateway::Gateway;
use crate::harness::pibolt::PiBolt;
use crate::harness::repo::HarnessRepo;
use crate::harness::skill;
use crate::repos::forge::GitHubForge;
use crate::repos::github::GitHub;
use crate::router::Router;
use crate::server::api::ApiState;
use crate::server::settings::{Live, Secrets};
use crate::store::Store;

/// How long a call into a plugin may take before it counts as refused.
const PLUGIN_TIME_LIMIT: std::time::Duration =
    std::time::Duration::from_secs(crate::config::PLUGIN_TIME_LIMIT_SECONDS);

/// The URL runs reach the desktop API on: the address it listens on, with the loopback address for one that listens on
/// all interfaces.
fn api_url(listening: SocketAddr) -> String {
    let ip = match listening.ip() {
        ip if ip.is_unspecified() && ip.is_ipv6() => std::net::Ipv6Addr::LOCALHOST.into(),
        ip if ip.is_unspecified() => std::net::Ipv4Addr::LOCALHOST.into(),
        ip => ip,
    };
    format!("http://{}", SocketAddr::new(ip, listening.port()))
}

pub struct Server {
    pub config: Config,
    pub app: Arc<App>,
    pub router: Arc<Router>,
    pub harness: Arc<PiBolt>,
    pub engine: Arc<Engine>,
    /// The configuration the server runs with, which edits of the settings change.
    pub live: Arc<Live>,
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
            .map_err(|e| format!("cannot write the built-in skills: {e}"))?;
        let store = Arc::new(
            Store::open(&config.data_dir).map_err(|e| format!("cannot open the store: {e}"))?,
        );
        let token = api::load_or_create_token(&config.token_file)
            .map_err(|e| format!("cannot read {}: {e}", config.token_file.display()))?;
        let secrets = Secrets::new(&config.data_dir);
        let upstreams = secrets.upstreams(&config)?;
        let router = Arc::new(
            Router::new(store.clone(), upstreams)
                .with_day_budget(config.day_tokens)
                .with_classes(config.classes.clone())
                .with_secrets(secrets.clone()),
        );

        let teacher = config
            .teacher
            .as_ref()
            .map(|teacher| Teacher::new(teacher, router.clone(), format!("http://{router_addr}")));
        let model = match &config.decision_model {
            Some(m) => {
                let key = secrets.resolve(m.api_key_env.as_deref(), m.api_key_secret.as_deref())?;
                Some(DecisionModel::new(m, key))
            }
            None => None,
        };
        let routed = Arc::new(Routed::default());
        let mut guard = Guard::new(teacher);
        let mut host = Host::new(PLUGIN_TIME_LIMIT);
        for plugin in &config.plugins {
            host.add_process(plugins::start(&config, plugin, PLUGIN_TIME_LIMIT)?);
        }
        if let Some(model) = &model {
            guard = guard.with_decision_model(model.clone(), routed.clone());
            host.add(model.clone());
        }
        host.add(guard);
        let mut app = App::new(store.clone(), Arc::new(host), &config.data_dir);
        app.decisions.share_routed(routed);
        app.checks_timeout = config.checks_timeout;
        app.decisions.route("tool_call", guard::NAME);
        for plugin in &config.plugins {
            for kind in &plugin.decides {
                app.decisions.route(kind, &plugin.name);
            }
        }
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
        leftovers::end_runs_of_a_previous_server(&app)?;
        if let Some(model) = model {
            model::start(&app, model)
                .map_err(|e| format!("cannot start the decision model: {e}"))?;
        }

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
        if let Some(harness_repo) = &harness_repo {
            app.set_harness_repo(harness_repo.clone());
        }
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
        let live = Arc::new(Live::new(config.clone()).with_targets(
            engine.clone(),
            router.clone(),
            harness.agents.clone(),
        ));
        Ok(Self {
            config,
            app,
            router,
            harness,
            engine,
            live,
            harness_repo,
            github,
            token: token.into(),
        })
    }

    /// Serves the desktop API and the router, ticks the board and polls GitHub, until the future is dropped.
    pub async fn serve(
        self: Arc<Self>,
        api_listener: TcpListener,
        router_listener: TcpListener,
    ) -> std::io::Result<()> {
        if let Ok(address) = api_listener.local_addr() {
            let _ = self.harness.api_url.set(api_url(address));
        }
        let state = ApiState {
            app: self.app.clone(),
            runs: self.harness.clone(),
            harness_repo: self.harness_repo.clone(),
            token: self.token.clone(),
            router: self.router.clone(),
            config: self.live.clone(),
        };
        let gateway = Arc::new(Gateway::new(
            self.app.clone(),
            self.router.clone(),
            self.config.rules_path(),
        ));
        let router_routes = self
            .router
            .clone()
            .routes()
            .merge(check::routes(
                self.app.clone(),
                self.router.clone(),
                self.config.rules_path(),
            ))
            .merge(gateway.clone().routes());
        tokio::select! {
            served = api::serve(api_listener, state) => served,
            served = axum::serve(router_listener, router_routes) => served,
            () = gateway.stop_ended_runs() => Ok(()),
            () = loops::board(self.clone()) => Ok(()),
            () = loops::poll(self.clone()) => Ok(()),
            () = loops::refresh(self.clone()) => Ok(()),
            () = schedule::run(self.clone()) => Ok(()),
            () = self.live.restart_asked() => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn the_moebius_skill_is_written_at_start_from_the_guide_and_the_reference() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::parse("", dir.path(), &dir.path().join("moebius.toml")).unwrap();
        Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let skill = dir.path().join(".moebius/skills/moebius");
        let skill_md = std::fs::read_to_string(skill.join("SKILL.md")).unwrap();
        assert!(skill_md.starts_with("---\nname: moebius\n"));
        for task in [
            "moebius-operate",
            "moebius-configure",
            "moebius-workflows",
            "moebius-extensions",
            "moebius-plugins",
        ] {
            let page = dir
                .path()
                .join(".moebius/skills")
                .join(task)
                .join("SKILL.md");
            let page = std::fs::read_to_string(page).unwrap();
            assert!(page.starts_with(&format!("---\nname: {task}\n")));
        }
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
}
