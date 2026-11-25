//! Queueing a job (ADR 0035): for a change at its head, once the Reviewer has said the code is right, and never while
//! something the owner holds back says no.
//!
//! A job is asked for when a card enters a column marked `lane = true` (`App::lane_entered`, called wherever a card
//! moves), by a column script (`moebius.lane_job()`), by a person, and by the lane's own pass for the cards that sit in
//! such a column and had to wait. The same checks hold when the job is about to start, since a freeze or a hold may
//! have come while it waited.

use crate::app::{App, AppError, Result, columns};
use crate::repos::git;
use crate::store::{Card, Change, LaneJob, Repository};

/// How long after a job errored the same head is left alone before it is queued again, in seconds.
const RETRY_AFTER: i64 = 600;

/// What asking for a job came to: the job, or why there is none.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LaneRequest {
    pub job: Option<LaneJob>,
    /// Why no job was queued, or that one is there already.
    pub why: Option<String>,
}

impl LaneRequest {
    fn blocked(why: impl Into<String>) -> Self {
        LaneRequest {
            job: None,
            why: Some(why.into()),
        }
    }
}

impl App {
    /// The change of `card` a lane job is for: the pull request it follows, else its own repository's open change.
    pub(super) fn lane_change(&self, card: &Card) -> Result<Option<Change>> {
        if let Some(change) = self.followed_change(card)? {
            return Ok(Some(change));
        }
        Ok(self
            .open_changes(card)?
            .into_iter()
            .find(|c| c.repository_id == Some(card.repository_id)))
    }

    /// Whether the Reviewer has approved the code of `head` of `change`: a followed pull request needs a round of the
    /// Reviewer for that head that ended `approve` or `approve_code`; a change of Moebius's own needs the card's latest
    /// review to approve.
    fn lane_reviewed(&self, card: &Card, change: &Change, head: &str) -> Result<bool> {
        if change.outside_author.is_some() {
            return Ok(self.store.external_reviews(change.id)?.iter().any(|r| {
                r.kind != "person"
                    && r.head == head
                    && matches!(r.verdict.as_deref(), Some("approve" | "approve_code"))
                    && !matches!(r.state.as_str(), "stale" | "failed")
            }));
        }
        let runs = self.store.runs_for_card(card.id)?;
        Ok(crate::board::latest_review(&runs).is_some_and(|review| review.verdict == "approve"))
    }

    /// Why no job may start for `change` of `card` now, as a sentence; none when one may. A job that is queued stays
    /// queued while this says something.
    pub(super) fn lane_blocker(
        &self,
        card: &Card,
        change: &Change,
        repository: &Repository,
        head: &str,
    ) -> Result<Option<String>> {
        if let Some(frozen) = &repository.frozen {
            return Ok(Some(format!("the repository is frozen: {}", frozen.reason)));
        }
        if self.runs_paused()?.is_some() {
            return Ok(Some("all agents are paused".into()));
        }
        if card.held.is_some() {
            return Ok(Some("the card is held".into()));
        }
        if change.state != "open" || change.head.as_deref() != Some(head) {
            return Ok(Some("the change has another head now".into()));
        }
        if !self.lane_reviewed(card, change, head)? {
            return Ok(Some(
                "the Reviewer has not approved the code of this version yet, and the review comes first".into(),
            ));
        }
        match self.merge_paths(repository, change) {
            Err(why) => Ok(Some(format!(
                "the paths of the change could not be read, so it cannot be checked against the locked paths: {why}"
            ))),
            Ok(files) => Ok(Self::locked_hit(repository, &files).map(|(file, pattern)| {
                    format!(
                        "the change touches {file}, which is locked by {pattern}: no job runs for it until a person has looked"
                    )
                })),
        }
    }

