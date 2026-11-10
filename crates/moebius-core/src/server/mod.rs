//! The server put together (spec 2): the store and the core, the desktop API, the router with the check and the MCP
//! gateway, the harnesses, the board engine and the loops that keep them going. The built-in skills are written to the
//! data directory at start.
//!
//! This module wires every part from the configuration and serves them; `leftovers` ends what a server that stopped
//! left going, `loops` holds the board's tick, the polls of the forges and the refresh of the clones, and `schedule` what is
//! looked at once a minute. `api` is the desktop API, `settings` edits `moebius.toml` while the server runs, and
//! `setup` is what the Models, Plugins and Get started pages show. `plugins` starts the plugins in their own process, `packages` is the package manager, and
//! `webhooks` is how a forge's webhook wakes its poll.

pub mod api;
#[cfg(test)]
mod fake_forgejo;
#[cfg(test)]
mod fake_github;
#[cfg(test)]
mod fake_gitlab;
mod leftovers;
mod loops;
pub mod packages;
pub mod plugins;
pub mod ports;
mod schedule;
pub mod settings;
pub mod setup;
pub mod webhooks;

use std::net::SocketAddr;
use std::sync::Arc;

use moebius_plugin_api::Host;
use tokio::net::TcpListener;

use crate::app::App;
use crate::board::{Engine, Kanban, Limits};
use crate::config::{Config, GitHubAuth, TokenSource};
use crate::decide::check;
use crate::decide::guard::{Guard, Teacher};
use crate::decide::localize::Localizer;
use crate::decide::model::{self, DecisionModel, Routed};
use crate::harness::gateway::Gateway;
use crate::harness::pibolt::PiBolt;
use crate::harness::pick::Harnesses;
use crate::harness::remote::Runners;
use crate::harness::repo::HarnessRepo;
use crate::harness::run::{
    AUTORESEARCHER_AGENT, CODER_AGENT, GRILLING_AGENT, MOEBIUS_AGENT, RETRO_AGENT, REVIEWER_AGENT,
};
use crate::harness::skill;
use crate::improve::lessons;
use crate::repos::forge::{ForgejoForge, GitHubForge, GitLabForge};
use crate::repos::forgejo::Forgejo;
use crate::repos::github::{GitHub, GitHubError};
use crate::repos::gitlab::GitLab;
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
    /// Pi-Bolt and Claude Code, which run the agents.
    pub harness: Arc<Harnesses>,
    pub engine: Arc<Engine>,
    /// The configuration the server runs with, which edits of the settings change.
    pub live: Arc<Live>,
    /// The harness configuration repository, if one is configured.
    pub harness_repo: Option<Arc<HarnessRepo>>,
    /// The client for GitHub, if one is configured.
    pub github: Option<Arc<GitHub>>,
    /// The client for the GitLab, if one is configured.
    pub gitlab: Option<Arc<GitLab>>,
    /// The client for the Forgejo (such as Codeberg), if one is configured.
    pub forgejo: Option<Arc<Forgejo>>,
    pub token: Arc<str>,
    /// The browsers signed in to the web app, and the codes that sign one in.
    pub sessions: api::Sessions,
}

/// The access token of `forge` from where the configuration keeps it: a file, or a secret. An empty token is an error.
fn read_token(token: &TokenSource, secrets: &Secrets, forge: &str) -> Result<String, String> {
    let token = match token {
        TokenSource::File(file) => std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read {}: {e}", file.display()))?,
        TokenSource::Secret(name) => secrets.resolve(None, Some(name))?.unwrap_or_default(),
    };
    if token.trim().is_empty() {
        return Err(format!("the {forge} access token is empty"));
    }
    Ok(token)
}

/// The client for the GitHub of `g`: with an App, its key file must be readable and hold an RSA key in PEM form; with
/// an access token, the token must be readable and not empty. Nothing is asked of GitHub here (see [`check_github`]).
fn github_client(g: &crate::config::GitHubConfig, secrets: &Secrets) -> Result<GitHub, String> {
    match &g.auth {
        GitHubAuth::App {
            app_id,
            private_key_file,
        } => {
            let key = std::fs::read(private_key_file)
                .map_err(|e| format!("cannot read {}: {e}", private_key_file.display()))?;
            GitHub::new(&g.api_url, *app_id, &key).map_err(|e| e.to_string())
        }
        GitHubAuth::Token(source) => {
            let token = read_token(source, secrets, "GitHub")?;
            Ok(GitHub::with_token(&g.api_url, &token))
        }
    }
}

