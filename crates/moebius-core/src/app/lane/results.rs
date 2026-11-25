//! What the end of a job does (ADR 0035), and what a new push does to the jobs of the old head.
//!
//! - **A step failed**: the card goes back to the author's turn (a change of Moebius's own to In progress with the
//!   failure as its note, a pull request someone else opened to In review) and one plain reply on the pull request names
//!   the step and gives the last lines of its output, redacted and cut. Output is data and is only ever quoted.
//! - **A measurement came of it**: it is kept as a measurement of the change at its head (`measurement_reports`, with the
//!   machine and every run) and starts the next round of the Reviewer, the path a measurer's comment takes, so that the
//!   Reviewer judges it against the review brief; an `approve` accepts it for the head and merges through the gate.
//! - **The lane failed**: three jobs in a row that ended for a reason of the lane's pause it and tell the owner.
//! - **A new push** cancels the queued and running jobs of the change and makes the results of the old head count for
//!   nothing.
//!
//! Handing a result on is the server acting for the owner, so a hold, a pause and a freeze delay it: the pass tries again.

use serde_json::{Value, json};

use super::report::failure_reply;
use super::{LANE_PAUSED, MEASURER_PREFIX};
use crate::app::{App, ROUND_LIMIT, Result, columns};
use crate::decide::Actor;
use crate::store::{Card, Change, INFRASTRUCTURE_FAILURES, LaneJob, Repository};

fn lane_actor(lane: &str) -> Actor {
    Actor::Script {
        column: format!("lane {lane}"),
    }
}

fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

impl App {
    /// Writes down how `job` ended and hands the result on.
    pub(super) fn lane_job_ended(
        &self,
        job: &LaneJob,
        state: &str,
        failed_step: Option<&str>,
        summary: &str,
        report: &Value,
    ) {
        let summary = crate::blobs::redact(summary);
        match self
            .store
            .finish_lane_job(job.id, state, failed_step, &summary, report)
        {
            Ok(true) => {}
            // It was cancelled meanwhile: the cancellation stands.
            _ => return,
        }
        let card = self.store.card(job.card_id).ok().flatten();
        let _ = self.store.note_activity(
            "lane",
            Some(job.repository_id),
            card.as_ref().map(|c| c.number),
            Some(&job.lane),
            &format!("{state}: {summary}"),
            &json!({
                "job": job.id, "state": state, "lane": job.lane, "head": job.head,
                "card": job.card_id, "title": card.as_ref().map(|c| c.title.clone()),
            }),
        );
        if state == "errored" {
            self.pause_a_failing_lane(job, &summary);
        }
        if let Err(e) = self.deliver_lane_job(job.id) {
            self.bus.publish(crate::bus::Event::ForgeError {
                repository: String::new(),
                message: format!("the result of test lane job {}: {e}", job.id),
            });
        }
        if let Ok(card) = self.card(job.card_id) {
            self.bus.publish(crate::bus::Event::CardUpdated(card));
        }
    }

    /// Pauses the lane when its last [`INFRASTRUCTURE_FAILURES`] jobs ended for a reason of its own, and tells the owner.
    fn pause_a_failing_lane(&self, job: &LaneJob, why: &str) {
        let Ok(streak) = self.store.lane_infrastructure_streak(&job.lane) else {
            return;
        };
        if streak < INFRASTRUCTURE_FAILURES {
            return;
        }
        let reason =
            format!("{streak} jobs in a row ended because of the lane itself; the last: {why}");
        self.pause_lane_for(&job.lane, &reason, Some(job.card_id));
    }

    /// Pauses the lane for `reason`, once, and tells the owner in the Inbox and the digest.
    pub(super) fn pause_lane_for(&self, lane: &str, reason: &str, card_id: Option<i64>) {
        if !self
            .store
            .pause_lane(lane, reason, card_id)
            .unwrap_or(false)
        {
            return;
        }
        // The reason of a pause that a card's job made may quote what that job printed: it stays in the pause record, which
        // is shown by the card's own sight, and the Inbox and the log say only that the lane stopped.
        let said = match card_id {
            Some(_) => format!("Test lane {lane} stopped itself; open the lane to see why"),
            None => reason.to_string(),
        };
        let _ = self.store.add_inbox_item(
            LANE_PAUSED,
            None,
            &format!("Test lane {lane} paused itself"),
            &json!({ "lane": lane, "reason": said }),
        );
        for repository in self.store.lane_repositories(lane).unwrap_or_default() {
            let _ = self.store.note_activity(
                "lane_paused",
                Some(repository),
                None,
                Some(lane),
                &said,
                &json!({ "lane": lane, "card": card_id }),
            );
        }
    }

