//! Backlog sync (ADR 0031): a project keeps a backlog as a file in one of its repositories, and a sync makes the issues
//! of its items. The sync is a tool, used by a person, the Moebius agent (`"$MOEBIUS" backlog sync`) or a scheduled
//! workflow, and it is the same for any project that names a file and a pattern for its items.
//!
//! An item is a line that matches the project's pattern (by default a heading that starts with an ID such as `K-47`)
//! and the lines after it until the next heading of the same level or a higher one. The issue of an item is titled
//! `<ID>: <title>`, quotes the item in full, and is found again by the ID in its title, so a sync that runs twice
//! makes nothing twice; when the item's text changed, the issue's body follows it. An item that is marked closed (the
//! project's marker anywhere in it, or a file `closed/<ID>.md` next to the backlog file) has its issue closed as not
//! planned. At most `cap` issues are made by one sync, so that the set of issues ready for agents stays small. An item
//! that states a difficulty (`easy`, `medium` or `hard`) or says a lane is required gives its issue the matching labels.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{App, AppError, Result};
use crate::decide::Actor;
use crate::repos::files;
use crate::store::{
    Backlog, DEFAULT_BACKLOG_CAP, DEFAULT_BACKLOG_PATTERN, DEFAULT_CLOSED_MARKER,
    DEFAULT_DIFFICULTY_LABEL, DEFAULT_LANE_LABEL,
};

/// The most issues one sync may be allowed to make.
pub const MOST_CAP: i64 = 100;

/// What a person, or the agent, sets of a project's backlog; what is none stays as it is, or is the default for a
/// backlog that is new.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BacklogEdit {
    /// The name of the repository that holds the file; it must be one of the project's.
    pub repository: Option<String>,
    pub path: Option<String>,
    pub pattern: Option<String>,
    pub closed_marker: Option<String>,
    pub cap: Option<i64>,
    pub difficulty_label: Option<String>,
    pub lane_label: Option<String>,
}

/// A project's backlog as the API tells it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BacklogView {
    pub project_id: i64,
    pub repository: String,
    pub path: String,
    pub pattern: String,
    pub closed_marker: String,
    pub cap: i64,
    pub difficulty_label: String,
    pub lane_label: String,
}

/// One item of a backlog file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub title: String,
    /// The item as written: its heading line and the lines after it.
    pub text: String,
    /// `easy`, `medium` or `hard` when the item says so.
    pub difficulty: Option<String>,
    pub lane_required: bool,
    pub closed: bool,
}

/// An issue a sync made, changed or closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Synced {
    pub id: String,
    pub number: i64,
    pub title: String,
}

/// What a sync did.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub project: String,
    pub repository: String,
    pub path: String,
    /// Whether nothing was written: the report says what a sync would do.
    pub dry_run: bool,
    /// How many items the file holds.
    pub items: usize,
    pub created: Vec<Synced>,
    /// Issues whose title or body followed their item.
    pub updated: Vec<Synced>,
    /// Issues given labels their item asks for.
    pub labelled: Vec<Synced>,
    /// Issues closed as not planned because their item is closed.
    pub closed: Vec<Synced>,
    /// The IDs of items that have no issue yet and wait for the next sync because the cap was reached.
    pub deferred: Vec<String>,
    /// How many items had an issue that was as it should be.
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

fn level_of(line: &str) -> usize {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if line[hashes..].starts_with(char::is_whitespace) {
        hashes
    } else {
        0
    }
}

fn normal(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}

static DIFFICULTY: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?i)\bdifficulty\b[\s:=*_\-]*\b(easy|medium|hard)\b")
        .expect("the pattern is valid")
});
static LANE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?i)\blane[ -]required\b(\s*[:=]\s*\**\s*(no|false|none)\b)?")
        .expect("the pattern is valid")
});

