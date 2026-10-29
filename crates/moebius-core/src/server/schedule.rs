//! The schedule: once a minute, whether the Retro is due, a proposal of the Retro is to be tried, a research is to
//! start, a replay is to run, a lesson is to be retired or an automation is to fire (see `app::automations`), and the pause of the Pi of each
//! chat that has waited long for its next message (`PiBolt::pause_idle_chats`). What fails is reported on the bus as
//! `retro_error`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use super::Server;
use super::loops::every;
use crate::board::Runs;
use crate::bus::AppEvent;
use crate::improve::{lessons, replay, research, retro, trial};

/// How often the schedule looks at the clock to see whether the Retro is due, trials may start or chats wait idle.
const RETRO_CHECK: Duration = Duration::from_secs(60);

/// Looks at the clock every [`RETRO_CHECK`] until the future is dropped.
pub(super) async fn run(server: Arc<Server>) {
    every(server, RETRO_CHECK, Server::on_schedule).await
}

impl Server {
    /// Starts what is due now: the Retro, trials, research and replays, and pauses the Pi of chats that wait idle.
    fn on_schedule(&self) {
        let runs: Arc<dyn Runs> = self.harness.clone();
        let repo = self.harness_repo.as_ref();
        let report = |result: crate::app::Result<()>| {
            if let Err(e) = result {
                self.app.bus.publish(AppEvent::new(
                    "retro_error",
                    json!({ "message": e.to_string() }),
                ));
            }
        };
        let config = self.live.get();
        report(retro::on_schedule(
            &self.app,
            (&runs, &self.live),
            repo,
            config.retro_at,
        ));
        let (window, limit) = (config.trial_window, config.run_limit);
        let regression = config.regression();
        report(trial::on_schedule(
            &self.app,
            &runs,
            repo,
            (window, limit),
            regression,
        ));
        report(research::on_schedule(
            &self.app,
            &runs,
            window,
            research::MEASURE_LIMIT,
        ));
        report(replay::on_schedule(&self.app, window));
        lessons::on_schedule(&self.app);
        report(self.on_automations(&*runs));
        self.harness.pause_idle_chats(&self.app, Instant::now());
    }

    /// Ends the chats of automations that have answered and fires the automations that came due, at the local minute.
    fn on_automations(&self, runs: &dyn Runs) -> crate::app::Result<()> {
        let now = self.app.store.local_minute()?;
        match crate::app::cron::parse_minute(&now) {
            Some(now) => crate::app::on_automation_schedule(&self.app, runs, now),
            None => Ok(()),
        }
    }
}
