//! Intake (ADR 0019): a project that takes requests has a form anyone who may see it fills in, a viewer too. A request
//! is the person's own words; a grilling turns it into a draft issue, which a member of the project accepts (it becomes a
//! card the requester follows), edits or declines (with a reason the requester is told). The flow is in `app::intake`.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The tables of this part: a project's switch for intake and the requests made to it.
pub(super) const SCHEMA: &str = r#"
ALTER TABLE projects ADD COLUMN intake INTEGER NOT NULL DEFAULT 0;
CREATE TABLE intake_requests (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    requester_id INTEGER NOT NULL REFERENCES people(id),
    text TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL,
    run_id INTEGER REFERENCES runs(id),
    card_id INTEGER REFERENCES cards(id),
    reason TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    decided_at INTEGER
);
CREATE INDEX intake_requests_by_project ON intake_requests (project_id);
CREATE INDEX intake_requests_by_run ON intake_requests (run_id);
"#;

/// A request to a project. `status` is `grilling` while the grilling chat works on it, `drafted` once it has a draft
/// issue (`title` and `body`) a member may edit, `accepted` once a card was made of it, or `declined` with the
/// `reason` the requester was told.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntakeRequest {
    pub id: i64,
    pub project_id: i64,
    /// The handle of the person who made the request.
    pub requester: String,
    /// What they wrote, in their own words.
    pub text: String,
    /// The draft issue: empty while the request is `grilling`.
    pub title: String,
    pub body: String,
    pub status: String,
    /// The grilling chat that drafts the issue, if one was started.
    pub run_id: Option<i64>,
    /// The card an accepted request became.
    pub card_id: Option<i64>,
    /// Why a declined request was declined.
    pub reason: String,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

const COLUMNS: &str = "r.id, r.project_id, p.handle, r.text, r.title, r.body, r.status, r.run_id, r.card_id, \
    r.reason, r.created_at, r.decided_at";
const FROM: &str = "intake_requests r JOIN people p ON p.id = r.requester_id";

fn request(row: &Row) -> rusqlite::Result<IntakeRequest> {
    Ok(IntakeRequest {
        id: row.get(0)?,
        project_id: row.get(1)?,
        requester: row.get(2)?,
        text: row.get(3)?,
        title: row.get(4)?,
        body: row.get(5)?,
        status: row.get(6)?,
        run_id: row.get(7)?,
        card_id: row.get(8)?,
        reason: row.get(9)?,
        created_at: row.get(10)?,
        decided_at: row.get(11)?,
    })
}

impl Store {
    /// Turns the project's intake on or off. Requests already made stay.
    pub fn set_project_intake(&self, project_id: i64, intake: bool) -> Result<()> {
        self.conn().execute(
            "UPDATE projects SET intake = ?2 WHERE id = ?1",
            params![project_id, intake],
        )?;
        Ok(())
    }

    /// Stores a request of the person `requester_id` to the project, `grilling`.
    pub fn add_request(
        &self,
        project_id: i64,
        requester_id: i64,
        text: &str,
    ) -> Result<IntakeRequest> {
        let id = {
            let conn = self.conn();
            conn.execute(
                "INSERT INTO intake_requests (project_id, requester_id, text, status, created_at) \
                 VALUES (?1, ?2, ?3, 'grilling', ?4)",
                params![project_id, requester_id, text, now()],
            )?;
            conn.last_insert_rowid()
        };
        Ok(self.request(id)?.expect("the request was just stored"))
    }

