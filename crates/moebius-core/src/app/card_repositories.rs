//! The repositories a card works in (ADR 0019). A card always works in its own issue's repository: the forge's for an
//! issue of a forge, and the project's files for a card of a project without repositories. A person or the Moebius
//! agent names the others (`set_card_repositories`), from the repositories of the card's project; a run of the card gets
//! a worktree of each and nothing else (see `harness::run`). A `code` run that finds it needs another says so in its
//! summary, in a line `REPOSITORY: <name>`; the run still succeeds, the repository is added to the card and the card
//! goes back to coding with a note saying so (`repositories_asked`, `grant_asked`), so the next round has its worktree. Each repository gets
//! its own change (see `changes`).

use std::collections::BTreeMap;

use super::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{Card, Repository};

/// What starts the line of a `code` run's summary that asks for another repository.
pub(crate) const ASKS_FOR: &str = "REPOSITORY:";

/// The names a summary asks for, one per `REPOSITORY:` line, in the order it gives them.
pub(crate) fn asked_for(summary: &str) -> Vec<String> {
    summary
        .lines()
        .filter_map(|line| line.trim().strip_prefix(ASKS_FOR))
        .map(|name| name.trim().trim_matches('`').trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// `names` joined for a sentence: `a`, `a and b`, `a, b and c`.
fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What a `code` run asked for (see `App::repositories_asked`).
pub(crate) struct Asked {
    card_id: i64,
    /// The ids of the repositories to add, in the order asked.
    repositories: Vec<i64>,
    note: String,
}

impl App {
    /// The repositories `card` works in, in the order it names them: its own issue's repository first.
    pub fn card_repositories(&self, card: &Card) -> Result<Vec<Repository>> {
        let mut repositories = vec![self.repository(card.repository_id)?];
        for id in self.store.card_repositories(card.id)? {
            repositories.push(self.repository(id)?);
        }
        Ok(repositories)
    }

    /// The repositories of the card's project that the card does not work in yet, which it may be given: the project's
    /// repositories and its files, in the project's order.
    pub fn repositories_to_add(&self, card: &Card) -> Result<Vec<Repository>> {
        let project = self.project(card.project_id)?;
        let mut candidates = Vec::new();
        for id in project.repositories.iter().chain(project.files.iter()) {
            if *id != card.repository_id && !candidates.iter().any(|r: &Repository| r.id == *id) {
                candidates.push(self.repository(*id)?);
            }
        }
        let named = self.store.card_repositories(card.id)?;
        candidates.retain(|r| !named.contains(&r.id));
        Ok(candidates)
    }

    /// `by` names the repositories the card works in besides its own, in the order its changes are to be merged:
    /// each a repository of the card's project or its files, by name in any case. The card's own repository is always
    /// first; naming it, or a name twice, changes nothing, and none names only its own. A name that is no repository
    /// of the project is refused, and so is leaving out a repository in which the card has an open change. Recorded as a decision of kind `set_repositories`; the next run of the card works in
    /// them, a run that is going keeps what it had.
    pub fn set_card_repositories(
        &self,
        by: &Actor,
        card_id: i64,
        names: &[String],
        reason: &str,
    ) -> Result<Card> {
        let card = self.card(card_id)?;
        let project = self.project(card.project_id)?;
        let mut allowed = self.repositories_to_add(&card)?;
        for id in self.store.card_repositories(card.id)? {
            allowed.push(self.repository(id)?);
        }
        let own = self.repository(card.repository_id)?;
        let mut ids = Vec::new();
        for name in names {
            let name = name.trim();
            if name.eq_ignore_ascii_case(&own.name) {
                continue;
            }
            let Some(repository) = allowed.iter().find(|r| r.name.eq_ignore_ascii_case(name))
            else {
                return Err(AppError::Invalid(format!(
                    "{name} is not a repository of the project {}",
                    project.name
                )));
            };
            if !ids.contains(&repository.id) {
                ids.push(repository.id);
            }
        }
        // A change already published in a repository stays the card's until it is merged or closed.
        for id in self.store.card_repositories(card.id)? {
            if !ids.contains(&id) && self.store.open_change_in(card.id, id)?.is_some() {
                return Err(AppError::Invalid(format!(
                    "the card has an open change in {}: it cannot leave the card while the change is open",
                    self.repository(id)?.name
                )));
            }
        }
        let context = BTreeMap::from([
            ("title".to_string(), card.title.clone()),
            ("repositories".to_string(), names.join(", ")),
        ]);
        self.record_action(by, "set_repositories", Some(card.id), None, context, reason)?;
        self.store.set_card_repositories(card.id, &ids)?;
        let card = self.card(card.id)?;
        self.bus.publish(Event::CardUpdated(card.clone()));
        Ok(card)
    }

    /// What the `code` run `run_id` of a card asks for in its summary (see [`asked_for`]): those of the project's
    /// repositories the card does not work in yet, to be added to it, and the note that sends the card back to coding
    /// naming them, or, when it asked only for names that are no repository of the project, saying which it may ask
    /// for. None when the run asks for nothing new: then it is to be ended as any other. A run that asks succeeds even
    /// when it changed nothing yet, since it could not do the work without the repository.
    pub(crate) fn repositories_asked(&self, run_id: i64) -> Result<Option<Asked>> {
        let run = self.run(run_id)?;
        let (Some(card_id), Some(summary)) = (run.card_id, run.summary.as_deref()) else {
            return Ok(None);
        };
        let asked = asked_for(summary);
        if asked.is_empty() {
            return Ok(None);
        }
        let card = self.card(card_id)?;
        let candidates = self.repositories_to_add(&card)?;
        let (mut added, mut unknown) = (Vec::<Repository>::new(), Vec::new());
        for name in asked {
            if card
                .repositories
                .iter()
                .any(|r| r.eq_ignore_ascii_case(&name))
                || added.iter().any(|r| r.name.eq_ignore_ascii_case(&name))
            {
                continue;
            }
            match candidates
                .iter()
                .find(|r| r.name.eq_ignore_ascii_case(&name))
            {
                Some(repository) => added.push(repository.clone()),
                None if !unknown.contains(&name) => unknown.push(name),
                None => {}
            }
        }
        if added.is_empty() && unknown.is_empty() {
            return Ok(None);
        }
        let names: Vec<String> = added.iter().map(|r| r.name.clone()).collect();
        let mut note = Vec::new();
        if !added.is_empty() {
            note.push(format!(
                "You asked for {}: the card now works in it too, and this round has a worktree of each of its \
                 repositories. Finish the work in every repository the issue needs.",
                listed(&names)
            ));
        }
        if !unknown.is_empty() {
            let project = self.project(card.project_id)?;
            let left: Vec<String> = candidates
                .iter()
                .filter(|r| !added.iter().any(|a| a.id == r.id))
                .map(|r| r.name.clone())
                .collect();
            let may = if left.is_empty() {
                "it has no other repository to add".to_string()
            } else {
                format!("you may ask for {}", listed(&left))
            };
            note.push(format!(
                "You asked for {}, which is no repository of the project {}; {may}.",
                listed(&unknown),
                project.name
            ));
        }
        Ok(Some(Asked {
            card_id,
            repositories: added.iter().map(|r| r.id).collect(),
            note: note.join("\n"),
        }))
    }

    /// Ends the `code` run `run_id` as succeeded with what it `asked` for: the repositories are added to its card and
    /// the card goes back to coding with the note, in one step (`Store::succeed_asking_for_repositories`).
    pub(crate) fn grant_asked(&self, run_id: i64, asked: &Asked) -> Result<()> {
        self.store.succeed_asking_for_repositories(
            run_id,
            asked.card_id,
            &asked.repositories,
            &asked.note,
        )?;
        self.bus
            .publish(Event::CardUpdated(self.card(asked.card_id)?));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_asks_for_the_repositories_of_its_lines() {
        let summary =
            "Done the API.\nREPOSITORY: web\n  REPOSITORY: `docs` \nREPOSITORY:\nNot REPOSITORY: x";
        assert_eq!(asked_for(summary), ["web", "docs"]);
        assert!(asked_for("Nothing asked.").is_empty());
    }

    #[test]
    fn names_are_listed_for_a_sentence() {
        let names = |n: &[&str]| listed(&n.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(names(&["a"]), "a");
        assert_eq!(names(&["a", "b"]), "a and b");
        assert_eq!(names(&["a", "b", "c"]), "a, b and c");
    }
}