/// Asks GitHub who an access token belongs to, so that a token it refuses, or a classic one without the scope `repo`,
/// stops the server from starting with a sentence that says so. An App has nothing to check. GitHub being out of reach
/// is not a reason to refuse: the token is tried again with every call. Runs on a thread of its own, since the server
/// starts inside a runtime.
fn check_github(github: &GitHub) -> Result<(), String> {
    if !github.is_token() {
        return Ok(());
    }
    let answer = std::thread::scope(|scope| {
        scope
            .spawn(|| -> Result<_, String> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("cannot check the GitHub access token: {e}"))?;
                Ok(runtime.block_on(github.check_token()))
            })
            .join()
            .unwrap_or_else(|_| Err("the check of the GitHub access token panicked".into()))
    })?;
    match answer {
        Ok(owner) => {
            if let Some(owner) = owner {
                eprintln!("Moebius acts on GitHub as {}.", owner.login);
            }
            Ok(())
        }
        Err(GitHubError::Http(e)) => {
            eprintln!(
                "Could not reach GitHub to check the access token ({e}); it is tried again with every call."
            );
            Ok(())
        }
        Err(e @ GitHubError::RateLimited { .. }) => {
            eprintln!(
                "Could not check the GitHub access token now: {e}; it is tried again with every call."
            );
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// The checks of `Server::new` on the forges of `config` that depend on files and secrets: the key of the GitHub App
/// must be a readable RSA key and the access tokens of GitHub, GitLab and Forgejo must be readable. `Server::new` and the
/// editing of the settings both run them, so a file that `moebius serve` would refuse is not written.
pub(crate) fn check_forges(config: &Config, secrets: &Secrets) -> Result<(), String> {
    if let Some(g) = &config.github {
        github_client(g, secrets)?;
    }
    if let Some(g) = &config.gitlab {
        read_token(&g.token, secrets, "GitLab")?;
    }
    if let Some(f) = &config.forgejo {
        read_token(&f.token, secrets, "Forgejo")?;
    }
    Ok(())
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
        crate::workflows::nodes::http::configure(&config);
        let upstreams = secrets.upstreams(&config)?;
        let router = Arc::new(
            Router::new(store.clone(), upstreams)
                .with_day_budget(config.day_tokens)
                .with_classes(config.classes.clone())
                .with_model_thinking(config.model_thinking.clone())
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
            plugins::put_in_host(&host, &config, plugin)?;
        }
        if let Some(model) = &model {
            guard = guard.with_decision_model(model.clone(), routed.clone());
            host.add(model.clone());
        }
        host.add(Localizer::new(store.clone()));
        host.add(guard);
        let host = Arc::new(host);
        let mut app = App::new(store.clone(), host.clone(), &config.data_dir);
        app.decisions.share_routed(routed);
        app.checks_timeout = config.checks_timeout;
        app.localize = config.localize;
        app.packages = config.packages.clone();
        plugins::route_deciders(&app.decisions, &config);
        // Whether the hint is asked for at all is the routing of `localize`, which can change while the server runs.
        app.localize.enabled = true;
        if !config.guard_enabled {
            eprintln!(
                "moebius: the guard is off ([guard] enabled = false): tool calls are not checked, neither by the built-in safety patterns nor by the rules"
            );
        }
        if config.has_lens_table {
            eprintln!(
                "moebius: the [lens] table is no longer used (the lens was removed) and is ignored; you can delete it"
            );
        }
        let launcher = plugins::launchers_of(&host, &config.plugins);
        let github = match &config.github {
            Some(g) => {
                let github = github_client(g, &secrets)?;
                check_github(&github)?;
                let github = Arc::new(github);
                app.add_forge(Box::new(GitHubForge::new(
                    github.clone(),
                    app.repos.clone(),
                    tokio::runtime::Handle::current(),
                )));
                Some(github)
            }
            None => None,
        };
        let gitlab = match &config.gitlab {
            Some(g) => {
                let token = read_token(&g.token, &secrets, "GitLab")?;
                let gitlab = Arc::new(GitLab::new(&g.url, &token));
                app.add_forge(Box::new(GitLabForge::new(
                    gitlab.clone(),
                    app.repos.clone(),
                    tokio::runtime::Handle::current(),
                )));
                Some(gitlab)
            }
            None => None,
        };
        let forgejo = match &config.forgejo {
            Some(f) => {
                let token = read_token(&f.token, &secrets, "Forgejo")?;
                let forgejo = Arc::new(Forgejo::new(&f.url, &token));
                app.add_forge(Box::new(ForgejoForge::new(
                    forgejo.clone(),
                    app.repos.clone(),
                    tokio::runtime::Handle::current(),
                )));
                Some(forgejo)
            }
            None => None,
        };
        app.set_server_info(crate::app::ServerInfo::detect(
            &config.file,
            &config.data_dir,
            router_addr.to_string(),
        ));
        let app = Arc::new(app);
        router.set_checker(check::Checker::new(app.clone(), config.rules_path()));
        let runners = Arc::new(Runners::new(config.runners.clone()));
        leftovers::end_runs_of_a_previous_server(&app, &runners)?;
        if let Some(launcher) = &launcher {
            // The runs of the last start are ended: nothing exists that the launcher must keep.
            launcher.clean_leftovers(&[])?;
        }
        if let Some(model) = model {
            model::start(&app, model)
                .map_err(|e| format!("cannot start the decision model: {e}"))?;
        }

        let mut pi_bolt = PiBolt::new(
            config.pi_bolt.clone(),
            config.moebius.clone(),
            format!("http://{router_addr}"),
            router.clone(),
            config.agents.clone(),
        );
        pi_bolt.runners = runners;
        pi_bolt.launcher.set(launcher);
        let harness = Arc::new(Harnesses::new(pi_bolt, &config.claude_code));
        app.set_runs(harness.clone());
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
            // This also gives older automations the workflow files that hold them from now on.
            app.set_harness_repo(harness_repo.clone());
        }
        // A run of a workflow that waits for a person waits on with this server: its node rows are replayed, so a
        // decision made while no server ran is taken up and an open wait is waited for again.
        if let Err(error) = app.resume_waiting_workflow_runs() {
            eprintln!("Moebius cannot resume the workflow runs that wait: {error}");
        }
        warn_about_launch_agents(&config, &harness);
        let engine = Arc::new(Engine::new(
            app.clone(),
            harness.clone(),
            Kanban::default(),
            Limits {
                run_limit: config.run_limit,
                round_limit: config.round_limit,
                day_budget: config.day_tokens,
                ..Limits::default()
            },
        ));
        let live = Arc::new(
            Live::new(config.clone())
                .with_targets(engine.clone(), router.clone(), harness.agents.clone())
                .with_launchers(harness.pi_bolt.launcher.clone()),
        );
        Ok(Self {
            config,
            app,
            router,
            harness,
            engine,
            live,
            harness_repo,
            github,
            gitlab,
            forgejo,
            token: token.into(),
            sessions: api::Sessions::default(),
        })
    }

    /// The address that opens the web app on the API at `address`, signed in with a new one-time code.
    pub fn web_login_url(&self, address: SocketAddr) -> Result<String, String> {
        let (code, _) = self
            .sessions
            .new_code()
            .map_err(|e| format!("cannot make a login code: {e}"))?;
        Ok(api::login_url(&api_url(address), &code))
    }

    /// What the desktop API needs from the server.
    pub fn api_state(&self) -> ApiState {
        ApiState {
            app: self.app.clone(),
            runs: self.harness.clone(),
            harness_repo: self.harness_repo.clone(),
            token: self.token.clone(),
            router: self.router.clone(),
            config: self.live.clone(),
            sessions: self.sessions.clone(),
        }
    }

    /// Serves the desktop API and the router, ticks the board and polls the forges, until the future is dropped.
    pub async fn serve(
        self: Arc<Self>,
        api_listener: TcpListener,
        router_listener: TcpListener,
    ) -> std::io::Result<()> {
        if let Ok(address) = api_listener.local_addr() {
            self.harness.set_api_url(api_url(address));
            self.app.set_api_address(api_url(address));
        }
        let state = self.api_state();
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
            .merge(lessons::tool::routes(self.app.clone(), self.router.clone()))
            .merge(gateway.clone().routes());
        tokio::select! {
            served = api::serve(api_listener, state) => served,
            served = axum::serve(router_listener, router_routes) => served,
            () = gateway.stop_ended_runs() => Ok(()),
            () = loops::board(self.clone()) => Ok(()),
            () = loops::poll(self.clone()) => Ok(()),
            () = loops::refresh(self.clone()) => Ok(()),
            () = loops::levels(self.clone()) => Ok(()),
            () = ports::watch(self.app.clone(), ports::PERIOD) => Ok(()),
            () = schedule::run(self.clone()) => Ok(()),
            () = self.live.restart_asked() => Ok(()),
        }
    }
}