    /// [`App::pause_lane_for`], for the tests of other modules (which need a Unix program to run).
    #[cfg(all(test, unix))]
    pub(crate) fn pause_lane_for_test(&self, lane: &str, reason: &str, card_id: Option<i64>) {
        self.pause_lane_for(lane, reason, card_id);
    }

    /// A person lifts the pause of a lane, and its count of failures starts over.
    pub fn resume_lane(&self, by: &Actor, name: &str, reason: &str) -> Result<bool> {
        if !by.is_person() {
            return Err(crate::app::AppError::Forbidden(
                "only a person resumes a lane that stopped itself".into(),
            ));
        }
        if self.lanes.lane(name).is_none() {
            return Err(crate::app::AppError::NotFound(format!("lane {name}")));
        }
        let resumed = self.store.resume_lane(name)?;
        if resumed {
            self.record_action(
                by,
                "resume_lane",
                None,
                None,
                std::collections::BTreeMap::from([("lane".into(), name.to_string())]),
                reason,
            )?;
            for item in self.store.inbox()? {
                if item.kind == LANE_PAUSED && item.data["lane"] == name {
                    self.store.resolve_inbox_item(item.id)?;
                }
            }
        }
        Ok(resumed)
    }

    /// Hands on what came of every job that ended and was not handed on yet.
    pub(super) fn deliver_lane_results(&self) -> Result<()> {
        for job in self.store.undelivered_lane_jobs()? {
            self.deliver_lane_job(job.id)?;
        }
        Ok(())
    }

    /// Hands on what came of one job, once.
    fn deliver_lane_job(&self, id: i64) -> Result<()> {
        let Some(job) = self.store.lane_job(id)? else {
            return Ok(());
        };
        if job.delivered || !matches!(job.state.as_str(), "measured" | "passed" | "failed") {
            return Ok(());
        }
        let (Some(card), Some(change)) = (
            self.store.card(job.card_id)?,
            self.store.change(job.change_id)?,
        ) else {
            return Ok(self.store.set_lane_job_delivered(id)?);
        };
        // The result is of one commit: a push since makes it count for nothing, and a closed change has no use for it.
        if change.state != "open" || change.head.as_deref() != Some(job.head.as_str()) {
            return Ok(self.store.set_lane_job_delivered(id)?);
        }
        // What the server writes for the owner waits for a hold, a pause and a freeze to be lifted.
        if !self.may_act(card.id)? {
            return Ok(());
        }
        let repository = self.repository(card.repository_id)?;
        let handed = match job.state.as_str() {
            "failed" => self.deliver_failure(&job, &card, &change, &repository)?,
            "measured" => self.deliver_measurement(&job, &card, &change, &repository)?,
            _ => true,
        };
        if handed {
            self.store.set_lane_job_delivered(id)?;
            self.bus
                .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
        }
        Ok(())
    }

    /// The failing step, answered on the pull request in one reply, and the card sent back to the author.
    fn deliver_failure(
        &self,
        job: &LaneJob,
        card: &Card,
        change: &Change,
        repository: &Repository,
    ) -> Result<bool> {
        let failure = &job.report["failure"];
        let reply = failure_reply(
            &job.lane,
            &job.head,
            failure["step"].as_str().unwrap_or("a step"),
            failure["why"].as_str().unwrap_or("it failed"),
            failure["output"].as_str().unwrap_or_default(),
        );
        let by = lane_actor(&job.lane);
        match (change.pull_request, repository.answer_on_forge) {
            (Some(number), true) => {
                let written = self.forge(repository).and_then(|forge| {
                    Ok(forge.comment_issue(repository, number, &crate::blobs::redact(&reply))?)
                });
                if let Err(e) = written {
                    self.bus.publish(crate::bus::Event::ForgeError {
                        repository: repository.name.clone(),
                        message: format!("pull request #{number}: the test lane's reply: {e}"),
                    });
                }
            }
            (None, true) => {
                // The change is not offered yet: the issue is where the author reads.
                let _ = self.comment_issue(&by, card.id, &reply);
            }
            _ => {}
        }
        // The author's turn: for the server's own change its coder, with the failure as the note.
        let own = change.outside_author.is_none();
        let to = if own {
            columns::IN_PROGRESS
        } else {
            columns::IN_REVIEW
        };
        let now = self.card(card.id)?;
        if now.column != to && self.column_class(now.project_id, &now.column) != columns::DONE {
            let note = format!(
                "The test lane {} stopped at the step {}: {}.",
                job.lane,
                failure["step"].as_str().unwrap_or("a step"),
                failure["why"].as_str().unwrap_or("it failed")
            );
            let note = if own {
                format!(
                    "{note}\n\n{}",
                    failure["output"].as_str().unwrap_or_default()
                )
            } else {
                note
            };
            self.move_card_from(
                card.id,
                &now.column,
                to,
                own.then_some(note.as_str()),
                Some(&by),
            )?;
        }
        Ok(true)
    }

