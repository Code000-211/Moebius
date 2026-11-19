//! Searching the wiki (ADR 0022): pages found by the words of their title and text, in the whole wiki or in one
//! project's pages, and only within what the person may see. The index is derived data in the store
//! (`store::wiki_search`); it is made again from the pages when the wiki's head is not the commit it was made at, so
//! an edit, a move, a delete or a push from elsewhere (each of which sends `wiki_changed`) is found at the next search.

use std::collections::HashMap;

use serde::Serialize;

use super::{Seen, Wiki, project_key};
use crate::app::{App, AppError, Result};
use crate::decide::Actor;
use crate::store::{IndexedPage, match_all};

/// The most pages a search answers with, unless asked for fewer.
pub const SEARCH_LIMIT: usize = 20;

/// How many matches are read before the pages the person may not see are left out; a private project's pages never
/// take the place of a page that is shown.
const CANDIDATES: usize = 500;

/// A page a search found.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SearchHit {
    /// The page's path without `.md`.
    pub path: String,
    /// The page's name.
    pub name: String,
    /// The key of the project the page is under; `null` for the organisation's pages.
    pub project: Option<String>,
    /// The words around the match, as plain text.
    pub snippet: String,
}

impl Wiki {
    /// Makes the index again from the pages if the wiki's head has moved since it was made.
    fn sync_index(&self, app: &App) -> Result<()> {
        // The head is read before the pages: pages read after a newer commit are newer than the commit the index is
        // marked with, so the next search makes it again rather than serving stale words.
        let Some(head) = self.head() else {
            if app.store.wiki_indexed_commit()?.is_some() {
                app.store.replace_wiki_index("", &[])?;
            }
            return Ok(());
        };
        if app.store.wiki_indexed_commit()?.as_deref() == Some(head.as_str()) {
            return Ok(());
        }
        let mut indexed = Vec::new();
        for info in self.pages()? {
            // A page deleted since the list was read is not in the index; any other failure stops the rebuild, so
            // the index is not marked current without a page and the next search tries again.
            let page = match self.page(&info.path, None) {
                Ok(page) => page,
                Err(AppError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            };
            indexed.push(IndexedPage {
                project: project_key(&info.path).map(str::to_string),
                path: info.path,
                title: info.name,
                body: page.text,
            });
        }
        app.store.replace_wiki_index(&head, &indexed)?;
        Ok(())
    }

    /// The pages `who` may see that match `query`, the best first, a title match before a match in the text. With
    /// `project`, only the pages under `projects/<project>/`. At most `limit` (`SEARCH_LIMIT` when none is given).
    pub fn search(
        &self,
        app: &App,
        who: &Actor,
        query: &str,
        project: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<SearchHit>> {
        let seen = Seen::of(app, who)?;
        self.search_among(app, |key| seen.project(app, key), query, project, limit)
    }

    /// [`Wiki::search`] with the projects whose pages are shown decided by `may_see`, given a project's key.
    fn search_among(
        &self,
        app: &App,
        may_see: impl Fn(&str) -> bool,
        query: &str,
        project: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<SearchHit>> {
        let limit = limit.unwrap_or(SEARCH_LIMIT).clamp(1, 100);
        let Some(matching) = match_all(query) else {
            return Ok(Vec::new());
        };
        self.sync_index(app)?;
        let mut may: HashMap<String, bool> = HashMap::new();
        let mut hits = Vec::new();
        for hit in app.store.search_wiki(&matching, project, CANDIDATES)? {
            if let Some(key) = &hit.project
                && !*may.entry(key.clone()).or_insert_with(|| may_see(key))
            {
                continue;
            }
            hits.push(SearchHit {
                name: hit.title,
                path: hit.path,
                project: hit.project,
                snippet: hit.snippet,
            });
            if hits.len() == limit {
                break;
            }
        }
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::Store;
    use crate::wiki::Author;
    use crate::wiki::testing::{bare_origin, push_from_elsewhere};

    fn setup() -> (tempfile::TempDir, App, Wiki, Author, Actor) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let app = App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(5))),
            &dir.path().join("data"),
        );
        let origin = bare_origin(dir.path());
        let wiki = Wiki::open(
            &app,
            &origin.to_string_lossy(),
            &dir.path().join("data/wiki"),
        )
        .unwrap();
        let author = Author {
            name: "Lea".into(),
            handle: "lea".into(),
        };
        (dir, app, wiki, author, Actor::person(1, "lea"))
    }