/// The items of `text` that `pattern` starts. `marker` says an item is closed; `closed` holds the IDs closed by a file.
pub fn parse_items(
    text: &str,
    pattern: &Regex,
    marker: &str,
    closed: &BTreeSet<String>,
) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    // A fenced block's lines start and end nothing.
    let mut fenced = vec![false; lines.len()];
    let mut inside = false;
    for (at, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            inside = !inside;
            fenced[at] = true;
        } else {
            fenced[at] = inside;
        }
    }
    let starts: Vec<(usize, String, String)> = lines
        .iter()
        .enumerate()
        .filter(|(at, _)| !fenced[*at])
        .filter_map(|(at, line)| {
            let c = pattern.captures(line)?;
            Some((
                at,
                c.name("id")?.as_str().trim().to_string(),
                c.name("title")
                    .map_or("", |m| m.as_str())
                    .trim()
                    .to_string(),
            ))
        })
        .collect();
    let mut items = Vec::new();
    for (n, (at, id, title)) in starts.iter().enumerate() {
        let level = level_of(lines[*at]);
        let next_start = starts.get(n + 1).map(|(next, _, _)| *next);
        let mut end = lines.len();
        for (i, line) in lines.iter().enumerate().skip(at + 1) {
            let ends = next_start == Some(i)
                || (!fenced[i] && level > 0 && (1..=level).contains(&level_of(line)));
            if ends {
                end = i;
                break;
            }
        }
        let body = lines[*at..end].join("\n");
        let body = body.trim_end().to_string();
        let lower = body.to_lowercase();
        let difficulty = DIFFICULTY.captures(&body).map(|c| c[1].to_lowercase());
        let lane_required = LANE.captures(&body).is_some_and(|c| c.get(1).is_none());
        let marked = !marker.trim().is_empty() && lower.contains(&marker.trim().to_lowercase());
        items.push(Item {
            id: id.clone(),
            title: if title.is_empty() {
                id.clone()
            } else {
                title.clone()
            },
            text: body,
            difficulty,
            lane_required,
            closed: marked || closed.contains(id),
        });
    }
    items
}

/// The title of the issue of `item`.
pub fn issue_title(item: &Item) -> String {
    format!("{}: {}", item.id, item.title)
}

