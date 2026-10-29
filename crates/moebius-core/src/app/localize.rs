//! The hint a code run is given (see `decide::localize`): which files its issue is likely to touch, asked of the
//! decision point `localize` when the run starts, and what came of it, recorded as the judgement's outcome when the
//! card's change is known: at the end of each code run and when the change is merged.
//!
//! Whether the hint is asked for is the server's `[localize]` table, which a repository overrides in
//! `.moebius/localize` on its default branch. Anything that fails leaves the run without a hint.

use std::collections::BTreeMap;
use std::path::Path;

use super::App;
use crate::config::{LocalizeConfig, MAX_LOCALIZE_FILES};
use crate::decide::Question;
use crate::decide::localize::{self, Issue, KIND, Outcome};
use crate::decide::model::localize as model;
use crate::repos::{git, git_bytes};
use crate::store::{Card, Change, Repository};

/// The file on a repository's default branch that turns the hint on or off for it and says how many files it names.
const SETTINGS: &str = ".moebius/localize";

/// How many of the files that best match the issue a decider with no index of its own is shown.
const CANDIDATES: usize = 100;

/// Reads `.moebius/localize` over `base`: lines that are empty or start with `#` are skipped, `on` and `off` turn the
/// hint on and off, and `files <n>` (1 to [`MAX_LOCALIZE_FILES`]) says how many files it names. A line that says
/// anything else is ignored.
fn read_settings(text: &str, base: LocalizeConfig) -> LocalizeConfig {
    let mut settings = base;
    for line in text.lines().map(str::trim) {
        let mut words = line.split_whitespace();
        match (words.next(), words.next(), words.next()) {
            (Some("on"), None, None) => settings.enabled = true,
            (Some("off"), None, None) => settings.enabled = false,
            (Some("files"), Some(count), None) => {
                if let Some(count) = count
                    .parse::<usize>()
                    .ok()
                    .filter(|count| (1..=MAX_LOCALIZE_FILES).contains(count))
                {
                    settings.files = count;
                }
            }
            _ => {}
        }
    }
    settings
}

