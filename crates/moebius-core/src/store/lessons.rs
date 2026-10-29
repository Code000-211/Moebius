//! Lessons (see `improve::lessons`): short notes from the Retro about what went wrong or what worked on earlier cards
//! of a repository. They are kept per repository with the cards they come from, searched with SQLite FTS5 (BM25 over
//! the situation, the lesson and the advice), and retired when they are shown to be harmful or are not retrieved for a
//! long time. A retired lesson stays in the store but is no longer found.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

const COLUMNS: &str = "id, repository_id, kind, situation, lesson, advice, cards, retro_date, status, \
     retired_reason, created_at, last_retrieved_at, retrievals";

/// One lesson.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lesson {
    pub id: i64,
    pub repository_id: i64,
    /// `failure` (a failed run), `rejection` (a review sent the change back) or `success` (merged at once).
    pub kind: String,
    /// The kind of card or step the lesson is about.
    pub situation: String,
    /// What went wrong, or what worked.
    pub lesson: String,
    /// What to do.
    pub advice: String,
    /// The cards the lesson was learned from, by id.
    pub cards: Vec<i64>,
    /// The day of the Retro that wrote it; none for a lesson a person wrote.
    pub retro_date: Option<String>,
    /// `active` or `retired`.
    pub status: String,
    /// Why it was retired.
    pub retired_reason: Option<String>,
    pub created_at: i64,
    /// When it was last put in a prompt or returned by the tool.
    pub last_retrieved_at: Option<i64>,
    pub retrievals: i64,
}

/// A lesson to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewLesson {
    pub repository_id: i64,
    pub kind: String,
    pub situation: String,
    pub lesson: String,
    pub advice: String,
    pub cards: Vec<i64>,
    pub retro_date: Option<String>,
}

fn lesson_from_row(r: &rusqlite::Row) -> rusqlite::Result<Lesson> {
    let cards: String = r.get(6)?;
    Ok(Lesson {
        id: r.get(0)?,
        repository_id: r.get(1)?,
        kind: r.get(2)?,
        situation: r.get(3)?,
        lesson: r.get(4)?,
        advice: r.get(5)?,
        cards: serde_json::from_str(&cards).unwrap_or_default(),
        retro_date: r.get(7)?,
        status: r.get(8)?,
        retired_reason: r.get(9)?,
        created_at: r.get(10)?,
        last_retrieved_at: r.get(11)?,
        retrievals: r.get(12)?,
    })
}

