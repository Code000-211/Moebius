//! The lane as the desktop API shows it: a repository's "Test lane" settings (which lane it uses, the lanes there are,
//! what each isolates and warns of, whether the project has a valid job definition) and a card's report (the jobs of its
//! change and why none is queued now). A person assigns the lane; nobody else changes it.

use std::collections::BTreeMap;

use serde::Serialize;

use super::definition::{self, LANE_FILE};
use crate::app::{App, AppError, Result, Sight};
use crate::config::LaneConfig;
use crate::decide::Actor;
use crate::store::{LaneJob, LanePause};

/// A lane as the API lists it.
#[derive(Debug, Clone, Serialize)]
pub struct LaneView {
    pub name: String,
    /// The runner it is reached through, or none for the server itself.
    pub runner: Option<String>,
    /// `always`, `external_only` or `off`.
    pub isolation: String,
    /// What the owner is warned of while the lane is not isolated everywhere.
    pub warning: Option<String>,
    pub runtime: String,
    pub image: Option<String>,
    pub gpus: Option<String>,
    pub power_cap: Option<String>,
    pub rocm: Option<String>,
    pub driver: Option<String>,
    pub devices: Vec<String>,
    pub timeout_seconds: u64,
    /// How many jobs wait for it, and whether one runs.
    pub queued: usize,
    pub busy: bool,
    /// The job that runs: only for someone who may see its card, since it names the card, the commit and the step of a
    /// repository that may be private to others. Everyone else sees `busy` and the length of the queue.
    pub running: Option<LaneJob>,
    /// Set while the lane has stopped itself after jobs that failed for its own reasons.
    pub paused: Option<LanePause>,
}

/// What the project's `lane.toml` is.
#[derive(Debug, Clone, Serialize)]
pub struct DefinitionView {
    pub present: bool,
    pub problem: Option<String>,
    pub steps: Vec<String>,
    pub measures: bool,
}

/// A repository's "Test lane" settings.
#[derive(Debug, Clone, Serialize)]
pub struct RepositoryLane {
    /// The lane the repository uses, or none.
    pub lane: Option<String>,
    /// The lane as it is now, when it is one the configuration has.
    pub current: Option<LaneView>,
    /// Every lane of the configuration.
    pub lanes: Vec<LaneView>,
    pub definition: DefinitionView,
    /// The latest jobs of the repository, newest first.
    pub jobs: Vec<LaneJob>,
}

/// A card's report of the lane.
#[derive(Debug, Clone, Serialize)]
pub struct CardLane {
    pub lane: Option<String>,
    /// The jobs of the card's change, oldest first.
    pub jobs: Vec<LaneJob>,
    /// The head of the change now: a job of another head is out of date.
    pub head: Option<String>,
    /// Why no job is queued for the head now, when none is.
    pub waiting: Option<String>,
}

impl App {
    /// Whether `sight` sees the card of `job`.
    fn sees_job(&self, sight: &Sight, job: &LaneJob) -> bool {
        self.store
            .card(job.card_id)
            .ok()
            .flatten()
            .is_some_and(|card| sight.card(&card))
    }

    fn lane_view(&self, lane: &LaneConfig, sight: &Sight) -> Result<LaneView> {
        let queue = self.store.lane_queue(&lane.name)?;
        Ok(LaneView {
            name: lane.name.clone(),
            runner: lane.runner.clone(),
            isolation: lane.isolation.as_str().into(),
            warning: lane.isolation.warning().map(String::from),
            runtime: lane.runtime.clone(),
            image: lane.image.clone(),
            gpus: lane.gpus.clone(),
            power_cap: lane.power_cap.clone(),
            rocm: lane.rocm.clone(),
            driver: lane.driver.clone(),
            devices: lane.devices.clone(),
            timeout_seconds: lane.timeout_seconds,
            queued: queue.iter().filter(|j| j.state == "queued").count(),
            busy: queue.iter().any(|j| j.state == "running"),
            running: queue
                .iter()
                .find(|j| j.state == "running" && self.sees_job(sight, j))
                .cloned(),
            paused: self.store.lane_pause(&lane.name)?,
        })
    }

    /// Every lane of the configuration with its state, as `sight` sees it.
    pub fn lane_views(&self, sight: &Sight) -> Result<Vec<LaneView>> {
        self.lanes
            .all()
            .iter()
            .map(|lane| self.lane_view(lane, sight))
            .collect()
    }

