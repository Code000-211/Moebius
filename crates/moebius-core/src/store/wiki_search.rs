//! The search index of the wiki (ADR 0022): derived data, made again from the pages whenever the wiki's head moves.
//! One row of SQLite FTS5 per page, with its title (the page's name) and its text, and the commit the rows were made
//! at; a title counts for far more than a word of the text. Nothing here is the source of a page: the index can be
//! thrown away and rebuilt from the clone (see `wiki::search`).

use rusqlite::{OptionalExtension, params};

use super::{Result, Store};

/// The tables of this part.
pub(super) const SCHEMA: &str = r#"
CREATE VIRTUAL TABLE wiki_pages_fts USING fts5(
    path UNINDEXED, project UNINDEXED, title, body, tokenize = 'porter unicode61'
);
CREATE TABLE wiki_indexed (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    commit_id TEXT NOT NULL
);
"#;

/// How much more a word of the title counts than a word of the text.
const TITLE_WEIGHT: f64 = 12.0;

/// A page to put in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedPage {
    pub path: String,
    /// The key of the project the page is under; none for the organisation's pages.
    pub project: Option<String>,
    pub title: String,
    pub body: String,
}

/// A page the index found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiHit {
    pub path: String,
    pub project: Option<String>,
    pub title: String,
    /// The part of the text around the words found, plain text.
    pub snippet: String,
}

/// An FTS5 query for the words of `text`: every word must be found, the last as a beginning so that a person still
/// typing is found. `None` when `text` holds no word.
pub(crate) fn match_all(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(16)
        .map(str::to_lowercase)
        .collect();
    let last = words.len().checked_sub(1)?;
    Some(
        words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                if i == last {
                    format!("\"{w}\"*")
                } else {
                    format!("\"{w}\"")
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

impl Store {
    /// The commit the index was made at; none before it was made.
    pub fn wiki_indexed_commit(&self) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row("SELECT commit_id FROM wiki_indexed WHERE id = 1", [], |r| {
                r.get(0)
            })
            .optional()?)
    }

    /// Replaces the whole index with `pages`, made at `commit`, in one transaction: a search sees the old index or the
    /// new one, never half of each.
    pub fn replace_wiki_index(&self, commit: &str, pages: &[IndexedPage]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM wiki_pages_fts", [])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO wiki_pages_fts (path, project, title, body) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for page in pages {
                insert.execute(params![page.path, page.project, page.title, page.body])?;
            }
        }
        tx.execute(
            "INSERT INTO wiki_indexed (id, commit_id) VALUES (1, ?1)
             ON CONFLICT (id) DO UPDATE SET commit_id = excluded.commit_id",
            [commit],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The pages that match the FTS5 query `matching`, the best first (a word of the title counts far more than one
    /// of the text); only those of the project `project` when given. At most `limit`.
    pub fn search_wiki(
        &self,
        matching: &str,
        project: Option<&str>,
        limit: usize,
    ) -> Result<Vec<WikiHit>> {
        self.rows(
            "SELECT path, project, title, snippet(wiki_pages_fts, 3, '', '', '…', 16)
             FROM wiki_pages_fts
             WHERE wiki_pages_fts MATCH ?1 AND (?2 IS NULL OR project = ?2)
             ORDER BY bm25(wiki_pages_fts, 0.0, 0.0, ?3, 1.0), path
             LIMIT ?4",
            params![matching, project, TITLE_WEIGHT, limit as i64],
            |r| {
                Ok(WikiHit {
                    path: r.get(0)?,
                    project: r.get(1)?,
                    title: r.get(2)?,
                    snippet: r.get(3)?,
                })
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(path: &str, title: &str, body: &str) -> IndexedPage {
        IndexedPage {
            path: path.into(),
            project: crate::wiki::project_key(path).map(str::to_string),
            title: title.into(),
            body: body.into(),
        }
    }

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn a_query_is_its_words_with_the_last_as_a_beginning() {
        assert_eq!(
            match_all("  On-call rota ").unwrap(),
            "\"on\" \"call\" \"rota\"*"
        );
        assert_eq!(match_all("a\"b OR").unwrap(), "\"a\" \"b\" \"or\"*");
        assert_eq!(match_all(" -*() "), None);
    }

    #[test]
    fn a_page_is_found_by_a_word_of_its_text_and_a_title_ranks_first() {
        let (_dir, store) = store();
        store
            .replace_wiki_index(
                "c1",
                &[
                    page(
                        "Runbook",
                        "Runbook",
                        "refunds refunds refunds refunds refunds are paid",
                    ),
                    page("Refunds", "Refunds", "How money goes back."),
                    page("Other", "Other", "nothing here"),
                ],
            )
            .unwrap();
        let hits = store
            .search_wiki(&match_all("refunds").unwrap(), None, 10)
            .unwrap();
        let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, ["Refunds", "Runbook"]);
        assert!(hits[1].snippet.contains("refunds"));
        // A word still being typed finds it, and a stem finds the plural.
        assert_eq!(
            store
                .search_wiki(&match_all("refu").unwrap(), None, 10)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            store
                .search_wiki(&match_all("refund").unwrap(), None, 10)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn a_new_index_replaces_the_old_and_remembers_its_commit() {
        let (_dir, store) = store();
        assert_eq!(store.wiki_indexed_commit().unwrap(), None);
        store
            .replace_wiki_index("c1", &[page("A", "A", "old words")])
            .unwrap();
        store
            .replace_wiki_index("c2", &[page("A", "A", "fresh words")])
            .unwrap();
        assert_eq!(store.wiki_indexed_commit().unwrap().as_deref(), Some("c2"));
        assert!(
            store
                .search_wiki(&match_all("old").unwrap(), None, 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store
                .search_wiki(&match_all("fresh").unwrap(), None, 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn a_search_can_be_kept_to_one_project() {
        let (_dir, store) = store();
        store
            .replace_wiki_index(
                "c1",
                &[
                    page("Setup", "Setup", "install the tools"),
                    page("projects/WEB/Setup", "Setup", "install the tools"),
                    page("projects/API/Setup", "Setup", "install the tools"),
                ],
            )
            .unwrap();
        let query = match_all("tools").unwrap();
        assert_eq!(store.search_wiki(&query, None, 10).unwrap().len(), 3);
        let web = store.search_wiki(&query, Some("WEB"), 10).unwrap();
        assert_eq!(web.len(), 1);
        assert_eq!(web[0].path, "projects/WEB/Setup");
        assert_eq!(store.search_wiki(&query, None, 2).unwrap().len(), 2);
    }
}
