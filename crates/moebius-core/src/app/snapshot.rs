//! What the snapshot needs to know about each card, read once for all of them: the repositories, the people, the
//! approved issues, the dependencies, the repositories a card works in and its open changes. `App::counts_as_sub_issue`,
//! `App::blocked_by_seen` and `App::open_changes` ask the store about one card at a time; the snapshot has thousands
//! of cards, so it asks [`CardLookups`], which answers the same from what it loaded, in a number of reads that does not
//! grow with the number of cards.

use std::collections::{HashMap, HashSet};

use super::checks::checks_state;
use super::{App, AppError, Result, Sight, columns, is_person};
use crate::store::{Card, Change, Repository, Role};

/// The class of the column and the project of a card, which is all a blocker is asked for.
struct Blocker {
    class: &'static str,
    project_id: i64,
}

/// What `App::card_lookups` read, to answer for any card of the repositories it was given.
pub struct CardLookups {
    repositories: HashMap<i64, Repository>,
    /// The handles of the people who are no viewers.
    non_viewers: HashSet<String>,
    approved: HashSet<i64>,
    card_repositories: HashMap<i64, Vec<i64>>,
    dependencies: HashMap<i64, Vec<i64>>,
    /// The latest open change of a card in a repository, by (card, repository).
    open_changes: HashMap<(i64, i64), Change>,
    /// The cards by (repository, issue number).
    cards: HashMap<(i64, i64), Blocker>,
    /// The issue numbers the forge was seen to say are closed, by repository.
    closed: HashMap<i64, HashSet<i64>>,
    /// The rounds of the Reviewer on each open pull request someone else opened, by change.
    external_reviews: HashMap<i64, Vec<crate::store::ExternalReview>>,
}

impl CardLookups {
    /// The rounds of the Reviewer on the change, oldest first; none for a change that is not a followed request.
    pub fn external_reviews(&self, change_id: i64) -> Result<Vec<crate::store::ExternalReview>> {
        Ok(self
            .external_reviews
            .get(&change_id)
            .cloned()
            .unwrap_or_default())
    }

    /// The repository `id`, as `App::repository` says it.
    fn repository(&self, id: i64) -> Result<&Repository> {
        self.repositories
            .get(&id)
            .ok_or_else(|| AppError::NotFound(format!("repository {id}")))
    }

    /// `App::counts_as_sub_issue`.
    pub fn counts_as_sub_issue(&self, card: &Card) -> bool {
        self.is_trusted(card) || card.confirmed || self.approved.contains(&card.id)
    }

    /// `App::is_trusted`.
    fn is_trusted(&self, card: &Card) -> bool {
        let Some(repository) = self.repositories.get(&card.repository_id) else {
            return false;
        };
        if repository.forge == "none" {
            return is_person(&card.author) && !self.needs_approval(&card.author);
        }
        repository
            .trusted
            .iter()
            .any(|login| login.eq_ignore_ascii_case(&card.author))
    }

    /// `App::needs_approval`.
    fn needs_approval(&self, author: &str) -> bool {
        let Some(handle) = author
            .strip_prefix(super::PERSON)
            .and_then(|rest| rest.strip_prefix(':'))
        else {
            return false;
        };
        !self.non_viewers.contains(handle)
    }

    /// `App::blocked_by_seen`.
    pub fn blocked_by_seen(&self, card: &Card, sight: &Sight) -> Vec<i64> {
        let Some(numbers) = self.dependencies.get(&card.id) else {
            return Vec::new();
        };
        let repository = card.repository_id;
        numbers
            .iter()
            .copied()
            .filter(|number| {
                let blocker = self.cards.get(&(repository, *number));
                let resolved = match blocker {
                    Some(blocker) => blocker.class == columns::DONE,
                    None => self
                        .closed
                        .get(&repository)
                        .is_some_and(|closed| closed.contains(number)),
                };
                let hidden = !sight.hides_nothing()
                    && blocker.is_some_and(|blocker| !sight.project(blocker.project_id));
                !resolved && !hidden
            })
            .collect()
    }

    /// `App::open_changes`.
    pub fn open_changes(&self, card: &Card) -> Result<Vec<Change>> {
        let mut changes = Vec::new();
        let named = self
            .card_repositories
            .get(&card.id)
            .map_or(&[][..], Vec::as_slice);
        for id in std::iter::once(&card.repository_id).chain(named) {
            self.repository(*id)?;
            if let Some(change) = self.open_changes.get(&(card.id, *id)) {
                changes.push(change.clone());
            }
        }
        Ok(changes)
    }

    /// The name of the repository `id`.
    pub fn repository_name(&self, id: i64) -> Result<String> {
        Ok(self.repository(id)?.name.clone())
    }

    /// `App::checks_of`.
    pub fn checks_of(&self, change: &Change) -> Result<&'static str> {
        checks_state(change, || match change.repository_id {
            Some(id) => Ok(self.repository(id)?.forge != "none"),
            None => Err(AppError::Invalid(format!(
                "change {} belongs to the harness configuration repository",
                change.id
            ))),
        })
    }
}

impl App {
    /// Reads what `CardLookups` answers from: for `repositories`, their `cards` (all of them, archived ones included,
    /// by repository id) and `changes` (by repository id), and for the whole store the rest, each in one read.
    pub fn card_lookups(
        &self,
        repositories: &[Repository],
        cards: &HashMap<i64, Vec<Card>>,
        changes: &HashMap<i64, Vec<Change>>,
    ) -> Result<CardLookups> {
        let mut open_changes = HashMap::new();
        for change in changes.values().flatten() {
            if let (Some(card), Some(repository)) = (change.card_id, change.repository_id)
                && change.state == "open"
            {
                // The latest one: a repository's changes are in order of their ids.
                let latest = open_changes
                    .entry((card, repository))
                    .or_insert_with(|| change.clone());
                if change.id > latest.id {
                    *latest = change.clone();
                }
            }
        }
        let mut by_number = HashMap::new();
        let mut class_of = self.class_of();
        for card in cards.values().flatten() {
            by_number.insert(
                (card.repository_id, card.number),
                Blocker {
                    class: class_of.class(card.project_id, &card.column),
                    project_id: card.project_id,
                },
            );
        }
        let mut closed = HashMap::new();
        for repository in repositories {
            closed.insert(repository.id, self.store.closed_on_forge(repository.id)?);
        }
        let mut external_reviews = HashMap::new();
        for change in open_changes.values().filter(|c| c.outside_author.is_some()) {
            external_reviews.insert(change.id, self.store.external_reviews(change.id)?);
        }
        Ok(CardLookups {
            repositories: repositories.iter().map(|r| (r.id, r.clone())).collect(),
            non_viewers: self
                .store
                .people()?
                .into_iter()
                .filter(|person| person.role != Role::Viewer)
                .map(|person| person.handle)
                .collect(),
            approved: self.store.cards_approved()?,
            card_repositories: self.store.all_card_repositories()?,
            dependencies: self.store.all_dependencies()?,
            open_changes,
            cards: by_number,
            closed,
            external_reviews,
        })
    }
}
