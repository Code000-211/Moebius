//! The links the wiki's pages make (see `wiki::links`): which page leads to which page and which card. They are
//! derived data: the wiki's files hold the truth, and the whole table is made again from them whenever the wiki
//! changes. A link to a page is kept as the page's path, a link to a card as its name (`WEB-12`).

use rusqlite::params;

use super::{Card, Result, Store};

/// The table this module adds.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE wiki_links (
    source TEXT NOT NULL,
    kind TEXT NOT NULL,
    target TEXT NOT NULL,
    PRIMARY KEY (source, kind, target)
);
CREATE INDEX wiki_links_by_target ON wiki_links (kind, target);
"#;

/// The kind of a link to a page, whose target is the page's path.
pub const PAGE: &str = "page";
/// The kind of a link to a card, whose target is the card's name, such as `WEB-12`.
pub const CARD: &str = "card";

/// One link: the page `source` leads to `target`, a page's path or a card's name by `kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    pub source: String,
    pub kind: &'static str,
    pub target: String,
}

impl Store {
    /// Replaces every link of the wiki with `links`, in one step.
    pub fn replace_wiki_links(&self, links: &[WikiLink]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM wiki_links", [])?;
        {
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO wiki_links (source, kind, target) VALUES (?1, ?2, ?3)",
            )?;
            for link in links {
                insert.execute(params![link.source, link.kind, link.target])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The paths of the pages that link to `target` as a `kind`, sorted.
    pub fn wiki_links_to(&self, kind: &str, target: &str) -> Result<Vec<String>> {
        self.rows(
            "SELECT source FROM wiki_links WHERE kind = ?1 AND target = ?2 ORDER BY source",
            params![kind, target],
            |r| r.get(0),
        )
    }

    /// The cards whose text may hold a `[[link]]`, for the wiki to read the pages they lead to.
    pub fn cards_with_wiki_links(&self) -> Result<Vec<Card>> {
        self.cards_where("body LIKE '%[[%' OR title LIKE '%[[%'", [])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(source: &str, kind: &'static str, target: &str) -> WikiLink {
        WikiLink {
            source: source.into(),
            kind,
            target: target.into(),
        }
    }

    #[test]
    fn the_links_are_replaced_as_a_whole() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(dir.path()).unwrap();
        store
            .replace_wiki_links(&[
                link("A", PAGE, "B"),
                link("C", PAGE, "B"),
                link("A", CARD, "WEB-1"),
                link("A", PAGE, "B"),
            ])
            .unwrap();
        assert_eq!(store.wiki_links_to(PAGE, "B").unwrap(), ["A", "C"]);
        assert_eq!(store.wiki_links_to(CARD, "WEB-1").unwrap(), ["A"]);
        store.replace_wiki_links(&[link("D", PAGE, "B")]).unwrap();
        assert_eq!(store.wiki_links_to(PAGE, "B").unwrap(), ["D"]);
        assert!(store.wiki_links_to(CARD, "WEB-1").unwrap().is_empty());
    }
}
