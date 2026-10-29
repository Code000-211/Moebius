//! Grillings: what a grilling chat ended in (see `grilling`), the issues it proposes or the report it wrote.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// One issue a grilling proposes: `blocked_by` are indexes into the list the issue is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrillingIssue {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub blocked_by: Vec<usize>,
}

/// What a grilling chat ended in (see `grilling`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grilling {
    pub id: i64,
    pub run_id: i64,
    pub repository_id: i64,
    /// The issues proposed; none for a report and for an empty grilling.
    pub issues: Vec<GrillingIssue>,
    /// The research report, for a grilling that ended in one.
    pub report: Option<String>,
    /// `proposed`, `reported`, `empty`, `confirmed` or `dismissed`.
    pub status: String,
    /// Why a grilling is `empty`.
    pub reason: String,
    pub at: i64,
}

impl Store {
    pub fn add_grilling(
        &self,
        run_id: i64,
        repository_id: i64,
        issues: &[GrillingIssue],
        report: Option<&str>,
        status: &str,
        reason: &str,
    ) -> Result<Grilling> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO grillings (run_id, repository_id, issues, report, status, reason, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run_id,
                repository_id,
                serde_json::to_string(issues)?,
                report,
                status,
                reason,
                now()
            ],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        Ok(self.grilling(id)?.expect("the grilling was just stored"))
    }

    fn grillings_where(
        &self,
        clause: &str,
        param: rusqlite::types::Value,
    ) -> Result<Vec<Grilling>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, run_id, repository_id, issues, report, status, reason, at
             FROM grillings WHERE {clause} ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map([param], |r| {
            Ok((
                Grilling {
                    id: r.get(0)?,
                    run_id: r.get(1)?,
                    repository_id: r.get(2)?,
                    issues: Vec::new(),
                    report: r.get(4)?,
                    status: r.get(5)?,
                    reason: r.get(6)?,
                    at: r.get(7)?,
                },
                r.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (mut grilling, issues) = row?;
            grilling.issues = serde_json::from_str(&issues)?;
            Ok(grilling)
        })
        .collect()
    }

    pub fn grilling(&self, id: i64) -> Result<Option<Grilling>> {
        Ok(self.grillings_where("id = ?1", id.into())?.pop())
    }

    /// All grillings, the newest first.
    pub fn grillings(&self) -> Result<Vec<Grilling>> {
        self.grillings_where("?1", 1.into())
    }

    /// Gives a grilling that has the status `from` the status `to`; false, and nothing changed, when it has another.
    pub fn move_grilling(&self, id: i64, from: &[&str], to: &str) -> Result<bool> {
        self.move_status(
            "grillings",
            "status = ?",
            vec![to.to_string().into()],
            id,
            from,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store as store;

    #[test]
    fn a_grilling_is_stored_with_its_issues_and_changes_status_once() {
        let (_dir, s) = store();
        let repo = s
            .add_repository("demo", "/tmp/demo", "none", "main")
            .unwrap();
        let run = s
            .add_run(None, "Grilling-partner", "pi-bolt", "chat")
            .unwrap();
        let issues = [GrillingIssue {
            title: "One".into(),
            body: "Body".into(),
            blocked_by: vec![],
        }];
        let grilling = s
            .add_grilling(run.id, repo.id, &issues, None, "proposed", "")
            .unwrap();
        assert_eq!(s.grilling(grilling.id).unwrap().unwrap().issues, issues);
        assert!(
            s.move_grilling(grilling.id, &["proposed"], "confirmed")
                .unwrap()
        );
        assert!(
            !s.move_grilling(grilling.id, &["proposed"], "dismissed")
                .unwrap()
        );
        assert_eq!(s.grillings().unwrap()[0].status, "confirmed");
    }
}