/// The body of the issue of `item` kept in `path` of `repository`: the item quoted in full, then where it comes from.
pub fn issue_body(item: &Item, repository: &str, path: &str) -> String {
    let quoted: Vec<String> = item
        .text
        .lines()
        .map(|l| {
            if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect();
    format!(
        "{}\n\n---\nKept in step with `{path}` in {repository} (item {}) by the backlog sync. Edit the item there; \
         this text is replaced when it changes.",
        quoted.join("\n"),
        item.id
    )
}

impl Backlog {
    fn regex(&self) -> Result<Regex> {
        Regex::new(&self.pattern).map_err(|e| {
            AppError::Invalid(format!("the item pattern is not a regular expression: {e}"))
        })
    }

    /// The labels an item's issue should carry.
    fn labels_of(&self, item: &Item) -> Vec<String> {
        let mut labels = Vec::new();
        if let (Some(level), false) = (&item.difficulty, self.difficulty_label.trim().is_empty()) {
            labels.push(format!("{}{level}", self.difficulty_label.trim()));
        }
        if item.lane_required && !self.lane_label.trim().is_empty() {
            labels.push(self.lane_label.trim().to_string());
        }
        labels
    }
}

fn label_name(name: &str, what: &str) -> Result<String> {
    let name = name.trim();
    if name.len() > 50 || name.contains([',', '\n']) {
        return Err(AppError::Invalid(format!("{name:?} is not a {what}")));
    }
    Ok(name.to_string())
}

impl App {
    /// The backlog of the project as it is kept, none when the project has none.
    pub fn backlog_of(&self, project_id: i64) -> Result<Option<BacklogView>> {
        self.project(project_id)?;
        self.store
            .backlog(project_id)?
            .map(|b| self.backlog_view(&b))
            .transpose()
    }

    fn backlog_view(&self, b: &Backlog) -> Result<BacklogView> {
        Ok(BacklogView {
            project_id: b.project_id,
            repository: self.repository(b.repository_id)?.name,
            path: b.path.clone(),
            pattern: b.pattern.clone(),
            closed_marker: b.closed_marker.clone(),
            cap: b.cap,
            difficulty_label: b.difficulty_label.clone(),
            lane_label: b.lane_label.clone(),
        })
    }

    /// A person, or the Moebius agent, says where the project's backlog is and how its items are read. A backlog that
    /// is new needs the repository and the path; the rest has defaults, and what is left out of an edit stays. Recorded
    /// as an `edit_backlog` decision.
    pub fn edit_backlog(
        &self,
        by: &Actor,
        project_id: i64,
        edit: &BacklogEdit,
        reason: &str,
    ) -> Result<BacklogView> {
        let project = self.project(project_id)?;
        let before = self.store.backlog(project_id)?;
        let repository_id = match (&edit.repository, &before) {
            (Some(name), _) => {
                let found = self
                    .store
                    .repositories()?
                    .into_iter()
                    .find(|r| r.name == *name && project.repositories.contains(&r.id));
                found
                    .ok_or_else(|| {
                        AppError::Invalid(format!(
                            "{name} is no repository of the project {}",
                            project.name
                        ))
                    })?
                    .id
            }
            (None, Some(b)) => b.repository_id,
            (None, None) => {
                return Err(AppError::Invalid(
                    "a backlog needs the repository that holds its file".into(),
                ));
            }
        };
        let path = match (&edit.path, &before) {
            (Some(path), _) => files::inside(path)?,
            (None, Some(b)) => b.path.clone(),
            (None, None) => {
                return Err(AppError::Invalid(
                    "a backlog needs the path of its file".into(),
                ));
            }
        };
        if path.is_empty() {
            return Err(AppError::Invalid("the backlog's path names no file".into()));
        }
        let pick = |new: &Option<String>, old: Option<&String>, default: &str| {
            new.clone()
                .or_else(|| old.cloned())
                .unwrap_or_else(|| default.to_string())
        };
        let backlog = Backlog {
            project_id,
            repository_id,
            path,
            pattern: pick(
                &edit.pattern,
                before.as_ref().map(|b| &b.pattern),
                DEFAULT_BACKLOG_PATTERN,
            ),
            closed_marker: pick(
                &edit.closed_marker,
                before.as_ref().map(|b| &b.closed_marker),
                DEFAULT_CLOSED_MARKER,
            )
            .trim()
            .to_string(),
            cap: edit
                .cap
                .or(before.as_ref().map(|b| b.cap))
                .unwrap_or(DEFAULT_BACKLOG_CAP),
            difficulty_label: label_name(
                &pick(
                    &edit.difficulty_label,
                    before.as_ref().map(|b| &b.difficulty_label),
                    DEFAULT_DIFFICULTY_LABEL,
                ),
                "label name",
            )?,
            lane_label: label_name(
                &pick(
                    &edit.lane_label,
                    before.as_ref().map(|b| &b.lane_label),
                    DEFAULT_LANE_LABEL,
                ),
                "label name",
            )?,
        };
        if !(1..=MOST_CAP).contains(&backlog.cap) {
            return Err(AppError::Invalid(format!(
                "the cap is from 1 to {MOST_CAP} issues a sync"
            )));
        }
        let regex = backlog.regex()?;
        let names: Vec<&str> = regex.capture_names().flatten().collect();
        if !names.contains(&"id") {
            return Err(AppError::Invalid(
                "the item pattern needs a group named id, as (?P<id>...); a group named title is the title".into(),
            ));
        }
        self.store.set_backlog(&backlog)?;
        let context = BTreeMap::from([
            ("project".into(), project.name.clone()),
            ("path".into(), backlog.path.clone()),
        ]);
        self.record_action(by, "edit_backlog", None, None, context, reason)?;
        self.backlog_view(&backlog)
    }

    /// Takes the project's backlog away; its issues stay.
    pub fn remove_backlog(&self, by: &Actor, project_id: i64, reason: &str) -> Result<()> {
        let project = self.project(project_id)?;
        if self.store.backlog(project_id)?.is_none() {
            return Err(AppError::NotFound(format!(
                "a backlog of the project {}",
                project.name
            )));
        }
        self.store.remove_backlog(project_id)?;
        let context = BTreeMap::from([("project".into(), project.name)]);
        self.record_action(by, "remove_backlog", None, None, context, reason)
    }

    /// Reads the project's backlog file from its repository's default branch and makes the issues its items ask for: an
    /// issue for each item that has none (at most the project's cap), the title, body and labels of an issue whose item
    /// changed, and the closing as not planned of an issue whose item is closed. A sync that has nothing to do changes
    /// nothing, and with `dry_run` nothing is written either way: the report says what would be. A step that fails on
    /// one item is a warning, and the others go on.
    pub fn sync_backlog(&self, by: &Actor, project_id: i64, dry_run: bool) -> Result<SyncReport> {
        let project = self.project(project_id)?;
        let backlog = self.store.backlog(project_id)?.ok_or_else(|| {
            AppError::NotFound(format!("a backlog of the project {}", project.name))
        })?;
        let regex = backlog.regex()?;
        let repository = self.repository(backlog.repository_id)?;
        let mut report = SyncReport {
            project: project.name.clone(),
            repository: repository.name.clone(),
            path: backlog.path.clone(),
            dry_run,
            ..SyncReport::default()
        };
        // The clone is brought up to date so that the file read is the one on the forge's default branch.
        if let Err(e) = self
            .forge(&repository)
            .and_then(|f| Ok(f.refresh(&repository)?))
        {
            report
                .warnings
                .push(format!("the clone could not be brought up to date: {e}"));
        }
        let view = files::file(self, repository.id, &backlog.path)?;
        let text = view
            .text
            .ok_or_else(|| AppError::Invalid(format!("{} is not a text file", backlog.path)))?;
        let directory = backlog
            .path
            .rsplit_once('/')
            .map_or(String::new(), |(dir, _)| dir.to_string());
        let closed_dir = if directory.is_empty() {
            "closed".to_string()
        } else {
            format!("{directory}/closed")
        };
        // Closed by file: `closed/<ID>.md`. A repository that has no such directory has none closed.
        let closed: BTreeSet<String> = files::tree(self, repository.id, &closed_dir)
            .map(|tree| {
                tree.entries
                    .into_iter()
                    .filter(|e| e.kind == "file")
                    .filter_map(|e| e.name.strip_suffix(".md").map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let items = parse_items(&text, &regex, &backlog.closed_marker, &closed);
        report.items = items.len();
        let mut seen = BTreeSet::new();
        let mut made = 0;
        for item in items {
            if !seen.insert(item.id.clone()) {
                report.warnings.push(format!(
                    "{} stands twice in the file; the first is used",
                    item.id
                ));
                continue;
            }
            if let Err(e) = self.sync_item(
                by,
                &backlog,
                &repository.name,
                &item,
                dry_run,
                &mut made,
                &mut report,
            ) {
                report.warnings.push(format!("{}: {e}", item.id));
            }
        }
        if !dry_run {
            self.store.note_activity(
                "backlog_sync",
                Some(repository.id),
                None,
                None,
                &format!(
                    "{} created, {} updated, {} closed",
                    report.created.len(),
                    report.updated.len(),
                    report.closed.len()
                ),
                &json!({ "project": project.name, "path": backlog.path }),
            )?;
        }
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)]
    fn sync_item(
        &self,
        by: &Actor,
        backlog: &Backlog,
        repository: &str,
        item: &Item,
        dry_run: bool,
        made: &mut i64,
        report: &mut SyncReport,
    ) -> Result<()> {
        let title = issue_title(item);
        let body = issue_body(item, repository, &backlog.path);
        let labels = backlog.labels_of(item);
        let synced = |number: i64| Synced {
            id: item.id.clone(),
            number,
            title: title.clone(),
        };
        let existing = self
            .store
            .cards_of_backlog_item(backlog.repository_id, &item.id)?;
        let Some(card_id) = existing.first().copied() else {
            if item.closed {
                // Nothing to close and nothing worth making.
                report.unchanged += 1;
                return Ok(());
            }
            if *made >= backlog.cap {
                report.deferred.push(item.id.clone());
                return Ok(());
            }
            *made += 1;
            if dry_run {
                report.created.push(synced(0));
                return Ok(());
            }
            let (card, warnings) = self.create_tracked_issue(
                by,
                backlog.repository_id,
                &title,
                &body,
                None,
                &labels,
                &[],
            )?;
            report
                .warnings
                .extend(warnings.into_iter().map(|w| format!("{}: {w}", item.id)));
            report.created.push(synced(card.number));
            return Ok(());
        };
        let card = self.card(card_id)?;
        if item.closed {
            if card.closed {
                report.unchanged += 1;
            } else {
                if !dry_run {
                    self.close_card_not_planned(card.id, Some(by))?;
                }
                report.closed.push(synced(card.number));
            }
            return Ok(());
        }
        if card.closed {
            // Someone closed it while the item is still open: the issue stays as they left it.
            report.unchanged += 1;
            return Ok(());
        }
        let mut changed = false;
        if card.title != title || normal(&card.body) != normal(&body) {
            if !dry_run {
                self.edit_card(
                    by,
                    card.id,
                    &title,
                    &body,
                    "its item in the backlog changed",
                )?;
            }
            report.updated.push(synced(card.number));
            changed = true;
        }
        let missing: Vec<&String> = labels
            .iter()
            .filter(|l| !card.labels.iter().any(|have| have.eq_ignore_ascii_case(l)))
            .collect();
        if !missing.is_empty() {
            if !dry_run {
                let mut all = card.labels.clone();
                all.extend(missing.into_iter().cloned());
                self.set_labels(card.id, &all)?;
            }
            report.labelled.push(synced(card.number));
            changed = true;
        }
        if !changed {
            report.unchanged += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(text: &str) -> Vec<Item> {
        parse_items(
            text,
            &Regex::new(DEFAULT_BACKLOG_PATTERN).unwrap(),
            DEFAULT_CLOSED_MARKER,
            &BTreeSet::new(),
        )
    }

    const FILE: &str = "# Optimisation backlog\n\nIntro.\n\n## K-47: Cache the plan\nDifficulty: hard\n\nBody.\n\n### Detail heading\nmore\n\n## P-16 - Shorter prompts [closed]\nDone already.\n\n## M-01. Lane work\nLane required\nDifficulty: **Easy**\n\n```\n## X-1: not an item\n```\n";

    #[test]
    fn items_are_the_headings_with_an_id_and_what_follows_them() {
        let all = items(FILE);
        let ids: Vec<_> = all.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["K-47", "P-16", "M-01"]);
        assert_eq!(all[0].title, "Cache the plan");
        // A deeper heading belongs to the item; the next heading of its own level ends it.
        assert!(all[0].text.contains("Detail heading"));
        assert!(!all[0].text.contains("P-16"));
        // A heading inside a fenced block is no item.
        assert!(all[2].text.contains("X-1"));
    }

    #[test]
    fn a_difficulty_a_lane_and_a_closed_marker_are_read_from_the_item() {
        let all = items(FILE);
        assert_eq!(all[0].difficulty.as_deref(), Some("hard"));
        assert!(!all[0].lane_required && !all[0].closed);
        assert!(all[1].closed);
        assert_eq!(all[2].difficulty.as_deref(), Some("easy"));
        assert!(all[2].lane_required);
        let negated = items("## A-1: x\nLane required: no\n");
        assert!(!negated[0].lane_required);
    }

    #[test]
    fn a_file_under_closed_closes_its_item() {
        let closed = BTreeSet::from(["K-47".to_string()]);
        let all = parse_items(
            FILE,
            &Regex::new(DEFAULT_BACKLOG_PATTERN).unwrap(),
            "",
            &closed,
        );
        assert!(all[0].closed);
        assert!(!all[1].closed);
    }

    #[test]
    fn the_issue_quotes_the_item_in_full() {
        let item = &items(FILE)[0];
        let body = issue_body(item, "acme/widgets", "notes/backlog.md");
        assert!(body.starts_with("> ## K-47: Cache the plan\n> Difficulty: hard\n>\n> Body."));
        assert!(body.contains("`notes/backlog.md` in acme/widgets (item K-47)"));
        assert_eq!(issue_title(item), "K-47: Cache the plan");
    }
}
