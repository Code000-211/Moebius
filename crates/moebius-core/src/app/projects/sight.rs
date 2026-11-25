//! Who sees what of a private project (ADR 0018): a private project names its members, and everyone else, owners
//! included unless they add themselves, sees neither its board, its cards, its runs nor its files. A [`Sight`] is what
//! one person sees, read once per request; the desktop API answers anything it hides as if it did not exist (404), and
//! leaves it out of every list and of the event stream.
//!
//! - A card is seen when its project is.
//! - A repository is seen when one of its projects is: its files and its board belong to every project it is in.
//! - A run is seen when its card is, or, without a card, when its repository is; a Retro analyst's run when every card
//!   of its day's report is, since its transcript quotes them.
//! - A lesson is seen when every card it was learned from is, and is put in a run's prompt only when it comes from no
//!   private project but the run's own (`lesson_fits`).
//! - A Retro's proposal to the shared configuration is seen by everyone, without quoting a private project they do
//!   not see (`unquoted`): its cards are left out, and the project's name and key and its cards' titles are replaced
//!   by [`UNQUOTED`]. A proposal to a repository's settings is seen when the repository is.
//! - An owner who is no member still sees that a private project exists, its name, key and members, so that they can
//!   add people to it or themselves (`ProjectView::member` is false then), and nothing of its board.
//!
//! The Moebius agent and the Grilling partner see what the person whose chat they run in sees; a run of either without
//! such a person sees no private project. A column script, which is not a request, sees everything.

use std::collections::HashSet;

use super::super::{App, Result};
use crate::decide::Actor;
use crate::store::{Card, InboxItem, Lesson, Proposal, Role, Run};

/// What a proposal says in place of a private project, its key or the title of one of its cards, to someone who does
/// not see it.
pub const UNQUOTED: &str = "a private project";

/// What one person sees of the private projects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sight {
    /// The private projects the person is no member of.
    hidden: HashSet<i64>,
    /// The repositories each of whose projects is hidden.
    hidden_repositories: HashSet<i64>,
    /// Whether the person is an owner, who sees that a hidden project exists.
    owner: bool,
}

impl Sight {
    /// A sight that hides nothing: what the server sees for itself.
    pub fn everything() -> Self {
        Self {
            owner: true,
            ..Self::default()
        }
    }

    /// A sight that hides the projects `ids` (for the tests that need a Unix program).
    #[cfg(all(test, unix))]
    pub(crate) fn hiding(ids: &[i64]) -> Self {
        Self {
            hidden: ids.iter().copied().collect(),
            ..Self::default()
        }
    }

    /// Whether nothing is hidden, so that a list need not be filtered.
    pub fn hides_nothing(&self) -> bool {
        self.hidden.is_empty()
    }

    /// Whether the person sees the board, cards and runs of project `id`.
    pub fn project(&self, id: i64) -> bool {
        !self.hidden.contains(&id)
    }

    /// Whether the person sees at least the record of project `id`: its own, or as an owner.
    pub fn project_record(&self, id: i64) -> bool {
        self.owner || self.project(id)
    }

    /// Whether the person sees `card`.
    pub fn card(&self, card: &Card) -> bool {
        self.project(card.project_id)
    }

    /// Whether the person sees the repository `id`: its files, its settings and its board.
    pub fn repository(&self, id: i64) -> bool {
        !self.hidden_repositories.contains(&id)
    }

    /// The private projects the person does not see.
    pub fn hidden(&self) -> &HashSet<i64> {
        &self.hidden
    }
}

impl App {
    /// What `by` sees: a person what their memberships let them, the Moebius agent and the Grilling partner what the
    /// person of their chat sees, a column script everything.
    pub fn sight(&self, by: &Actor) -> Result<Sight> {
        let person = match by {
            Actor::Member { id, .. } => Some(*id),
            Actor::Agent { run, .. } => self.store.run(*run)?.and_then(|run| run.owner_id),
            Actor::Script { .. } => return Ok(Sight::everything()),
        };
        self.sight_of(person)
    }

    /// What the person `person` sees; with none, nothing private.
    pub fn sight_of(&self, person: Option<i64>) -> Result<Sight> {
        let hidden = self.store.hidden_projects(person)?;
        let owner = match person {
            Some(id) => self
                .store
                .person(id)?
                .is_some_and(|p| p.role == Role::Owner && !p.disabled),
            None => false,
        };
        let mut hidden_repositories = HashSet::new();
        if !hidden.is_empty() {
            let projects = self.store.projects()?;
            for repository in self.store.repositories()? {
                let seen = projects
                    .iter()
                    .filter(|p| p.repositories.contains(&repository.id))
                    .map(|p| p.id)
                    .chain([repository.home_project_id])
                    .any(|project| !hidden.contains(&project));
                if !seen {
                    hidden_repositories.insert(repository.id);
                }
            }
        }
        Ok(Sight {
            hidden,
            hidden_repositories,
            owner,
        })
    }

