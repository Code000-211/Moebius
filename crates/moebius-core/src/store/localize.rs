//! The search index of the `localize` decision point (see `decide::localize`): for each repository the files of its
//! default branch in an FTS5 table of their own, split into words for ranking by BM25, and the commit the index was
//! made from, so that it is made again only once the default branch has moved.

use rusqlite::params;

use super::{Result, Store, now};

/// One file of the default branch as the index holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFile {
    pub path: String,
    /// The words of the path, identifiers split on camelCase and snake_case.
    pub path_words: String,
    /// The words of the contents, in the same way, whole identifiers included.
    pub words: String,
}

/// The name of the table that indexes `repository_id`; made of a number, so that it needs no quoting. Visible to
/// the rest of the store so that deleting a repository can drop its index.
pub(super) fn table(repository_id: i64) -> String {
    format!("localize_files_{repository_id}")
}

impl Store {
    /// The commit of the default branch the repository's index was made from; none when it has none.
    pub fn localize_index_commit(&self, repository_id: i64) -> Result<Option<String>> {
        Ok(self
            .rows(
                "SELECT commit_id FROM localize_indexed WHERE repository_id = ?1",
                [repository_id],
                |r| r.get(0),
            )?
            .pop())
    }

    /// Makes `files` the repository's index, made from `commit`, in place of the one it had.
    pub fn replace_localize_index(
        &self,
        repository_id: i64,
        commit: &str,
        files: &[IndexedFile],
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let table = table(repository_id);
        tx.execute_batch(&format!(
            "DROP TABLE IF EXISTS {table};
             CREATE VIRTUAL TABLE {table} USING fts5(path UNINDEXED, path_words, words, tokenize = \"unicode61 tokenchars '_'\");"
        ))?;
        {
            let mut insert = tx.prepare(&format!(
                "INSERT INTO {table} (path, path_words, words) VALUES (?1, ?2, ?3)"
            ))?;
            for file in files {
                insert.execute(params![file.path, file.path_words, file.words])?;
            }
        }
        tx.execute(
            "INSERT INTO localize_indexed (repository_id, commit_id, files, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (repository_id) DO UPDATE SET commit_id = ?2, files = ?3, at = ?4",
            params![repository_id, commit, files.len() as i64, now()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The paths in the repository's index, in alphabetical order.
    pub fn localize_paths(&self, repository_id: i64) -> Result<Vec<String>> {
        if self.localize_index_commit(repository_id)?.is_none() {
            return Ok(Vec::new());
        }
        self.rows(
            &format!("SELECT path FROM {} ORDER BY path", table(repository_id)),
            [],
            |r| r.get(0),
        )
    }

    /// The files that match `query` (FTS5 syntax over the words of the path and the contents), best first by BM25 with
    /// the words of the path weighing four times those of the contents, each with its score, higher being better; at
    /// most `limit`.
    pub fn localize_search(
        &self,
        repository_id: i64,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(String, f64)>> {
        if self.localize_index_commit(repository_id)?.is_none() {
            return Ok(Vec::new());
        }
        let table = table(repository_id);
        self.rows(
            &format!(
                "SELECT path, -bm25({table}, 0.0, 4.0, 1.0) AS score FROM {table}
                 WHERE {table} MATCH ?1 ORDER BY score DESC, path LIMIT ?2"
            ),
            params![query, limit as i64],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store as store;

    fn file(path: &str, words: &str) -> IndexedFile {
        IndexedFile {
            path: path.into(),
            path_words: path.replace(['/', '.'], " "),
            words: words.into(),
        }
    }

    #[test]
    fn an_index_is_replaced_whole_and_searched_by_bm25() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        assert_eq!(s.localize_index_commit(repo.id).unwrap(), None);
        assert!(
            s.localize_search(repo.id, "\"parser\"", 5)
                .unwrap()
                .is_empty()
        );

        let files = [
            file("src/parser.rs", "parse parser token token"),
            file("src/render.rs", "render draw token"),
        ];
        s.replace_localize_index(repo.id, "abc", &files).unwrap();
        assert_eq!(
            s.localize_index_commit(repo.id).unwrap().as_deref(),
            Some("abc")
        );
        let found = s
            .localize_search(repo.id, "\"token\" OR \"parser\"", 5)
            .unwrap();
        assert_eq!(found[0].0, "src/parser.rs");
        assert_eq!(found.len(), 2);
        assert!(found[0].1 > found[1].1);

        s.replace_localize_index(repo.id, "def", &files[1..])
            .unwrap();
        assert_eq!(
            s.localize_index_commit(repo.id).unwrap().as_deref(),
            Some("def")
        );
        assert_eq!(s.localize_paths(repo.id).unwrap(), ["src/render.rs"]);
    }
}
