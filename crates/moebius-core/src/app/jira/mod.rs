//! The import from Jira (issue #390): a one-off, repeatable copy of a Jira project into a Moebius project. The client
//! (`repos::jira`) reads Jira; this folder maps what it read onto Moebius and applies it.
//!
//! | Jira | Moebius |
//! |---|---|
//! | issue | a card of the project's own files (it lives in Moebius only, ADR 0019), with a footer that names the Jira issue |
//! | issue type | a kind (made if the organisation has none of that name) offered by the project |
//! | custom field | a field of that kind: text, number, date or choice by its type |
//! | labels, priority, due date, story points | labels, `priority:`, `due:` and `estimate:` |
//! | reporter, assignee, comment authors | people (disabled until an owner gives them a token), or their names |
//! | comments | comments, with their time and author |
//! | sub-task or child | the card's parent |
//! | `Blocks`, `Duplicate`, any other link | a blocker, a duplicate (the duplicate is closed), a related card |
//! | sprint | a cycle of the project (a closed sprint closes its cycle) |
//! | epic | a milestone, which the epic's issues are in; the epic is no card |
//! | status category done | a closed card |
//!
//! What the import made is remembered in `store::imports`, with a note of what each record looked like: running it
//! again changes only what differs from the note and makes nothing twice. A record a person deleted in Moebius is made
//! again; one deleted in Jira is left. Open cards are not confirmed, so that nothing starts on them before a person
//! approves them. [`App::import_jira`] is the entry; it never talks to Jira (the route fetches first).

mod cards;
mod kinds;
mod people;
mod planning;
mod read;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[cfg(test)]
use super::MilestoneFields;
use super::{App, AppError, Result};
use crate::decide::Actor;
use crate::repos::jira::Fetched;
use crate::store::Repository;
#[cfg(test)]
use crate::store::{FieldDef, FieldType};

/// The name the import's records are kept under in `store::imports`.
pub(super) const SOURCE: &str = "jira";

/// What becomes of the people Jira names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Users {
    /// Each becomes a person who is disabled until an owner gives them a token.
    #[default]
    People,
    /// None does: what they wrote keeps their name.
    Names,
}

/// What the import is told besides what Jira holds.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The address of the Jira site, for the links in what is imported.
    pub site: String,
    pub users: Users,
    /// Jira users (by account id, email or name, in any case) who are people of Moebius already: their handles.
    pub user_map: BTreeMap<String, String>,
}

/// How many records of one kind an import made, changed and left as they were.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
}

/// How many links were made, taken away (the link is gone from Jira), left as they were, and left out because an end
/// of it was not imported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkCounts {
    pub added: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub skipped: usize,
}

/// What an import did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub issues: Counts,
    pub comments: Counts,
    pub kinds: Counts,
    pub people: Counts,
    pub cycles: Counts,
    pub milestones: Counts,
    pub links: LinkCounts,
    /// What could not be used or done, one line each; the import goes on past each.
    pub warnings: Vec<String>,
}

/// The projects an import is running into: a second import into one would make every card twice.
static RUNNING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

struct Running(String);

impl Running {
    fn start(app: &App, project: i64) -> Result<Self> {
        let project = format!("{}#{project}", app.data_dir.display());
        let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        if running.contains(&project) {
            return Err(AppError::Conflict(
                "an import into this project is running".into(),
            ));
        }
        running.push(project.clone());
        Ok(Self(project))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        running.retain(|p| *p != self.0);
    }
}

/// The state of one run of the import.
struct Import<'a> {
    app: &'a App,
    who: &'a Actor,
    project_id: i64,
    /// The repository the cards are made in: the project's own files.
    repository: Repository,
    options: &'a Options,
    report: Report,
    /// The author of what a Jira user wrote and the person they are, by Jira id.
    users: BTreeMap<String, people::Who>,
}

impl Import<'_> {
    fn warn(&mut self, line: String) {
        self.report.warnings.push(line);
    }
}

impl App {
    /// Imports what `fetched` holds of a Jira project into the project `project_id`, as `who` (an owner: it may make
    /// people). Running it again with what Jira holds now changes what changed there. A record the import cannot make is a
    /// warning in the report, not an error; errors are for what stops it before it starts.
    pub fn import_jira(
        &self,
        who: &Actor,
        project_id: i64,
        options: &Options,
        fetched: &Fetched,
    ) -> Result<Report> {
        let project = self.project(project_id)?;
        if !self.may_see_project(who, project.id) {
            return Err(AppError::NotFound(format!("project {project_id}")));
        }
        for (user, handle) in &options.user_map {
            if self
                .store
                .person_by_handle(handle.trim_start_matches('@'))?
                .is_none()
            {
                return Err(AppError::Invalid(format!(
                    "{user} is mapped to @{handle}, who is nobody"
                )));
            }
        }
        let _one = Running::start(self, project_id)?;
        let read = read::read(&fetched.fields, &fetched.issues);
        let repository = self.project_files(project_id)?;
        let mut run = Import {
            app: self,
            who,
            project_id,
            repository,
            options,
            report: Report::default(),
            users: BTreeMap::new(),
        };
        let kinds = run.import_kinds(&read)?;
        let milestones = run.import_epics(&read)?;
        let cycles = run.import_sprints(&read)?;
        let (cards, touched) = run.import_cards(&read, &kinds)?;
        run.import_relations(&read, &cards, &milestones, &cycles, &touched)?;
        run.close_sprints(&read, &cycles)?;
        Ok(run.report)
    }
}

/// `text` as a name of at most `limit` characters: control characters become spaces, the ends are trimmed, and a name
/// that is too long is cut with an ellipsis.
pub(super) fn name_of(text: &str, limit: usize) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.chars().count() <= limit {
        return cleaned.to_string();
    }
    let cut: String = cleaned.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}