    pub fn request(&self, id: i64) -> Result<Option<IntakeRequest>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM {FROM} WHERE r.id = ?1"),
                [id],
                request,
            )
            .optional()?)
    }

    /// The requests of the project, the newest first; only those of the person `requester_id` when given.
    pub fn requests(
        &self,
        project_id: i64,
        requester_id: Option<i64>,
    ) -> Result<Vec<IntakeRequest>> {
        let conn = self.read();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM {FROM} WHERE r.project_id = ?1 AND (?2 IS NULL OR r.requester_id = ?2) \
             ORDER BY r.id DESC"
        ))?;
        let rows = stmt.query_map(params![project_id, requester_id], request)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The request the grilling chat `run_id` drafts.
    pub fn request_for_run(&self, run_id: i64) -> Result<Option<IntakeRequest>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLUMNS} FROM {FROM} WHERE r.run_id = ?1"),
                [run_id],
                request,
            )
            .optional()?)
    }

    /// Names the chat that drafts the request.
    pub fn set_request_run(&self, id: i64, run_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE intake_requests SET run_id = ?2 WHERE id = ?1",
            params![id, run_id],
        )?;
        Ok(())
    }

    /// Gives a request that is `grilling` or `drafted` the draft `title` and `body` and the status `drafted`; false,
    /// and nothing changed, when it is decided already.
    pub fn set_request_draft(&self, id: i64, title: &str, body: &str) -> Result<bool> {
        self.move_status(
            "intake_requests",
            "title = ?, body = ?, status = 'drafted'",
            vec![title.to_string().into(), body.to_string().into()],
            id,
            &["grilling", "drafted"],
        )
    }

    /// Claims a drafted request for acceptance: it becomes `accepted`, so that a second acceptance finds it taken; false
    /// when it was not `drafted`. The card is named with [`Store::set_request_card`] once made, or the request given
    /// back with [`Store::reopen_request`] when it could not be.
    pub fn accept_request(&self, id: i64) -> Result<bool> {
        self.move_status(
            "intake_requests",
            "status = 'accepted', decided_at = ?",
            vec![now().into()],
            id,
            &["drafted"],
        )
    }

    /// Names the card an accepted request became.
    pub fn set_request_card(&self, id: i64, card_id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE intake_requests SET card_id = ?2 WHERE id = ?1",
            params![id, card_id],
        )?;
        Ok(())
    }

    /// Gives back to the members a request that was claimed for acceptance but made no card.
    pub fn reopen_request(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE intake_requests SET status = 'drafted', decided_at = NULL \
             WHERE id = ?1 AND status = 'accepted' AND card_id IS NULL",
            [id],
        )?;
        Ok(())
    }

    /// Resolves the open inbox item of the request.
    pub fn resolve_inbox_for_request(&self, id: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE inbox_items SET resolved_at = ?1 \
             WHERE kind = 'intake_request' AND json_extract(data, '$.request') = ?2 AND resolved_at IS NULL",
            params![now(), id],
        )?;
        Ok(())
    }

    /// Makes a request that is not decided `declined` with the `reason`; false when it was decided already.
    pub fn decline_request(&self, id: i64, reason: &str) -> Result<bool> {
        self.move_status(
            "intake_requests",
            "status = 'declined', reason = ?, decided_at = ?",
            vec![reason.to_string().into(), now().into()],
            id,
            &["grilling", "drafted"],
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::store::{Role, test_store};

    #[test]
    fn a_request_is_drafted_then_accepted_or_declined_once() {
        let (_dir, s) = test_store();
        let viewer = s.add_person("kim", "Kim", Role::Viewer).unwrap();
        let project = s.add_project("Shop", "SHOP").unwrap();
        assert!(!project.intake);
        s.set_project_intake(project.id, true).unwrap();
        assert!(s.project(project.id).unwrap().unwrap().intake);

        let made = s
            .add_request(project.id, viewer.id, "Add dark mode")
            .unwrap();
        assert_eq!(
            (made.status.as_str(), made.requester.as_str()),
            ("grilling", "kim")
        );
        assert!(!s.accept_request(made.id).unwrap(), "not drafted yet");
        assert!(
            s.set_request_draft(made.id, "Dark mode", "Details")
                .unwrap()
        );
        assert!(s.set_request_draft(made.id, "Dark theme", "More").unwrap());
        assert_eq!(s.request(made.id).unwrap().unwrap().title, "Dark theme");

        assert!(s.accept_request(made.id).unwrap());
        assert!(!s.accept_request(made.id).unwrap(), "taken once");
        s.reopen_request(made.id).unwrap();
        assert_eq!(s.request(made.id).unwrap().unwrap().status, "drafted");
        assert!(s.decline_request(made.id, "later").unwrap());
        assert!(!s.decline_request(made.id, "again").unwrap());
        assert!(!s.set_request_draft(made.id, "x", "y").unwrap());
        let declined = s.request(made.id).unwrap().unwrap();
        assert_eq!(
            (declined.status.as_str(), declined.reason.as_str()),
            ("declined", "later")
        );
        assert!(declined.decided_at.is_some());
    }

    #[test]
    fn requests_are_listed_per_project_and_per_requester() {
        let (_dir, s) = test_store();
        let kim = s.add_person("kim", "Kim", Role::Viewer).unwrap();
        let lou = s.add_person("lou", "Lou", Role::Viewer).unwrap();
        let shop = s.add_project("Shop", "SHOP").unwrap();
        let blog = s.add_project("Blog", "BLOG").unwrap();
        let a = s.add_request(shop.id, kim.id, "a").unwrap();
        let b = s.add_request(shop.id, lou.id, "b").unwrap();
        s.add_request(blog.id, kim.id, "c").unwrap();
        let all: Vec<i64> = s
            .requests(shop.id, None)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(all, [b.id, a.id]);
        let mine: Vec<i64> = s
            .requests(shop.id, Some(kim.id))
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(mine, [a.id]);
    }
}
