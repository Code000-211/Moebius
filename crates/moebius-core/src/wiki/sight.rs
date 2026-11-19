//! Who sees which wiki pages (ADR 0018, ADR 0022): the pages under `projects/<key>/` of a private project are seen
//! only by its members, and put into the prompt of a run only for a card of that project. Every route, list, search and prompt of the wiki asks here;
//! a page that is not seen is answered as if it were not there. The organisation's pages and those of a public project
//! are seen by everyone, and so is a folder of a key no project has, which is nobody's private page.

use super::project_key;
use crate::app::{App, Result, Sight};
use crate::decide::Actor;

/// The pages one asker sees.
pub struct Seen {
    sight: Sight,
    /// The project of the card a run is for, whose pages the run is given.
    own: Option<i64>,
}

impl Seen {
    /// What `by` sees of the wiki: what their memberships let them. The Moebius agent sees what the person of its chat
    /// sees, and an agent without one no private project's pages but those of the project of its run's card.
    pub fn of(app: &App, by: &Actor) -> Result<Self> {
        let own = match by {
            Actor::Agent { run, .. } => app
                .store
                .run(*run)?
                .and_then(|run| run.card_id)
                .map(|card| app.store.card(card))
                .transpose()?
                .flatten()
                .map(|card| card.project_id),
            _ => None,
        };
        Ok(Self {
            sight: app.sight(by)?,
            own,
        })
    }

    /// What a person with `sight` sees, who is no agent's run.
    pub fn from_sight(sight: Sight) -> Self {
        Self { sight, own: None }
    }

    /// What the run of a card of the project `project` sees: no private project but its own.
    pub fn of_project(app: &App, project: i64) -> Result<Self> {
        Ok(Self {
            sight: app.sight_of(None)?,
            own: Some(project),
        })
    }

    /// Whether the pages of the project with `key` are seen; what cannot be checked is not.
    pub fn project(&self, app: &App, key: &str) -> bool {
        if self.sight.hides_nothing() {
            return true;
        }
        match app.store.project_by_key(key) {
            Ok(Some(project)) => self.sight.project(project.id) || self.own == Some(project.id),
            Ok(None) => true,
            Err(_) => false,
        }
    }

    /// Whether the page `path` is seen.
    pub fn page(&self, app: &App, path: &str) -> bool {
        project_key(path).is_none_or(|key| self.project(app, key))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::Store;

    #[test]
    fn a_run_reads_the_pages_of_its_own_private_project_and_no_other() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let one = store
            .add_repository("one", "/srv/one", "none", "main")
            .unwrap();
        let two = store
            .add_repository("two", "/srv/two", "none", "main")
            .unwrap();
        for repository in [&one, &two] {
            store
                .set_project_access(repository.home_project_id, true, &[])
                .unwrap();
        }
        let key = |repository: &crate::store::Repository| {
            store
                .project(repository.home_project_id)
                .unwrap()
                .unwrap()
                .key
        };
        let card = store
            .add_card(one.id, "Plan", "", "person", "Not started")
            .unwrap();
        let run = store
            .add_run(Some(card.id), "Coder", "test", "code")
            .unwrap();
        let app = App::new(
            store.clone(),
            Arc::new(Host::new(Duration::from_secs(1))),
            &dir.path().join("data"),
        );
        let agent = Actor::Agent {
            name: "Coder".into(),
            run: run.id,
        };
        let seen = Seen::of(&app, &agent).unwrap();
        assert!(seen.page(&app, &format!("projects/{}/Plans", key(&one))));
        assert!(!seen.page(&app, &format!("projects/{}/Plans", key(&two))));
        // An agent of no card sees no private project.
        let chat = store.add_run(None, "Moebius", "test", "chat").unwrap();
        let chat = Actor::Agent {
            name: "Moebius".into(),
            run: chat.id,
        };
        assert!(
            !Seen::of(&app, &chat)
                .unwrap()
                .page(&app, &format!("projects/{}/Plans", key(&one)))
        );
    }
}