/// The files `git diff` named, which `-z` ends each with a NUL.
fn names(output: &[u8]) -> Vec<String> {
    output
        .split(|&b| b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

/// The files the commits of `HEAD` in `dir` changed after the ones of `base`.
fn touched_since(dir: &Path, base: &str) -> Option<Vec<String>> {
    let range = format!("{base}...HEAD");
    git_bytes(dir, &["diff", "--name-only", "--no-renames", "-z", &range])
        .ok()
        .map(|output| names(&output))
}

impl App {
    /// How the hint is asked for in `repository`: the server's settings with the repository's own over them.
    pub fn localize_for(&self, repository: &Repository) -> LocalizeConfig {
        let spec = format!("{}:{SETTINGS}", repository.base_ref());
        match git(Path::new(&repository.path), &["show", &spec]) {
            Ok(text) => read_settings(&text, self.localize),
            Err(_) => self.localize,
        }
    }

    /// The text a code run of `card` is given after its issue: the files likely to be relevant, as the decision point
    /// `localize` ranked them, and to be verified. None when the hint is off for the card's repository, when nothing
    /// answers the decision point, or when anything fails, so that the run starts without one.
    pub fn localize_hint(&self, card: &Card) -> Option<String> {
        let repository = self.store.repository(card.repository_id).ok()??;
        let settings = self.localize_for(&repository);
        if !settings.enabled || !self.decisions.decides(KIND) {
            return None;
        }
        localize::refresh(&self.store, &repository).ok()?;
        let issue = Issue {
            title: &card.title,
            body: &card.body,
        };
        let candidates =
            localize::candidates(&self.store, repository.id, &issue, CANDIDATES).ok()?;
        let decision = self
            .decisions
            .decide(&Question {
                kind: KIND.into(),
                context: BTreeMap::from([
                    ("title".to_string(), card.title.clone()),
                    ("body".to_string(), card.body.clone()),
                    ("repository".to_string(), repository.name.clone()),
                    ("card".to_string(), card.id.to_string()),
                    ("files".to_string(), settings.files.to_string()),
                    ("candidates".to_string(), candidates.join("\n")),
                ]),
                options: Vec::new(),
                fail_closed: localize::EMPTY.into(),
                run_id: None,
                card_id: Some(card.id),
            })
            .ok()?;
        if decision.refused {
            return None;
        }
        let known: std::collections::HashSet<String> = self
            .store
            .localize_paths(repository.id)
            .ok()?
            .into_iter()
            .collect();
        let hints: Vec<_> = localize::parse(&decision.choice)
            .ok()?
            .into_iter()
            .filter(|hint| known.contains(&hint.path))
            .take(settings.files)
            .collect();
        if hints.is_empty() {
            return None;
        }
        let mut text = String::from("Files that are likely relevant (a hint, not a limit):\n\n");
        for hint in &hints {
            match hint.reason.trim() {
                "" => text.push_str(&format!("- {}\n", hint.path)),
                reason => text.push_str(&format!("- {}: {reason}\n", hint.path)),
            }
        }
        text.push_str(
            "\nThey come from a search over the repository, not from reading the change. Verify them before you rely on \
             them, and look elsewhere when they do not fit.\n",
        );
        Some(text)
    }

    /// Records what came of the hints given for `card_id`: the files the change touched, `touched` (none when they
    /// cannot be read), with the recall at ten and the precision of each hint, as the outcome of its judgement. A
    /// known outcome is replaced by a newer one, never by an unknown one. A decision model that shadows the kind is
    /// then given the chance to propose a takeover.
    pub fn record_localize_outcome(&self, card_id: i64, touched: Option<&[String]>) {
        let Ok(judgements) = self.store.judgements_for_card(card_id) else {
            return;
        };
        let mut recorded = false;
        for judgement in judgements.iter().filter(|j| j.kind == KIND) {
            let outcome = match touched {
                Some(touched) => {
                    let hinted: Vec<String> = localize::parse(&judgement.choice)
                        .map(|hints| hints.into_iter().map(|hint| hint.path).collect())
                        .unwrap_or_default();
                    Outcome::of(&hinted, touched)
                }
                None if judgement.outcome.is_some() => continue,
                None => Outcome::unknown(),
            };
            recorded |= self
                .store
                .set_outcome(judgement.id, &outcome.json())
                .is_ok();
        }
        if recorded {
            model::reconsider(self);
        }
    }

    /// A code run of a card ended and its work is committed in `worktree`: what the card's change touches so far is the
    /// outcome of the hints given for it.
    pub fn localize_run_ended(&self, run_id: i64, worktree: &Path) {
        let Some(card_id) = self
            .store
            .run(run_id)
            .ok()
            .flatten()
            .and_then(|run| run.card_id)
        else {
            return;
        };
        let hinted = self
            .store
            .judgements_for_card(card_id)
            .is_ok_and(|judgements| judgements.iter().any(|j| j.kind == KIND));
        let Some(repository) = self
            .card(card_id)
            .ok()
            .and_then(|card| self.store.repository(card.repository_id).ok().flatten())
        else {
            return;
        };
        if hinted {
            let touched = touched_since(worktree, &repository.base_ref());
            self.record_localize_outcome(card_id, touched.as_deref());
        }
    }

    /// The change of a card was merged, as `commit` if the merge is known: the files it touched are the outcome of the
    /// hints given for the card. A change merged without a trace in the clone leaves the outcomes as they are.
    pub(super) fn localize_change_merged(&self, change: &Change, commit: Option<&str>) {
        let Some(card_id) = change.card_id else {
            return;
        };
        let Some(repository) = change
            .repository_id
            .and_then(|id| self.store.repository(id).ok().flatten())
        else {
            return;
        };
        let touched = merged_files(&repository, &change.branch, commit);
        self.record_localize_outcome(card_id, touched.as_deref());
    }
}

/// The files a merged change touched: for a branch that the default branch does not hold, as a squash leaves it, what
/// it changed since it forked; otherwise, for a merge commit, what the merge brought in.
fn merged_files(
    repository: &Repository,
    branch: &str,
    commit: Option<&str>,
) -> Option<Vec<String>> {
    let clone = Path::new(&repository.path);
    let base = repository.base_ref();
    let held = git(clone, &["merge-base", "--is-ancestor", branch, &base]).is_ok();
    if git(clone, &["rev-parse", "--verify", "--quiet", branch]).is_ok() && !held {
        let range = format!("{base}...{branch}");
        return git_bytes(
            clone,
            &["diff", "--name-only", "--no-renames", "-z", &range],
        )
        .ok()
        .map(|output| names(&output));
    }
    let commit = commit?;
    let parents = git(clone, &["rev-list", "--parents", "-n", "1", commit]).ok()?;
    if parents.split_whitespace().count() != 3 {
        return None;
    }
    let first = format!("{commit}^1");
    git_bytes(
        clone,
        &["diff", "--name-only", "--no-renames", "-z", &first, commit],
    )
    .ok()
    .map(|output| names(&output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_turns_the_hint_on_or_off_and_names_how_many_files() {
        let server = LocalizeConfig {
            enabled: true,
            files: 10,
        };
        assert_eq!(read_settings("", server), server);
        let off = read_settings("# no hints here\noff\n", server);
        assert!(!off.enabled);
        let few = read_settings("files 3\n", server);
        assert_eq!((few.enabled, few.files), (true, 3));
        let on = read_settings(
            "on\nfiles 0\nfiles 11\nfiles x\nnonsense\nfiles 4 5",
            LocalizeConfig {
                enabled: false,
                files: 6,
            },
        );
        assert_eq!((on.enabled, on.files), (true, 6));
    }
}