    /// The measurement of a lane as the measurement of the change at its head, and the next round of the Reviewer with it.
    /// False when it has to wait: a round is going, or the head's code is not the one that was approved.
    fn deliver_measurement(
        &self,
        job: &LaneJob,
        card: &Card,
        change: &Change,
        repository: &Repository,
    ) -> Result<bool> {
        // Only a followed pull request has a Reviewer's round that reads a measurement.
        if change.outside_author.is_none() {
            return Ok(true);
        }
        let rounds = self.store.external_reviews(change.id)?;
        let waiting = rounds
            .iter()
            .rev()
            .find(|r| r.kind != "person")
            .is_some_and(|r| r.state == "awaiting_measurement" && r.head == job.head);
        if !waiting {
            // Nothing waits for it: the code was approved with no measurement asked, or is not approved at this head.
            return Ok(true);
        }
        let going = rounds.iter().any(|r| {
            matches!(
                r.state.as_str(),
                "wanted" | "starting" | "running" | "approved" | "posting"
            )
        });
        if going {
            return Ok(false);
        }
        if self.store.external_review_rounds(change.id)? >= ROUND_LIMIT {
            self.ask_owner_to_look(card, change)?;
            return Ok(true);
        }
        let body = crate::blobs::redact(job.report["text"].as_str().unwrap_or_default());
        let body: String = body.chars().take(super::report::TEXT_AT_MOST).collect();
        let measurer = format!("{MEASURER_PREFIX}{}", job.lane);
        // The comment of a measurement of a lane is the job: negative, so that no comment of the forge has its number.
        let Some(report) = self.store.add_measurement_report(
            change.id,
            (&job.head, &job.base),
            &measurer,
            -job.id,
            &body,
        )?
        else {
            return Ok(true);
        };
        self.store
            .add_measurement_review(change.id, &job.head, report)?;
        self.store.note_activity(
            "measurement_read",
            Some(repository.id),
            Some(card.number),
            Some(&measurer),
            &format!(
                "measurement of {} read from the test lane",
                short(&job.head)
            ),
            &json!({ "change": change.id, "head": job.head, "card": card.id }),
        )?;
        self.answer(
            repository,
            card.id,
            &format!(
                "The test lane {} built, tested and measured {}. The Reviewer reads the numbers with the pull request once its \
                 checks have passed; nobody needs to write /done.",
                job.lane,
                short(&job.head)
            ),
        );
        Ok(true)
    }

    /// The change has a new head: the jobs of the old one that are queued or running are cancelled, and what came of the
    /// ones that ended counts for nothing.
    pub(crate) fn lane_head_moved(&self, change: &Change, head: &str) -> Result<()> {
        let cancelled = self.store.cancel_lane_jobs(
            change.id,
            Some(head),
            "the pull request has a new commit, so the job of the old one counts for nothing",
        )?;
        self.store.set_lane_results_invalid(change.id, head)?;
        if !cancelled.is_empty()
            && let Some(card) = change.card_id.map(|id| self.card(id)).transpose()?
        {
            self.bus.publish(crate::bus::Event::CardUpdated(card));
        }
        Ok(())
    }

    /// Cancels the jobs whose change is closed or has another head than the job's: a push the poll saw.
    pub(super) fn cancel_moved_lane_jobs(&self) -> Result<()> {
        for job in self.store.lane_active()? {
            let open_head = self
                .store
                .change(job.change_id)?
                .filter(|c| c.state == "open")
                .and_then(|c| c.head);
            if open_head.as_deref() != Some(job.head.as_str()) {
                self.store.cancel_lane_jobs(
                    job.change_id,
                    open_head.as_deref(),
                    "the change was closed or has a new commit",
                )?;
            }
        }
        Ok(())
    }
}
