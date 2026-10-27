//! The server put together (spec 2): the store and the core, the desktop API, the router, the Pi-Bolt harness and the
//! board engine, which runs every card's column script on a fixed tick.

use std::net::SocketAddr;
use std::sync::Arc;

use moebius_plugin_api::Host;
use serde_json::json;
use tokio::net::TcpListener;

use crate::api::{self, ApiState};
use crate::app::{App, columns};
use crate::board::{Engine, Limits, Workflow};
use crate::bus::AppEvent;
use crate::config::Config;
use crate::pibolt::{AgentProfile, PiBolt};
use crate::router::{Router, Upstream};
use crate::store::Store;

/// How long a call into a plugin may take before it counts as refused.
const PLUGIN_TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

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

        let agents = config
            .agents
            .iter()
            .map(|(name, a)| {
                (
                    name.clone(),
                    AgentProfile {
                        model: a.model.clone(),
                        thinking: a.thinking.clone(),
                        instructions: a.instructions.clone(),
                        skills: a.skills.clone(),
                        extensions: a.extensions.clone(),
                        protocol: a.protocol,
                        token_budget: a.token_budget,
                    },
                )
            })
            .collect();
        let harness = Arc::new(PiBolt::new(
            config.pi_bolt.clone(),
            format!("http://{router_addr}"),
            router.clone(),
            agents,
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
