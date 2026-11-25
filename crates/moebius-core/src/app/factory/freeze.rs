//! Freezing a repository (ADR 0031): the owner's brake on the factory. A frozen repository starts no review round,
//! merges nothing (not even for a person, who unfreezes it first) and starts no agent run. The owner gets an Inbox
//! item, and the card window says "Frozen: <reason>".
//!
//! A repository is frozen by
//!
//! - a person, with `POST /v1/repositories/{id}/freeze` (`manual`);
//! - the label `factory:freeze` on any open issue of the repository, which the poll reads (`label`): the freeze lasts
//!   as long as a label is there, and the person who takes it off has lifted it;
//! - a breaker (`automatic`): two merged changes reverted within seven days, more than 80% of the day's token budget
//!   used, or a failed nightly check ([`App::freeze_for_failed_nightly`] is the hook the nightly check calls).
//!
//! What a person or a breaker froze stays frozen until a person unfreezes it. A breaker trips once for what tripped
//! it (a day's budget, a new revert), so that unfreezing is not undone at the next poll while the cause still stands.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::{FACTORY_FROZEN, FREEZE_LABEL};
use crate::app::{App, AppError, Result, review_context};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{Frozen, Repository, now};

/// How long a revert counts, and how many of them freeze a repository.
const REVERT_WINDOW: i64 = 7 * 24 * 60 * 60;
const REVERTS_THAT_FREEZE: i64 = 2;
/// The share of the day's token budget, in percent, above which every repository on a forge freezes.
const BUDGET_PERCENT: i64 = 80;

static REVERTS_COMMIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"This reverts commit ([0-9a-f]{40})").expect("the pattern is valid")
});
static REVERTS_REQUEST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Reverts [\w./-]*#(\d+)").expect("the pattern is valid"));

impl App {
    /// A person (or the Moebius agent) freezes the repository by hand. Freezing a frozen repository changes nothing.
    pub fn freeze_repository(&self, by: &Actor, id: i64, reason: &str) -> Result<Repository> {
        let repository = self.repository(id)?;
        let reason = match reason.trim() {
            "" => format!("frozen by {}", by.decider()),
            reason if reason.len() > 300 || reason.contains('\n') => {
                return Err(AppError::Invalid(
                    "the reason is one line of at most 300 characters".into(),
                ));
            }
            reason => reason.to_string(),
        };
        self.freeze(&repository, "manual", &reason, by)
    }

    /// A person lifts the freeze. Refused for an agent or a script, and while the label `factory:freeze` is still on an
    /// issue (the label would freeze it again at the next poll).
    pub fn unfreeze_repository(&self, by: &Actor, id: i64, reason: &str) -> Result<Repository> {
        if !by.is_person() {
            return Err(AppError::Forbidden(
                "only a person unfreezes a repository".into(),
            ));
        }
        let repository = self.repository(id)?;
        if repository.frozen.is_none() {
            return Ok(repository);
        }
        if let Some(number) = self.label_on(&repository)? {
            return Err(AppError::Conflict(format!(
                "the label {FREEZE_LABEL} is on #{number}: take it off first, and the freeze lifts"
            )));
        }
        self.lift(&repository, by, reason)
    }

    /// Freezes the repository for a breaker. True when it was not frozen before.
    pub(crate) fn freeze_automatically(
        &self,
        repository: &Repository,
        source: &str,
        reason: &str,
    ) -> Result<bool> {
        let by = Actor::Script {
            column: "factory".into(),
        };
        let was = self.repository(repository.id)?.frozen.is_some();
        self.freeze(repository, source, reason, &by)?;
        Ok(!was)
    }

    /// The hook for a nightly check that failed: freezes the repository with the check's reason, so that nothing is
    /// merged on top of a repository whose own nightly run is red.
    pub fn freeze_for_failed_nightly(&self, repository_id: i64, why: &str) -> Result<bool> {
        let repository = self.repository(repository_id)?;
        self.freeze_automatically(
            &repository,
            "automatic",
            &format!("the nightly check failed: {}", why.trim()),
        )
    }

    fn freeze(
        &self,
        repository: &Repository,
        source: &str,
        reason: &str,
        by: &Actor,
    ) -> Result<Repository> {
        if !self
            .store
            .freeze_repository(repository.id, source, reason)?
        {
            return self.repository(repository.id);
        }
        self.record_action(
            by,
            "freeze_repository",
            None,
            None,
            BTreeMap::from([
                ("repository".into(), repository.name.clone()),
                ("source".into(), source.into()),
            ]),
            reason,
        )?;
        self.store.add_inbox_item(
            FACTORY_FROZEN,
            None,
            &format!("{} is frozen", repository.name),
            &serde_json::json!({
                "repository": repository.name,
                "repository_id": repository.id,
                "reason": reason,
                "source": source,
            }),
        )?;
        let repository = self.repository(repository.id)?;
        self.bus
            .publish(Event::RepositoryUpdated(repository.clone()));
        Ok(repository)
    }

    fn lift(&self, repository: &Repository, by: &Actor, reason: &str) -> Result<Repository> {
        self.store.unfreeze_repository(repository.id)?;
        self.record_action(
            by,
            "unfreeze_repository",
            None,
            None,
            BTreeMap::from([("repository".into(), repository.name.clone())]),
            reason,
        )?;
        for item in self.store.inbox()? {
            if item.kind == FACTORY_FROZEN && item.data["repository_id"] == repository.id {
                self.store.resolve_inbox_item(item.id)?;
            }
        }
        let repository = self.repository(repository.id)?;
        self.bus
            .publish(Event::RepositoryUpdated(repository.clone()));
        Ok(repository)
    }