/// Says at the start which `launch_agents` of the launchers are no agent that exists: such a name takes no run, so the
/// agent it was meant for is wrapped by the last launcher instead of the one that names it.
fn warn_about_launch_agents(config: &Config, harness: &Harnesses) {
    let mut known: std::collections::BTreeSet<String> = [
        MOEBIUS_AGENT,
        GRILLING_AGENT,
        CODER_AGENT,
        REVIEWER_AGENT,
        RETRO_AGENT,
        AUTORESEARCHER_AGENT,
    ]
    .map(String::from)
    .into();
    known.extend(config.agents.keys().cloned());
    known.extend(
        harness
            .agents
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .agents
            .keys()
            .cloned(),
    );
    for each in plugins::unknown_launch_agents(&config.plugins, &known) {
        eprintln!(
            "moebius: plugins.launch_agents ({each}) is the name of no agent, so it takes no run"
        );
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
            "moebius-report",
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
                let page = page.unwrap();
                if !page.file_type().unwrap().is_file() {
                    continue;
                }
                let name = page.file_name();
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

    #[tokio::test]
    async fn the_server_needs_the_forgejo_token_from_its_file_or_its_secret() {
        let dir = tempfile::tempdir().unwrap();
        let start = |text: &str| {
            let config = Config::parse(text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
            Server::new(config, "127.0.0.1:1".parse().unwrap())
        };
        let error = start("[forgejo]\ntoken_file = \"missing-token\"\n")
            .err()
            .unwrap();
        assert!(error.contains("missing-token"), "{error}");
        let error = start("[forgejo]\ntoken_secret = \"codeberg\"\n")
            .err()
            .unwrap();
        assert!(
            error.contains("the secret codeberg is not stored"),
            "{error}"
        );
        std::fs::write(dir.path().join("empty"), "\n").unwrap();
        let error = start("[forgejo]\ntoken_file = \"empty\"\n").err().unwrap();
        assert_eq!(error, "the Forgejo access token is empty");

        Secrets::new(&dir.path().join(".moebius"))
            .set("codeberg", "secret-token")
            .unwrap();
        let server = start("[forgejo]\ntoken_secret = \"codeberg\"\n").unwrap();
        let forgejo = server.forgejo.unwrap();
        assert_eq!(
            (forgejo.url(), forgejo.token()),
            ("https://codeberg.org", "secret-token")
        );
    }

    #[tokio::test]
    async fn the_server_needs_the_gitlab_token_from_its_file_or_its_secret() {
        let dir = tempfile::tempdir().unwrap();
        let start = |text: &str| {
            let config = Config::parse(text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
            Server::new(config, "127.0.0.1:1".parse().unwrap())
        };
        let error = start("[gitlab]\ntoken_file = \"missing-token\"\n")
            .err()
            .unwrap();
        assert!(error.contains("missing-token"), "{error}");
        let error = start("[gitlab]\ntoken_secret = \"gitlab\"\n")
            .err()
            .unwrap();
        assert!(error.contains("the secret gitlab is not stored"), "{error}");

        Secrets::new(&dir.path().join(".moebius"))
            .set("gitlab", "glpat-secret")
            .unwrap();
        let server = start("[gitlab]\ntoken_secret = \"gitlab\"\n").unwrap();
        assert_eq!(server.gitlab.unwrap().token(), "glpat-secret");
        std::fs::write(dir.path().join("gitlab-token"), "glpat-file\n").unwrap();
        let server = start("[gitlab]\ntoken_file = \"gitlab-token\"\n").unwrap();
        let gitlab = server.gitlab.unwrap();
        assert_eq!(
            (gitlab.url(), gitlab.token()),
            ("https://gitlab.com", "glpat-file")
        );
    }
}
