//! The server put together (spec 2): the store and the core, the desktop API, the router, the Pi-Bolt harness and the
//! board engine, which runs every card's column script on a fixed tick.

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
use crate::pibolt::PiBolt;
use crate::repos::Worktree;
use crate::router::{Router, Upstream};
use crate::store::Store;

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
        Ok(Self {
            config,
            app,
            router,
            harness,
            engine,
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

    /// Serves the desktop API and the router, and ticks the board, until the future is dropped.
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
        tokio::select! {
            served = api::serve(api_listener, state) => served,
            served = self.router.clone().serve(router_listener) => served,
            () = board => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repos::git;

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
}