/// The schema of lessons: the table, its full-text index kept in step by triggers, and the record of which card was
/// given which lesson.
pub(super) const SCHEMA: &str = r#"
CREATE TABLE lessons (
    id INTEGER PRIMARY KEY,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    kind TEXT NOT NULL,
    situation TEXT NOT NULL,
    lesson TEXT NOT NULL,
    advice TEXT NOT NULL,
    cards TEXT NOT NULL DEFAULT '[]',
    retro_date TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    retired_reason TEXT,
    created_at INTEGER NOT NULL,
    last_retrieved_at INTEGER,
    retrievals INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX lessons_by_repository ON lessons (repository_id, status);
CREATE VIRTUAL TABLE lessons_fts USING fts5(
    situation, lesson, advice, content = 'lessons', content_rowid = 'id', tokenize = 'porter unicode61'
);
CREATE TRIGGER lessons_added AFTER INSERT ON lessons BEGIN
    INSERT INTO lessons_fts (rowid, situation, lesson, advice) VALUES (new.id, new.situation, new.lesson, new.advice);
END;
CREATE TRIGGER lessons_removed AFTER DELETE ON lessons BEGIN
    INSERT INTO lessons_fts (lessons_fts, rowid, situation, lesson, advice)
    VALUES ('delete', old.id, old.situation, old.lesson, old.advice);
END;
CREATE TRIGGER lessons_changed AFTER UPDATE OF situation, lesson, advice ON lessons BEGIN
    INSERT INTO lessons_fts (lessons_fts, rowid, situation, lesson, advice)
    VALUES ('delete', old.id, old.situation, old.lesson, old.advice);
    INSERT INTO lessons_fts (rowid, situation, lesson, advice) VALUES (new.id, new.situation, new.lesson, new.advice);
END;
CREATE TABLE lesson_uses (
    id INTEGER PRIMARY KEY,
    lesson_id INTEGER NOT NULL REFERENCES lessons(id) ON DELETE CASCADE,
    card_id INTEGER NOT NULL,
    at INTEGER NOT NULL
);
CREATE INDEX lesson_uses_by_card ON lesson_uses (card_id, at);
"#;

/// Words too common to say what a text is about.
const STOP_WORDS: [&str; 24] = [
    "the", "and", "for", "with", "that", "this", "from", "are", "was", "not", "but", "you", "can",
    "has", "have", "its", "into", "when", "what", "then", "than", "will", "should", "would",
];

/// An FTS5 query matching any of the words of `text`: words of three letters or more that are not common, each
/// quoted, at most 48 of them.
fn match_any(text: &str) -> Option<String> {
    let mut words: Vec<String> = Vec::new();
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
    {
        let word = word.to_lowercase();
        if !words.contains(&word) && !STOP_WORDS.contains(&word.as_str()) {
            words.push(word);
        }
        if words.len() == 48 {
            break;
        }
    }
    if words.is_empty() {
        return None;
    }
    Some(
        words
            .iter()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

impl Store {
    /// Stores a lesson. A lesson with the same situation and advice in the repository, active or not, is not stored
    /// again: the call answers none.
    pub fn add_lesson(&self, new: &NewLesson) -> Result<Option<Lesson>> {
        let duplicate: Option<i64> = self
            .conn()
            .query_row(
                "SELECT id FROM lessons WHERE repository_id = ?1 AND situation = ?2 AND advice = ?3",
                params![new.repository_id, new.situation, new.advice],
                |r| r.get(0),
            )
            .optional()?;
        if duplicate.is_some() {
            return Ok(None);
        }
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO lessons (repository_id, kind, situation, lesson, advice, cards, retro_date, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    new.repository_id,
                    new.kind,
                    new.situation,
                    new.lesson,
                    new.advice,
                    serde_json::to_string(&new.cards)?,
                    new.retro_date,
                    now()
                ],
            )?;
            conn.last_insert_rowid()
        };
        self.lesson(id)
    }

    pub fn lesson(&self, id: i64) -> Result<Option<Lesson>> {
        Ok(self
            .rows(
                &format!("SELECT {COLUMNS} FROM lessons WHERE id = ?1"),
                [id],
                lesson_from_row,
            )?
            .pop())
    }

    /// The lessons, the newest first: those of `repository` when given, and the retired ones only when asked for.
    pub fn lessons(&self, repository: Option<i64>, retired: bool) -> Result<Vec<Lesson>> {
        self.rows(
            &format!(
                "SELECT {COLUMNS} FROM lessons WHERE (?1 IS NULL OR repository_id = ?1) AND (?2 OR status = 'active')
                 ORDER BY id DESC"
            ),
            params![repository, retired],
            lesson_from_row,
        )
    }

    /// Rewrites the text of a lesson; false when there is none.
    pub fn edit_lesson(
        &self,
        id: i64,
        situation: &str,
        lesson: &str,
        advice: &str,
    ) -> Result<bool> {
        Ok(self.conn().execute(
            "UPDATE lessons SET situation = ?1, lesson = ?2, advice = ?3 WHERE id = ?4",
            params![situation, lesson, advice, id],
        )? == 1)
    }

    /// Deletes a lesson; false when there is none.
    pub fn delete_lesson(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM lessons WHERE id = ?1", [id])?
            == 1)
    }

    /// The active lessons of the repository that best match `text`, best first by BM25, at most `limit`. A search
    /// leaves no trace; `note_lessons_used` does.
    pub fn search_lessons(
        &self,
        repository_id: i64,
        text: &str,
        limit: usize,
    ) -> Result<Vec<Lesson>> {
        let Some(query) = match_any(text) else {
            return Ok(Vec::new());
        };
        let columns = COLUMNS
            .split(", ")
            .map(|c| format!("l.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.rows(
            &format!(
                "SELECT {columns} FROM lessons_fts f JOIN lessons l ON l.id = f.rowid
                 WHERE lessons_fts MATCH ?1 AND l.repository_id = ?2 AND l.status = 'active'
                 ORDER BY bm25(lessons_fts), l.id DESC LIMIT ?3"
            ),
            params![query, repository_id, limit as i64],
            lesson_from_row,
        )
    }

    /// Notes that these lessons were given to a run, of `card_id` when it has a card, which keeps them from being
    /// retired for lack of use.
    pub fn note_lessons_used(&self, ids: &[i64], card_id: Option<i64>) -> Result<()> {
        let at = now();
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE lessons SET last_retrieved_at = ?1, retrievals = retrievals + 1 WHERE id = ?2",
                params![at, id],
            )?;
            if let Some(card_id) = card_id {
                tx.execute(
                    "INSERT INTO lesson_uses (lesson_id, card_id, at) VALUES (?1, ?2, ?3)",
                    params![id, card_id, at],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The active lessons that were given to the card's runs since `since`.
    pub fn lessons_used_since(&self, card_id: i64, since: i64) -> Result<Vec<i64>> {
        self.rows(
            "SELECT DISTINCT u.lesson_id FROM lesson_uses u JOIN lessons l ON l.id = u.lesson_id
             WHERE u.card_id = ?1 AND u.at >= ?2 AND l.status = 'active' ORDER BY u.lesson_id",
            params![card_id, since],
            |r| r.get(0),
        )
    }

    /// Retires an active lesson for `reason`; false when it is not active.
    pub fn retire_lesson(&self, id: i64, reason: &str) -> Result<bool> {
        self.move_status(
            "lessons",
            "status = 'retired', retired_reason = ?",
            vec![reason.to_string().into()],
            id,
            &["active"],
        )
    }

    /// Retires the active lessons that were last retrieved before `before`, or, when never retrieved, were made
    /// before it; answers their ids.
    pub fn retire_unused_lessons(&self, before: i64, reason: &str) -> Result<Vec<i64>> {
        let ids: Vec<i64> = self.rows(
            "SELECT id FROM lessons WHERE status = 'active' AND COALESCE(last_retrieved_at, created_at) < ?1",
            [before],
            |r| r.get(0),
        )?;
        for id in &ids {
            self.retire_lesson(*id, reason)?;
        }
        Ok(ids)
    }

    /// Sets when a lesson was created and last retrieved; for tests of retirement.
    #[cfg(test)]
    pub(crate) fn age_lesson(&self, id: i64, created_at: i64, last_retrieved_at: Option<i64>) {
        self.conn()
            .execute(
                "UPDATE lessons SET created_at = ?1, last_retrieved_at = ?2 WHERE id = ?3",
                params![created_at, last_retrieved_at, id],
            )
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store as store;

    fn new(repository_id: i64, situation: &str, lesson: &str, advice: &str) -> NewLesson {
        NewLesson {
            repository_id,
            kind: "failure".into(),
            situation: situation.into(),
            lesson: lesson.into(),
            advice: advice.into(),
            cards: vec![7],
            retro_date: Some("2026-03-10".into()),
        }
    }

    #[test]
    fn lessons_are_found_by_their_words_and_only_in_their_repository() {
        let (_dir, s) = store();
        let a = s.add_repository("a", "/a", "none", "main").unwrap();
        let b = s.add_repository("b", "/b", "none", "main").unwrap();
        let parser = s
            .add_lesson(&new(
                a.id,
                "Changing the parser for a new token",
                "The tokenizer tests broke",
                "Run the tokenizer tests first",
            ))
            .unwrap()
            .unwrap();
        s.add_lesson(&new(
            a.id,
            "Adding a database migration",
            "The schema version was not bumped",
            "Bump the schema version",
        ))
        .unwrap();
        s.add_lesson(&new(b.id, "Parser work", "x", "y")).unwrap();

        let found = s
            .search_lessons(a.id, "Parser fails on a new token", 5)
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, parser.id);
        assert_eq!(found[0].cards, [7]);
        // Words are stemmed, and a text with no word of three letters finds nothing.
        assert_eq!(s.search_lessons(a.id, "migrations", 5).unwrap().len(), 1);
        assert!(s.search_lessons(a.id, "a b", 5).unwrap().is_empty());
        assert!(
            s.search_lessons(a.id, "\"quote\" OR ) (", 5)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            s.search_lessons(a.id, "the schema parser", 1)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn an_edit_is_searchable_and_a_deleted_lesson_is_gone() {
        let (_dir, s) = store();
        let repo = s.add_repository("a", "/a", "none", "main").unwrap();
        let lesson = s
            .add_lesson(&new(repo.id, "Old words", "old", "old advice"))
            .unwrap()
            .unwrap();
        assert!(
            s.add_lesson(&new(repo.id, "Old words", "x", "old advice"))
                .unwrap()
                .is_none()
        );
        assert!(
            s.edit_lesson(lesson.id, "Fresh words", "fresh", "do it")
                .unwrap()
        );
        assert!(s.search_lessons(repo.id, "old", 5).unwrap().is_empty());
        assert_eq!(s.search_lessons(repo.id, "fresh", 5).unwrap().len(), 1);
        assert!(s.delete_lesson(lesson.id).unwrap());
        assert!(!s.delete_lesson(lesson.id).unwrap());
        assert!(s.search_lessons(repo.id, "fresh", 5).unwrap().is_empty());
        assert!(!s.edit_lesson(lesson.id, "a", "b", "c").unwrap());
    }

    #[test]
    fn a_retired_lesson_is_not_found_and_use_keeps_a_lesson_alive() {
        let (_dir, s) = store();
        let repo = s.add_repository("a", "/a", "none", "main").unwrap();
        let card = s.add_card(repo.id, "Card", "", "leon", "Backlog").unwrap();
        let used = s
            .add_lesson(&new(repo.id, "Parser work", "p", "do p"))
            .unwrap()
            .unwrap();
        let idle = s
            .add_lesson(&new(repo.id, "Parser idle", "q", "do q"))
            .unwrap()
            .unwrap();
        let day = 86_400;
        s.age_lesson(used.id, now() - 90 * day, None);
        s.age_lesson(idle.id, now() - 90 * day, None);
        s.note_lessons_used(&[used.id], Some(card.id)).unwrap();
        assert_eq!(s.lessons_used_since(card.id, 0).unwrap(), [used.id]);
        assert_eq!(s.lesson(used.id).unwrap().unwrap().retrievals, 1);

        let retired = s.retire_unused_lessons(now() - 60 * day, "unused").unwrap();
        assert_eq!(retired, [idle.id]);
        let idle = s.lesson(idle.id).unwrap().unwrap();
        assert_eq!(
            (idle.status.as_str(), idle.retired_reason.as_deref()),
            ("retired", Some("unused"))
        );
        assert_eq!(s.search_lessons(repo.id, "parser", 5).unwrap().len(), 1);
        assert_eq!(s.lessons(Some(repo.id), false).unwrap().len(), 1);
        assert_eq!(s.lessons(None, true).unwrap().len(), 2);
        assert!(!s.retire_lesson(idle.id, "again").unwrap());
    }
}