    /// Whether `sight` sees the card `id`; a card that does not exist is seen, so that its route answers 404 itself.
    pub fn sees_card(&self, sight: &Sight, id: i64) -> Result<bool> {
        if sight.hides_nothing() {
            return Ok(true);
        }
        Ok(self.store.card(id)?.is_none_or(|card| sight.card(&card)))
    }

    /// Whether `sight` sees `run`: by its card, else by its repository; a Retro analyst's run by the cards of its day.
    pub fn sees_run(&self, sight: &Sight, run: &Run) -> Result<bool> {
        if sight.hides_nothing() {
            return Ok(true);
        }
        if let Some(card) = run.card_id {
            return self.sees_card(sight, card);
        }
        if let Some(repository) = run.repository_id
            && !sight.repository(repository)
        {
            return Ok(false);
        }
        if run.purpose == "retro" {
            for retro in self.store.retros()? {
                if retro.run_id != Some(run.id) {
                    continue;
                }
                for card in report_cards(&retro.report) {
                    if !self.sees_card(sight, card)? {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    /// Whether `sight` sees the run `id`; one that does not exist is seen, so that its route answers 404 itself.
    pub fn sees_run_id(&self, sight: &Sight, id: i64) -> Result<bool> {
        match self.store.run(id)? {
            Some(run) => self.sees_run(sight, &run),
            None => Ok(true),
        }
    }

    /// Whether `sight` sees `lesson`: every card it was learned from, and its repository when it names no card.
    pub fn sees_lesson(&self, sight: &Sight, lesson: &Lesson) -> Result<bool> {
        if sight.hides_nothing() {
            return Ok(true);
        }
        if lesson.cards.is_empty() {
            return Ok(sight.repository(lesson.repository_id));
        }
        for card in &lesson.cards {
            if !self.sees_card(sight, *card)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Whether `lesson` may be found by a run of the project `project` (none for a run of no project's): it was
    /// learned from no private project but that one, so that what a private project taught is never shown on another's
    /// board.
    pub fn lesson_fits(&self, lesson: &Lesson, project: Option<i64>) -> Result<bool> {
        for card in &lesson.cards {
            if let Some(card) = self.store.card(*card)?
                && Some(card.project_id) != project
                && self
                    .store
                    .project(card.project_id)?
                    .is_some_and(|p| p.private)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Whether `sight` sees what a judgement was made on: its card, else its run.
    pub fn sees_judgement(
        &self,
        sight: &Sight,
        card: Option<i64>,
        run: Option<i64>,
    ) -> Result<bool> {
        match (card, run) {
            (Some(card), _) => self.sees_card(sight, card),
            (None, Some(run)) => self.sees_run_id(sight, run),
            (None, None) => Ok(true),
        }
    }
}

impl App {
    /// `proposal` as `sight` sees it: none when it changes a repository it does not see, otherwise without the cards
    /// and the quotes of the private projects it does not see.
    pub fn unquoted(&self, sight: &Sight, mut proposal: Proposal) -> Result<Option<Proposal>> {
        if sight.hides_nothing() {
            return Ok(Some(proposal));
        }
        if proposal.repository.is_some_and(|r| !sight.repository(r)) {
            return Ok(None);
        }
        let quotes = self.quotes(sight, &proposal.cards)?;
        let mut cards = Vec::new();
        for id in proposal.cards {
            if self.sees_card(sight, id)? {
                cards.push(id);
            }
        }
        proposal.cards = cards;
        for text in [&mut proposal.title, &mut proposal.body, &mut proposal.diff] {
            *text = unquote(text, &quotes);
        }
        Ok(Some(proposal))
    }

    /// An inbox item about a Retro's proposal (`data.proposal`) as `sight` sees it: its title and every text of its
    /// data without the quotes of the private projects it does not see, as [`App::unquoted`] shows the proposal.
    pub fn unquoted_item(&self, sight: &Sight, mut item: InboxItem) -> Result<InboxItem> {
        if sight.hides_nothing() {
            return Ok(item);
        }
        let Some(proposal) = item.data["proposal"].as_i64() else {
            return Ok(item);
        };
        let cards = self
            .store
            .proposal(proposal)?
            .map(|p| p.cards)
            .unwrap_or_default();
        let quotes = self.quotes(sight, &cards)?;
        item.title = unquote(&item.title, &quotes);
        unquote_json(&mut item.data, &quotes);
        Ok(item)
    }

    /// What must not be quoted to `sight`: the titles of those of `cards` it does not see, and the name and key of every
    /// private project it does not see; the longest first, so that a title holding a key is replaced whole.
    fn quotes(&self, sight: &Sight, cards: &[i64]) -> Result<Vec<String>> {
        let mut quotes = Vec::new();
        for id in cards {
            if let Some(card) = self.store.card(*id)?
                && !sight.card(&card)
            {
                quotes.push(card.title);
            }
        }
        for id in sight.hidden() {
            if let Some(project) = self.store.project(*id)? {
                quotes.push(project.name);
                quotes.push(project.key);
            }
        }
        quotes.retain(|q| !q.trim().is_empty());
        quotes.sort_by_key(|q| std::cmp::Reverse(q.len()));
        Ok(quotes)
    }
}

/// `text` with each of `quotes` replaced by [`UNQUOTED`].
fn unquote(text: &str, quotes: &[String]) -> String {
    quotes.iter().fold(text.to_string(), |text, quote| {
        text.replace(quote.as_str(), UNQUOTED)
    })
}

/// Every text in `value` unquoted.
fn unquote_json(value: &mut serde_json::Value, quotes: &[String]) {
    match value {
        serde_json::Value::String(text) => *text = unquote(text, quotes),
        serde_json::Value::Array(items) => items.iter_mut().for_each(|v| unquote_json(v, quotes)),
        serde_json::Value::Object(fields) => {
            fields.values_mut().for_each(|v| unquote_json(v, quotes))
        }
        _ => {}
    }
}

/// The cards a Retro's report names, its errors' and its merged cards'.
fn report_cards(report: &serde_json::Value) -> Vec<i64> {
    let errors = report["errors"].as_array().into_iter().flatten();
    let merged = report["merged"].as_array().into_iter().flatten();
    errors
        .chain(merged)
        .filter_map(|entry| entry["card"].as_i64())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::tests::app;
    use super::*;

    #[test]
    fn a_private_project_is_seen_by_its_members_only() {
        let (dir, app) = app();
        let path = dir.path().join("web").to_string_lossy().into_owned();
        let web = app.add_repository("web", &path).unwrap();
        let lea = app.store.add_person("lea", "Lea", Role::Member).unwrap();
        let ben = app.store.add_person("ben", "Ben", Role::Member).unwrap();
        let card = app
            .store
            .add_card(web.id, "Secret", "", "lea", "Backlog")
            .unwrap();
        let run = app
            .store
            .add_run(Some(card.id), "coder", "fake", "code")
            .unwrap();
        let ben_sees = app.sight_of(Some(ben.id)).unwrap();
        assert!(ben_sees.hides_nothing() && ben_sees.card(&card) && ben_sees.repository(web.id));

        app.store
            .set_project_access(web.home_project_id, true, &[lea.id])
            .unwrap();
        let (lea_sees, ben_sees) = (
            app.sight_of(Some(lea.id)).unwrap(),
            app.sight_of(Some(ben.id)).unwrap(),
        );
        assert!(lea_sees.card(&card) && lea_sees.repository(web.id));
        assert!(!ben_sees.card(&card) && !ben_sees.repository(web.id));
        assert!(!ben_sees.project_record(web.home_project_id));
        assert!(!app.sees_run(&ben_sees, &run).unwrap());
        assert!(app.sees_run(&lea_sees, &run).unwrap());
        // The first owner is no member either, but sees that the project exists.
        let owner = app.sight(&Actor::first_owner()).unwrap();
        assert!(!owner.project(web.home_project_id) && owner.project_record(web.home_project_id));
        // A run of the Moebius agent sees what the person of its chat sees, and without one nothing private.
        let chat = app.store.add_run(None, "Moebius", "fake", "chat").unwrap();
        let agent = Actor::Agent {
            name: "Moebius".into(),
            run: chat.id,
        };
        assert!(!app.sight(&agent).unwrap().project(web.home_project_id));
        app.store.set_chat_owner(chat.id, lea.id).unwrap();
        assert!(app.sight(&agent).unwrap().project(web.home_project_id));
    }
}
