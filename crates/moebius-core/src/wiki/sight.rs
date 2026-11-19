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
    /// sees, and an agent without one no private project's pages.
    pub fn of(app: &App, by: &Actor) -> Result<Self> {
        Ok(Self::from_sight(app.sight(by)?))
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
