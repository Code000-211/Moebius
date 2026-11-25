//! The merge gate (ADR 0031): the one place that decides whether the server may merge a change, whichever path it came
//! by: the Reviewer's approval (`merge_reviewed`), a person's Merge, a column script's `moebius.merge_change`, a card's
//! merge from Ready to merge and the desktop API's merge route all end in `App::merge_one`, which asks it first.
//!
//! The gate refuses with a reason and never fails open: what it cannot find out (the paths a change touches, the rules)
//! is a refusal. In this order it checks:
//!
//! 1. the repository is not frozen;
//! 2. for what the server does on its own account (a script or an agent; a person's own Merge is their decision): all
//!    agents are not paused and the card is not held;
//! 3. the checks of the head have passed;
//! 4. a measurement is accepted for the head, or the issue carries `no-measurement`, when the repository requires one;
//! 5. for the same actors: no path the change touches is locked (see `paths`), which only a person merges;
//! 6. the repository's rules, as a decision of the kind `merge_check`, decided like a tool call.
//!
//! A refusal puts the card in Ready to merge with an Inbox item `merge_refused` that names the reason (and the item
//! `ready_to_merge`, which offers a person the merge, if the card has none), and the merge fails with
//! `AppError::Refused`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::json;

use super::paths;
use super::{MERGE_CHECK, MERGE_REFUSED, NO_MEASUREMENT_LABEL};
use crate::app::{App, AppError, Result, columns, review_context};
use crate::decide::rules;
use crate::decide::{Actor, Question};
use crate::store::{Card, Change, Repository};

/// A refusal: `Ok` when the gate lets the merge through, else why it does not.
pub(crate) type Gate = std::result::Result<(), String>;

impl App {
    /// Where the server's own rules are (`rules_file` of the configuration), for the gate's decision.
    pub fn set_rules_file(&self, path: Option<PathBuf>) {
        *self.rules_file.lock().unwrap_or_else(|e| e.into_inner()) = path;
    }

