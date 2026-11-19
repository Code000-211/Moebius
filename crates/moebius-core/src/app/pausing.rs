//! Pausing all agents: the pause is kept in the store, so the board engine, which asks `board::may_start` before every
//! run, starts none while it holds, and the harness holds the runs that are going. Resuming lets them go on where they
//! were and the board engine start runs again.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::{App, Result};
use crate::board::CardRuns;
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::Paused;

/// Held by [`App::pause_runs`] and [`App::resume_runs`] throughout, so that a resume cannot come between a pause's
/// store and its hold of the runs and leave them held.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

impl App {
    /// The pause that is in force: who paused all agents and when, or none.
    pub fn runs_paused(&self) -> Result<Option<Paused>> {
        Ok(self.store.runs_paused()?)
    }

    /// A person pauses all agents: the runs going are held and no run starts until [`App::resume_runs`]. Pausing again
    /// changes nothing. Returns the pause in force.
    pub fn pause_runs(&self, by: &Actor, runs: &dyn CardRuns, reason: &str) -> Result<Paused> {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let author = self.author_of(by);
        if self.store.pause_runs(&author)? {
            // The pause is stored first: a run that starts meanwhile sees it and holds itself.
            runs.hold(true);
            self.record_action(by, "pause_runs", None, None, BTreeMap::new(), reason)?;
            if let Some(paused) = self.store.runs_paused()? {
                self.bus.publish(Event::RunsPaused {
                    by: paused.by,
                    at: paused.at,
                });
            }
        }
        self.store
            .runs_paused()?
            .ok_or_else(|| super::AppError::Invalid("the pause was lifted meanwhile".into()))
    }

    /// A person resumes all agents: the runs held go on and the board starts runs again. False when they were not
    /// paused.
    pub fn resume_runs(&self, by: &Actor, runs: &dyn CardRuns, reason: &str) -> Result<bool> {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        if !self.store.resume_runs()? {
            return Ok(false);
        }
        runs.hold(false);
        self.record_action(by, "resume_runs", None, None, BTreeMap::new(), reason)?;
        self.bus.publish(Event::RunsResumed {});
        Ok(true)
    }
}
