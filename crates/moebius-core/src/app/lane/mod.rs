//! The test lane (ADR 0035): a named machine a change is built, tested and measured on once the Reviewer approved its
//! code, so that a measurement the owner's repository asks for is made by the owner's own hardware and goes on to the
//! Reviewer without a click.
//!
//! - this file: the lanes of the server's configuration (`Lanes`), kept where the app can ask for them;
//! - [`definition`]: a project's job, `projects/<project>/lane.toml`: steps with commands and time limits, and which of
//!   them measure;
//! - `request`: queueing a job for a change at its head (a card entering a column marked `lane = true`, a column
//!   script's `moebius.lane_job()`, a person's request), and everything that stops one: a freeze, a hold, a pause, a
//!   locked path, no review verdict yet;
//! - `exec` and `job`: running a job on the machine, in a container or directly as the lane's isolation says;
//! - `rows` and `report`: what a measuring step printed, and the report of the job;
//! - `results`: what a job's end does: the failure answered on the pull request, the measurement handed to the Reviewer,
//!   a lane that paused itself, a new push that cancels;
//! - `view`: the lane's settings of a repository and the report of a card, for the desktop API.
//!
//! Nothing here reads a secret of the server, pushes to a forge or merges: the job only reads the pinned commits, and the
//! only things written to the forge are one plain reply on a failure and what the existing review and merge paths write.

pub mod definition;
mod exec;
mod job;
mod report;
mod request;
mod results;
mod rows;
mod view;

use std::sync::RwLock;

pub use report::measurement_section;
pub use request::LaneRequest;
pub use view::{CardLane, DefinitionView, LaneView, RepositoryLane};

use crate::config::{LaneConfig, RunnerConfig};

/// The prefix of the measurer's name a measurement of a lane carries, as in `lane:gpu1`.
pub const MEASURER_PREFIX: &str = "lane:";

/// The kind of the inbox item that says a lane stopped itself.
pub const LANE_PAUSED: &str = "lane_paused";

/// The lanes of the configuration, and the runners they are reached through.
#[derive(Default)]
pub struct Lanes {
    setup: RwLock<Setup>,
    /// Variables the tests give every process of a machine (see `exec::Machine::with_environment`).
    #[cfg(test)]
    environment: RwLock<Vec<(String, String)>>,
}

#[derive(Default)]
struct Setup {
    lanes: Vec<LaneConfig>,
    runners: Vec<RunnerConfig>,
}

impl Lanes {
    /// Sets the lanes and the runners they use, as the configuration has them.
    pub fn configure(&self, lanes: Vec<LaneConfig>, runners: Vec<RunnerConfig>) {
        *self.setup.write().unwrap_or_else(|e| e.into_inner()) = Setup { lanes, runners };
    }

    /// What the tests put in the environment of every process of a lane's machine.
    #[cfg(test)]
    pub fn test_environment(&self) -> Vec<(String, String)> {
        self.environment
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Sets what [`Lanes::test_environment`] says.
    #[cfg(test)]
    pub fn set_test_environment(&self, variables: Vec<(String, String)>) {
        *self.environment.write().unwrap_or_else(|e| e.into_inner()) = variables;
    }

    /// The lane called `name`.
    pub fn lane(&self, name: &str) -> Option<LaneConfig> {
        self.all().into_iter().find(|l| l.name == name)
    }

    /// Every lane, in the order of the configuration.
    pub fn all(&self) -> Vec<LaneConfig> {
        self.setup
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .lanes
            .clone()
    }

    /// The runner called `name`.
    pub fn runner(&self, name: &str) -> Option<RunnerConfig> {
        self.setup
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .runners
            .iter()
            .find(|r| r.name == name)
            .cloned()
    }
}