    fn rules_file(&self) -> Option<PathBuf> {
        self.rules_file
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Asks the gate whether `by` may merge `change` now. A refusal is also written where the owner will see it: the
    /// card goes to Ready to merge with an inbox item that gives the reason. A change of the harness configuration
    /// repository belongs to no forge and has no repository's rules to hold to.
    pub(in crate::app) fn merge_gate(&self, by: &Actor, change: &Change) -> Result<()> {
        let Some(repository_id) = change.repository_id else {
            return Ok(());
        };
        let repository = self.repository(repository_id)?;
        let card = change.card_id.map(|id| self.card(id)).transpose()?;
        match self.gate_says(by, change, &repository, card.as_ref())? {
            Ok(()) => Ok(()),
            Err(reason) => {
                self.merge_refused(change, card.as_ref(), &repository, &reason)?;
                Err(AppError::Refused(reason))
            }
        }
    }

    /// The gate's answer, without writing anything.
    pub(crate) fn gate_says(
        &self,
        by: &Actor,
        change: &Change,
        repository: &Repository,
        card: Option<&Card>,
    ) -> Result<Gate> {
        if let Some(frozen) = &repository.frozen {
            return Ok(Err(format!(
                "the repository is frozen: {}. A person unfreezes it",
                frozen.reason
            )));
        }
        if !by.is_person() {
            if self.runs_paused()?.is_some() {
                return Ok(Err("all agents are paused".into()));
            }
            if card.is_some_and(|card| card.held.is_some()) {
                return Ok(Err("the card is held".into()));
            }
        }
        let number = change.pull_request.unwrap_or_default();
        let unknown = change.checks_head != change.head;
        let outside = change.outside_author.is_some();
        let failing = matches!(change.checks.as_deref(), Some("pending" | "failure"));
        // A pull request someone else opened is merged only once its checks have passed for its current head; for the
        // server's own work, checks that are known for the head and have not passed hold it back too (a person decides
        // about their own).
        if (outside && (failing || unknown)) || (!by.is_person() && failing && !unknown) {
            return Ok(Err(format!(
                "the checks of pull request #{number} have not passed"
            )));
        }
        let measured = self.measured(change)?;
        if repository.require_measurement && !measured {
            let labelled = card.is_some_and(|card| {
                card.labels
                    .iter()
                    .any(|label| label.eq_ignore_ascii_case(NO_MEASUREMENT_LABEL))
            });
            if !labelled {
                return Ok(Err(format!(
                    "this repository requires a measurement, and none is accepted for this version of the change; \
                     accept one, or label the issue {NO_MEASUREMENT_LABEL}"
                )));
            }
        }
        let files = self.merge_paths(repository, change);
        if !by.is_person() {
            match &files {
                Err(why) => {
                    return Ok(Err(format!(
                        "the paths of the change could not be read, so it cannot be checked against the locked paths: {why}"
                    )));
                }
                Ok(files) => {
                    let harness =
                        card.map_or_else(Vec::new, |card| self.lane_harness_patterns(card));
                    if let Some((file, pattern)) = Self::locked_hit(repository, files, &harness) {
                        return Ok(Err(format!(
                            "the change touches {file}, which is locked by {pattern}: only a person merges it"
                        )));
                    }
                }
            }
        }
        self.rules_allow(
            by,
            change,
            repository,
            card,
            files.ok().as_deref(),
            measured,
        )
    }

    /// The first of `files` that a locked path of the repository holds, and the pattern that locks it. The locked paths are
    /// the built-in ones, the owner's, and the repository's rules for its reviews, which are locked from the root: a change
    /// that rewrote them would be believed by every review after it. `harness` are the paths of the test lane's measuring
    /// harness (see `app::lane`), which only a person merges too.
    pub(in crate::app) fn locked_hit(
        repository: &Repository,
        files: &[String],
        harness: &[String],
    ) -> Option<(String, String)> {
        let brief: Vec<String> = repository
            .review_brief
            .iter()
            .map(|path| format!("/{}", path.trim_start_matches('/')))
            .collect();
        let owners = repository.locked_paths.iter().map(String::as_str);
        let locked = paths::DEFAULT_LOCKED
            .iter()
            .copied()
            .chain(owners)
            .chain(harness.iter().map(String::as_str))
            .chain(brief.iter().map(String::as_str));
        let hit = files.iter().find_map(|file| {
            paths::locked_by(locked.clone(), file)
                .map(|pattern| (file.clone(), pattern.to_string()))
        });
        // A file at the place of a directory above a harness path (a link `ci` for `ci/bench`) is a change of the harness too.
        hit.or_else(|| {
            files.iter().find_map(|file| {
                harness
                    .iter()
                    .find(|pattern| {
                        let path = pattern.trim_matches('/');
                        path.starts_with(&format!("{}/", file.trim_matches('/')))
                    })
                    .map(|pattern| (file.clone(), pattern.clone()))
            })
        })
    }

    /// [`App::locked_hit`], for the tests of other modules (which need a Unix program to run).
    #[cfg(all(test, unix))]
    pub(crate) fn locked_hit_for_test(
        repository: &Repository,
        files: &[String],
        harness: &[String],
    ) -> Option<(String, String)> {
        Self::locked_hit(repository, files, harness)
    }

    /// Whether a measurement is accepted for the change's current head.
    fn measured(&self, change: &Change) -> Result<bool> {
        match change.head.as_deref() {
            Some(head) => Ok(self.store.has_measurement(change.id, head)?),
            None => Ok(false),
        }
    }

    /// The paths merging the change into the default branch changes, read from git objects only; why not, when they
    /// cannot be.
    pub(in crate::app) fn merge_paths(
        &self,
        repository: &Repository,
        change: &Change,
    ) -> std::result::Result<Vec<String>, String> {
        let clone = std::path::Path::new(&repository.path);
        let base = crate::repos::git(clone, &["rev-parse", &repository.base_ref()])
            .map_err(|e| e.to_string())?;
        let head = change.head.clone().unwrap_or_else(|| change.branch.clone());
        match review_context::merged_paths(repository, &base, &head) {
            Ok(paths) => Ok(paths),
            // A change that conflicts cannot merge, and the forge says so; what the lock needs is the paths it touches.
            Err(review_context::ContextError::Conflicts) => {
                review_context::branch_paths(repository, &head)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// The decision of the kind `merge_check`: the repository's rules (the server's, then the repository's own
    /// `rules.md`) against this merge, decided as the rules are for a tool call. With no rules there is nothing to
    /// decide; with rules and no decider, or one that fails, the merge is refused.
    fn rules_allow(
        &self,
        by: &Actor,
        change: &Change,
        repository: &Repository,
        card: Option<&Card>,
        files: Option<&[String]>,
        measured: bool,
    ) -> Result<Gate> {
        let project = self.repository_file(repository, "rules.md");
        let rules = match rules::applicable(self.rules_file().as_deref(), project.as_deref()) {
            Ok(rules) => rules,
            Err(error) => return Ok(Err(format!("the rules cannot be read: {error}"))),
        };
        if rules.is_empty() {
            return Ok(Ok(()));
        }
        let input = json!({
            "repository": repository.name,
            "branch": change.branch,
            "pull_request": change.pull_request,
            "head": change.head,
            "author": change.outside_author,
            "merged_by": by.decider(),
            "checks": change.checks,
            "files": files,
            "measurement_accepted": measured,
            "labels": card.map(|card| card.labels.clone()),
        });
        let mut context = BTreeMap::from([
            ("tool".to_string(), "merge".to_string()),
            ("input".to_string(), input.to_string()),
            ("rules".to_string(), rules::to_markdown(&rules)),
        ]);
        if let Some(card) = card {
            context.insert("card_title".into(), card.title.clone());
            context.insert("card_body".into(), card.body.clone());
        }
        let decision = self.decisions.decide(&Question {
            kind: MERGE_CHECK.into(),
            context,
            options: vec!["allow".into(), "refuse".into()],
            fail_closed: "refuse".into(),
            run_id: None,
            card_id: change.card_id,
        })?;
        Ok(if decision.choice == "allow" {
            Ok(())
        } else {
            Err(format!("the rules refuse this merge: {}", decision.reason))
        })
    }

    /// Writes down that the gate refused `change`: the card goes to Ready to merge (it stays when it is there already
    /// or is done), and one open inbox item `merge_refused` names the reason.
    fn merge_refused(
        &self,
        change: &Change,
        card: Option<&Card>,
        repository: &Repository,
        reason: &str,
    ) -> Result<()> {
        // The same refusal, asked again by a script at each tick, is one item and not a new one each time.
        if let Some(card) = card
            && self.store.inbox_for_card(card.id)?.iter().any(|item| {
                item.kind == MERGE_REFUSED
                    && item.resolved_at.is_none()
                    && item.data["change"] == change.id
                    && item.data["reason"] == reason
            })
        {
            return Ok(());
        }
        self.store.resolve_merge_refused_for_change(change.id)?;
        let title = match card {
            Some(card) => card.title.clone(),
            None => change.branch.clone(),
        };
        self.store.add_inbox_item(
            MERGE_REFUSED,
            card.map(|card| card.id),
            &title,
            &json!({
                "change": change.id,
                "branch": change.branch,
                "pull_request": change.pull_request,
                "repository": repository.name,
                "reason": reason,
            }),
        )?;
        // The daily digest tells why the gate refused (ADR 0032).
        let _ = self.store.note_activity(
            "merge_refused",
            Some(repository.id),
            card.map(|card| card.number).or(change.pull_request),
            None,
            reason,
            &json!({ "change": change.id, "card": card.map(|card| card.id), "title": title }),
        );
        if let Some(card) = card {
            // The owner can merge it where a person may: the card keeps the item that offers the merge, once.
            let offered = self.store.inbox_for_card(card.id)?.iter().any(|item| {
                item.kind == "ready_to_merge"
                    && item.resolved_at.is_none()
                    && item.data["change"] == change.id
            });
            if !offered {
                self.store.add_inbox_item(
                    "ready_to_merge",
                    Some(card.id),
                    &card.title,
                    &json!({ "change": change.id, "branch": change.branch }),
                )?;
            }
            let now = self.card(card.id)?;
            let class = self.column_class(now.project_id, &now.column);
            if class != columns::READY_TO_MERGE && class != columns::DONE && !now.closed {
                self.move_card(
                    card.id,
                    columns::READY_TO_MERGE,
                    Some(&Actor::Script {
                        column: "merge-gate".into(),
                    }),
                )?;
            }
            self.bus
                .publish(crate::bus::Event::CardUpdated(self.card(card.id)?));
        }
        Ok(())
    }
}