    /// The "Test lane" settings of a repository.
    pub fn repository_lane_view(
        &self,
        repository_id: i64,
        sight: &Sight,
    ) -> Result<RepositoryLane> {
        let repository = self.repository(repository_id)?;
        let lane = self.store.repository_lane(repository_id)?;
        let lanes = self.lane_views(sight)?;
        let current = lane
            .as_ref()
            .and_then(|name| lanes.iter().find(|l| &l.name == name).cloned());
        let text = self
            .project(repository.home_project_id)
            .ok()
            .and_then(|project| self.project_file(&project, LANE_FILE));
        let definition = match text.as_deref().map(definition::parse) {
            None => DefinitionView {
                present: false,
                problem: None,
                steps: Vec::new(),
                measures: false,
            },
            Some(Ok(found)) => DefinitionView {
                present: true,
                problem: None,
                measures: found.measures(),
                steps: found.steps.iter().map(|s| s.name.clone()).collect(),
            },
            Some(Err(problem)) => DefinitionView {
                present: true,
                problem: Some(problem),
                steps: Vec::new(),
                measures: false,
            },
        };
        Ok(RepositoryLane {
            lane,
            current,
            lanes,
            definition,
            jobs: self
                .store
                .lane_jobs_of_repository(repository_id, 10)?
                .into_iter()
                .filter(|job| self.sees_job(sight, job))
                .collect(),
        })
    }

    /// A person assigns the lane a repository uses (or none). Recorded as an `edit_lane` decision. An agent does not: the
    /// lane runs what a pull request brings, which is the owner's to allow.
    pub fn edit_repository_lane(
        &self,
        by: &Actor,
        repository_id: i64,
        lane: Option<String>,
        reason: &str,
    ) -> Result<RepositoryLane> {
        if !by.is_person() {
            return Err(AppError::Forbidden(
                "only a person assigns a test lane to a repository".into(),
            ));
        }
        let repository = self.repository(repository_id)?;
        let lane = lane.map(|l| l.trim().to_string()).filter(|l| !l.is_empty());
        if let Some(name) = &lane
            && self.lanes.lane(name).is_none()
        {
            return Err(AppError::Invalid(format!(
                "{name:?} is no lane of the server's configuration ({})",
                self.lanes
                    .all()
                    .iter()
                    .map(|l| l.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if self.store.repository_lane(repository_id)? != lane {
            self.record_action(
                by,
                "edit_lane",
                None,
                None,
                BTreeMap::from([
                    ("repository".into(), repository.name.clone()),
                    ("lane".into(), lane.clone().unwrap_or_default()),
                ]),
                reason,
            )?;
            self.store
                .set_repository_lane(repository_id, lane.as_deref())?;
            self.bus.publish(crate::bus::Event::RepositoryUpdated(
                self.repository(repository_id)?,
            ));
        }
        self.repository_lane_view(repository_id, &self.sight(by)?)
    }

    /// The lane's report of a card.
    pub fn card_lane(&self, card_id: i64) -> Result<CardLane> {
        let card = self.card(card_id)?;
        let lane = self.store.repository_lane(card.repository_id)?;
        let change = self.lane_change(&card)?;
        let jobs = self.store.lane_jobs_of_card(card_id)?;
        // A change that was merged or closed has no open head: its last job's change has the head it ended with, so that
        // the measurement that justified a merge stays readable.
        let head = match &change {
            Some(change) => change.head.clone(),
            None => jobs
                .last()
                .and_then(|job| self.store.change(job.change_id).ok().flatten())
                .and_then(|change| change.head),
        };
        let current = jobs.iter().any(|j| {
            Some(&j.head) == head.as_ref()
                && matches!(
                    j.state.as_str(),
                    "queued" | "running" | "measured" | "passed" | "failed"
                )
        });
        let waiting = match (&lane, &change, &head) {
            (Some(_), Some(change), Some(head)) if !current => {
                let repository = self.repository(card.repository_id)?;
                self.lane_blocker(&card, change, &repository, head)?
            }
            _ => None,
        };
        Ok(CardLane {
            lane,
            jobs,
            head,
            waiting,
        })
    }
}