    fn projects(app: &App, keys: &[&str]) {
        for key in keys {
            app.store
                .add_project(&format!("Project {key}"), key)
                .unwrap();
        }
    }

    fn paths(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|h| h.path.as_str()).collect()
    }

    #[test]
    fn a_page_is_found_by_its_text_and_a_title_comes_first() {
        let (_d, app, wiki, by, who) = setup();
        wiki.write_page(
            &app,
            &by,
            "Runbook",
            "refunds refunds refunds refunds are paid",
            None,
        )
        .unwrap();
        wiki.write_page(&app, &by, "Refunds", "How money goes back.", None)
            .unwrap();
        wiki.write_page(&app, &by, "Other", "Nothing to see", None)
            .unwrap();
        let hits = wiki.search(&app, &who, "refunds", None, None).unwrap();
        assert_eq!(paths(&hits), ["Refunds", "Runbook"]);
        assert_eq!(hits[0].name, "Refunds");
        assert!(hits[1].snippet.contains("refunds"), "{:?}", hits[1]);
        assert!(
            wiki.search(&app, &who, "  ", None, None)
                .unwrap()
                .is_empty()
        );
        assert!(
            wiki.search(&app, &who, "absent", None, None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn an_edited_moved_and_deleted_page_is_found_as_it_is_now() {
        let (d, app, wiki, by, who) = setup();
        wiki.write_page(&app, &by, "Notes", "alpha beta", None)
            .unwrap();
        assert_eq!(
            wiki.search(&app, &who, "alpha", None, None).unwrap().len(),
            1
        );
        wiki.write_page(&app, &by, "Notes", "gamma delta", None)
            .unwrap();
        assert!(
            wiki.search(&app, &who, "alpha", None, None)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            paths(&wiki.search(&app, &who, "gamma", None, None).unwrap()),
            ["Notes"]
        );
        wiki.move_page(&app, &by, "Notes", "Team/Notes").unwrap();
        assert_eq!(
            paths(&wiki.search(&app, &who, "gamma", None, None).unwrap()),
            ["Team/Notes"]
        );
        // A page pushed from elsewhere is found after the refresh that reads it.
        let origin = d.path().join("origin.git");
        push_from_elsewhere(d.path(), &origin, "Elsewhere.md", "epsilon\n");
        wiki.refresh(&app).unwrap();
        assert_eq!(
            paths(&wiki.search(&app, &who, "epsilon", None, None).unwrap()),
            ["Elsewhere"]
        );
        wiki.delete_page(&app, &by, "Team/Notes").unwrap();
        assert!(
            wiki.search(&app, &who, "gamma", None, None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_search_is_across_the_organisation_or_in_one_project() {
        let (_d, app, wiki, by, who) = setup();
        projects(&app, &["WEB", "API"]);
        wiki.write_page(&app, &by, "Setup", "install the tools", None)
            .unwrap();
        wiki.write_page(&app, &by, "projects/WEB/Setup", "install the tools", None)
            .unwrap();
        wiki.write_page(&app, &by, "projects/API/Setup", "install the tools", None)
            .unwrap();
        let all = wiki.search(&app, &who, "tools", None, None).unwrap();
        assert_eq!(all.len(), 3);
        let web = wiki.search(&app, &who, "tools", Some("WEB"), None).unwrap();
        assert_eq!(paths(&web), ["projects/WEB/Setup"]);
        assert_eq!(web[0].project.as_deref(), Some("WEB"));
        assert_eq!(
            wiki.search(&app, &who, "tools", None, Some(2))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn a_page_of_a_project_the_person_may_not_see_is_never_returned() {
        let (_d, app, wiki, by, _who) = setup();
        projects(&app, &["WEB", "API"]);
        wiki.write_page(&app, &by, "Public", "secret plans", None)
            .unwrap();
        wiki.write_page(&app, &by, "projects/WEB/Plans", "secret plans", None)
            .unwrap();
        wiki.write_page(&app, &by, "projects/API/Plans", "secret plans", None)
            .unwrap();
        let hidden = |key: &str| key != "WEB";
        let all = wiki
            .search_among(&app, hidden, "secret", None, None)
            .unwrap();
        assert_eq!(paths(&all), ["Public", "projects/API/Plans"]);
        // Asking for the hidden project by name finds nothing, and the hidden pages do not use up the limit.
        assert!(
            wiki.search_among(&app, hidden, "secret", Some("WEB"), None)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            wiki.search_among(&app, hidden, "secret", None, Some(2))
                .unwrap()
                .len(),
            2
        );
    }
}