    /// Asks for a job for the change of `card_id` at its head. A job is queued, or the answer says why not: no lane is
    /// assigned to the repository, the project has no valid `lane.toml`, a hold, freeze or pause, a locked path, no
    /// review yet, or a job for this head that counts is there already.
    pub fn request_lane_job(&self, card_id: i64) -> Result<LaneRequest> {
        let card = self.card(card_id)?;
        let repository = self.repository(card.repository_id)?;
        let Some(name) = self.store.repository_lane(repository.id)? else {
            return Ok(LaneRequest::blocked("this repository uses no test lane"));
        };
        let Some(lane) = self.lanes.lane(&name) else {
            return Ok(LaneRequest::blocked(format!(
                "the lane {name} is not in the server's configuration"
            )));
        };
        let Some(change) = self.lane_change(&card)? else {
            return Ok(LaneRequest::blocked("the card has no open change"));
        };
        let Some(head) = change.head.clone() else {
            return Ok(LaneRequest::blocked(
                "the head of the change is not known yet",
            ));
        };
        if let Some(why) = self.lane_blocker(&card, &change, &repository, &head)? {
            return Ok(LaneRequest::blocked(why));
        }
        if let Err(why) = self.lane_definition_of(card.id) {
            return Ok(LaneRequest::blocked(why));
        }
        let base = git(
            std::path::Path::new(&repository.path),
            &["merge-base", &repository.base_ref(), &head],
        )
        .map_err(|e| {
            AppError::Conflict(format!(
                "the merge base of the change could not be read: {e}"
            ))
        })?;
        let Some(id) = self.store.add_lane_job(
            repository.id,
            card.id,
            change.id,
            &lane.name,
            &head,
            base.trim(),
            change.outside_author.is_some(),
            RETRY_AFTER,
        )?
        else {
            let latest = self
                .store
                .lane_jobs_of_change(change.id)?
                .into_iter()
                .rfind(|j| j.head == head);
            return Ok(LaneRequest {
                why: Some("a job for this version is there already".into()),
                job: latest,
            });
        };
        let job = self.store.lane_job(id)?;
        self.bus
            .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
        Ok(LaneRequest { job, why: None })
    }

    /// A card moved into `column`: a column marked `lane = true` of its project queues a job, as far as the checks allow;
    /// what they refuse is tried again by the lane's pass. Never fails the move.
    pub(crate) fn lane_entered(&self, card_id: i64, column: &str) {
        let Ok(card) = self.card(card_id) else {
            return;
        };
        let Ok(board) = self.project_columns_of(card.project_id) else {
            return;
        };
        let marked = board
            .column(column)
            .is_some_and(|c| c.lane && c.counts_as == columns::IN_REVIEW);
        if marked && let Err(e) = self.request_lane_job(card_id) {
            self.bus.publish(crate::bus::Event::ForgeError {
                repository: String::new(),
                message: format!("the test lane for card {card_id}: {e}"),
            });
        }
    }

    /// The lane's pass, which the server runs every few seconds: cards waiting in a lane column get a job once the checks
    /// allow, jobs for a head the change left are cancelled, results are handed on, and the queue moves. The jobs that
    /// start are returned; each is to be run with [`App::run_lane_job`] on a thread of its own.
    pub fn lane_pass(&self) -> Vec<i64> {
        if self.lanes.all().is_empty() {
            return Vec::new();
        }
        let report = |what: &str, outcome: Result<()>| {
            if let Err(e) = outcome {
                self.bus.publish(crate::bus::Event::ForgeError {
                    repository: String::new(),
                    message: format!("the test lane, {what}: {e}"),
                });
            }
        };
        report("looking for cards", self.queue_waiting_cards());
        report(
            "cancelling the jobs of old heads",
            self.cancel_moved_lane_jobs(),
        );
        report("handing on results", self.deliver_lane_results());
        self.start_lane_jobs().unwrap_or_default()
    }

    /// Cards in a column marked `lane = true` that have no job for their head get one.
    fn queue_waiting_cards(&self) -> Result<()> {
        for repository in self.store.repositories()? {
            if self.store.repository_lane(repository.id)?.is_none() {
                continue;
            }
            for card in self.store.cards(repository.id)? {
                if card.closed || card.archived_at.is_some() {
                    continue;
                }
                let board = self.project_columns_of(card.project_id)?;
                if board.column(&card.column).is_some_and(|c| c.lane) {
                    self.request_lane_job(card.id)?;
                }
            }
        }
        Ok(())
    }

    /// Takes the next job of each lane that is free, as far as the checks allow now. The ids of the jobs that started.
    pub fn start_lane_jobs(&self) -> Result<Vec<i64>> {
        let mut started = Vec::new();
        for lane in self.lanes.all() {
            if self.store.lane_pause(&lane.name)?.is_some() {
                continue;
            }
            let queue = self.store.lane_queue(&lane.name)?;
            if queue.iter().any(|job| job.state == "running") {
                continue;
            }
            for job in queue {
                let (Some(card), Some(change)) = (
                    self.store.card(job.card_id)?,
                    self.store.change(job.change_id)?,
                ) else {
                    self.store.cancel_lane_jobs(
                        job.change_id,
                        None,
                        "the card or the change is gone",
                    )?;
                    continue;
                };
                let repository = self.repository(job.repository_id)?;
                if self
                    .lane_blocker(&card, &change, &repository, &job.head)?
                    .is_some()
                {
                    continue;
                }
                if self.store.claim_lane_job(job.id)? {
                    self.bus
                        .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
                    started.push(job.id);
                }
                break;
            }
        }
        Ok(started)
    }
}