    /// The number of an open issue of the repository that carries the label `factory:freeze`, if one does.
    fn label_on(&self, repository: &Repository) -> Result<Option<i64>> {
        Ok(self
            .store
            .cards(repository.id)?
            .into_iter()
            .filter(|card| !card.closed && card.archived_at.is_none())
            .find(|card| {
                card.labels
                    .iter()
                    .any(|label| label.eq_ignore_ascii_case(FREEZE_LABEL))
            })
            .map(|card| card.number))
    }

    /// Why the repository is frozen, if it is.
    pub fn frozen(&self, repository_id: i64) -> Result<Option<Frozen>> {
        Ok(self.repository(repository_id)?.frozen)
    }

    /// Follows the label `factory:freeze` as the poll just read the issues: an issue that has it freezes the
    /// repository, and a freeze the label made lifts when no issue has it.
    pub fn sync_freeze_label(&self, repository: &Repository) -> Result<()> {
        let repository = self.repository(repository.id)?;
        match (self.label_on(&repository)?, &repository.frozen) {
            (Some(number), None) => {
                self.freeze_automatically(
                    &repository,
                    "label",
                    &format!("the label {FREEZE_LABEL} is on #{number}"),
                )?;
            }
            (None, Some(frozen)) if frozen.source == "label" => {
                let by = Actor::Script {
                    column: "factory".into(),
                };
                self.lift(
                    &repository,
                    &by,
                    &format!("the label {FREEZE_LABEL} is gone"),
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    /// What the poll does for the factory after it read a repository: follows the label, looks for reverts of merged
    /// changes, and checks the day's token budget.
    pub fn factory_poll(&self, repository: &Repository) -> Result<()> {
        self.sync_freeze_label(repository)?;
        self.note_reverts(repository)?;
        self.check_token_budget(repository)
    }

    /// Sets the day's token budget the breaker reads (`budgets.day_tokens`); none is no breaker.
    pub fn set_day_budget(&self, tokens: Option<i64>) {
        *self.day_budget.lock().unwrap_or_else(|e| e.into_inner()) = tokens;
    }

    /// Freezes the repository when more than 80% of the day's token budget is used. Trips once a day.
    pub fn check_token_budget(&self, repository: &Repository) -> Result<()> {
        let Some(budget) = *self.day_budget.lock().unwrap_or_else(|e| e.into_inner()) else {
            return Ok(());
        };
        let used = self.store.tokens_today()?;
        if used.saturating_mul(100) <= budget.saturating_mul(BUDGET_PERCENT) {
            return Ok(());
        }
        let day = format!("tokens:{}", now() / 86_400);
        if !self.store.trip_breaker(repository.id, "tokens", &day)? {
            return Ok(());
        }
        self.freeze_automatically(
            repository,
            "automatic",
            &format!(
                "more than {BUDGET_PERCENT}% of the day's token budget is used ({used} of {budget} tokens)"
            ),
        )?;
        Ok(())
    }

    /// Looks in the last week of the default branch for commits that revert a merged change (a revert commit, which
    /// says `This reverts commit <sha>` of the change's merge, or a merged revert request, whose text says `Reverts
    /// #<number>`), keeps each once, and freezes the repository when two different changes were reverted in seven days.
    pub fn note_reverts(&self, repository: &Repository) -> Result<()> {
        if repository.forge == "none" {
            return Ok(());
        }
        let log = review_context::read_git(
            Path::new(&repository.path),
            &[
                "log",
                "-n",
                "300",
                "--since=7.days.ago",
                "--format=%H%x1f%ct%x1f%B%x1e",
                &repository.base_ref(),
            ],
            1_000_000,
        );
        // A clone that has no history yet (or a branch it lacks) has nothing to say about reverts.
        let Ok(log) = log else {
            return Ok(());
        };
        let mut newest = None;
        for record in log.split('\x1e') {
            let mut fields = record.trim_start().splitn(3, '\x1f');
            let (Some(commit), Some(at), Some(body)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let at: i64 = at.trim().parse().unwrap_or_default();
            let mut reverted = Vec::new();
            for found in REVERTS_COMMIT.captures_iter(body) {
                reverted.extend(
                    self.store
                        .merged_change_of_commit(repository.id, &found[1])?,
                );
            }
            for found in REVERTS_REQUEST.captures_iter(body) {
                if let Ok(number) = found[1].parse::<i64>() {
                    reverted.extend(self.store.merged_change_of_request(repository.id, number)?);
                }
            }
            for change in reverted {
                if self.store.add_revert(repository.id, change, commit, at)? {
                    newest = Some(commit.to_string());
                }
            }
        }
        let Some(newest) = newest else {
            return Ok(());
        };
        let count = self
            .store
            .reverted_since(repository.id, now() - REVERT_WINDOW)?;
        if count >= REVERTS_THAT_FREEZE
            && self
                .store
                .trip_breaker(repository.id, "reverts", &format!("revert:{newest}"))?
        {
            self.freeze_automatically(
                repository,
                "automatic",
                &format!("{count} merged changes were reverted within 7 days"),
            )?;
        }
        Ok(())
    }
}
