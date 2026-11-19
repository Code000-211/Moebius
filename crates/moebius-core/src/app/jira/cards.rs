//! The cards of the import: each Jira issue that is no epic becomes a card of the project's own files, with its kind and
//! fields, labels, priority, due date, estimate, assignee and closed state; then, once every card exists, its parent,
//! milestone, cycle, comments and the links between cards. A card is written only when what Jira shows of it differs
//! from the note kept of the last import.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Value, json};

use super::kinds::Kinds;
use super::read::{Custom, Issue, Link, Read};
use super::{Import, SOURCE};
use crate::app::{AppError, Result, columns};
use crate::blobs::redact;
use crate::bus::Event;
use crate::store::{ESTIMATE_LIMIT, FieldType};

/// The cards the import has, by the key of their issue: the card's id and its number in the project's files.
pub(super) type Cards = BTreeMap<String, (i64, i64)>;

/// The keys of the issues whose card was made or written by this import.
pub(super) type Touched = BTreeSet<String>;

/// What a card is to look like after the import: the fields the note is made of.
#[derive(Serialize)]
struct Mapped {
    title: String,
    body: String,
    author: String,
    labels: Vec<String>,
    priority: &'static str,
    due: Option<String>,
    estimate: Option<u32>,
    kind: Option<String>,
    fields: BTreeMap<String, Value>,
    /// The handle of the assignee, when Jira's is a person here.
    assignee: Option<String>,
    done: bool,
    /// What the card refers to, so that a change of it is a change of the card.
    parent: Option<String>,
    epic: Option<String>,
    sprint: Option<i64>,
}

/// The priority of Moebius that Jira's name for one comes closest to.
fn priority(name: Option<&str>) -> &'static str {
    match name.map(str::to_ascii_lowercase).as_deref() {
        Some("highest" | "blocker" | "critical" | "urgent") => "urgent",
        Some("high" | "major") => "high",
        Some("medium" | "normal") => "medium",
        Some("low" | "lowest" | "minor" | "trivial") => "low",
        _ => "none",
    }
}

impl Import<'_> {
    /// Makes or changes the card of each issue; the cards by the issue's key.
    pub(super) fn import_cards(&mut self, read: &Read, kinds: &Kinds) -> Result<(Cards, Touched)> {
        let app = self.app;
        let mut cards = Cards::new();
        let mut touched = Touched::new();
        for issue in read.issues.iter().filter(|i| !i.is_epic) {
            let mapped = self.map(issue, kinds)?;
            let seen = serde_json::to_string(&mapped).unwrap_or_default();
            let before = app
                .store
                .imported(self.project_id, SOURCE, "issue", &issue.key)?;
            let card = match &before {
                Some(b) => app.store.card(b.local_id)?,
                None => None,
            };
            let created = card.is_none();
            if created && before.is_some() {
                // The card was deleted here: what was kept of its comments and links is of no use to the new one.
                self.forget_around(&issue.key)?;
            }
            let card = match card {
                Some(card) => {
                    if before.is_some_and(|b| b.note == seen) {
                        self.report.issues.unchanged += 1;
                        cards.insert(issue.key.clone(), (card.id, card.number));
                        continue;
                    }
                    self.report.issues.updated += 1;
                    card
                }
                None => {
                    let card = app.store.add_card(
                        self.repository.id,
                        &mapped.title,
                        &mapped.body,
                        &mapped.author,
                        columns::BACKLOG,
                    )?;
                    if let Some(at) = issue.created {
                        app.store.set_card_created_at(card.id, at)?;
                    }
                    // Remembered at once and without a note: a step that fails is made again by the next import.
                    app.store.set_imported(
                        self.project_id,
                        SOURCE,
                        "issue",
                        &issue.key,
                        card.id,
                        "",
                    )?;
                    self.report.issues.created += 1;
                    card
                }
            };
            touched.insert(issue.key.clone());
            let warned = self.report.warnings.len();
            self.apply(card.id, issue, &mapped);
            if self.report.warnings.len() == warned {
                app.store.set_imported(
                    self.project_id,
                    SOURCE,
                    "issue",
                    &issue.key,
                    card.id,
                    &seen,
                )?;
            }
            if created {
                app.bus.publish(Event::CardAdded(app.card(card.id)?));
            }
            let card = app.card(card.id)?;
            cards.insert(issue.key.clone(), (card.id, card.number));
        }
        Ok((cards, touched))
    }

    /// Forgets the notes of the comments of the issue `key` and of the links that end in it.
    fn forget_around(&mut self, key: &str) -> Result<()> {
        let store = &self.app.store;
        for kind in ["comment", "link"] {
            for (id, _) in store.imported_of_kind(self.project_id, SOURCE, kind)? {
                let named = if kind == "comment" {
                    id.starts_with(&format!("{key}/"))
                } else {
                    id.split_once(':')
                        .is_some_and(|(_, ends)| ends.split('>').any(|k| k == key))
                };
                if named {
                    store.forget_imported(self.project_id, SOURCE, kind, &id)?;
                }
            }
        }
        Ok(())
    }

    /// What the card of `issue` is to be.
    fn map(&mut self, issue: &Issue, kinds: &Kinds) -> Result<Mapped> {
        let author = match &issue.reporter {
            Some(user) => self.who_is(user)?.author,
            None => "Jira".to_string(),
        };
        let assignee = match &issue.assignee {
            Some(user) => self.who_is(user)?.person.map(|(_, handle)| handle),
            None => None,
        };
        let body = redact(
            format!(
                "{}\n\n---\nImported from Jira: {}/browse/{} ({}, {})",
                issue.description,
                self.options.site,
                issue.key,
                issue.issue_type,
                if issue.status.is_empty() {
                    "no status"
                } else {
                    &issue.status
                }
            )
            .trim(),
        );
        let kind = kinds.of_type.get(&issue.issue_type).cloned();
        let fields = match &kind {
            Some(kind) => self.fields_of(issue, kind, kinds)?,
            None => BTreeMap::new(),
        };
        let labels = issue
            .labels
            .iter()
            .filter(|l| {
                let l = l.to_ascii_lowercase();
                !["priority:", "due:", "estimate:"]
                    .iter()
                    .any(|p| l.starts_with(p))
            })
            .cloned()
            .collect();
        Ok(Mapped {
            title: redact(&issue.summary),
            body,
            author,
            labels,
            priority: priority(issue.priority.as_deref()),
            due: issue.due.clone(),
            estimate: issue
                .points
                .map(|p| p.round() as u32)
                .filter(|p| (1..=ESTIMATE_LIMIT).contains(p)),
            kind,
            fields,
            assignee,
            done: issue.done,
            parent: issue.parent.clone(),
            epic: issue.epic.clone(),
            sprint: issue
                .sprints
                .iter()
                .max_by_key(|s| (s.state == "active", s.id))
                .map(|s| s.id),
        })
    }

    /// The values of the custom fields of `issue` that the kind `kind` has a field for, as the card holds them.
    fn fields_of(
        &mut self,
        issue: &Issue,
        kind: &str,
        kinds: &Kinds,
    ) -> Result<BTreeMap<String, Value>> {
        let kind = self.app.kind_named(kind)?;
        let mut fields = BTreeMap::new();
        for (jira, value) in &issue.custom {
            let Some(name) = kinds.field_names.get(jira) else {
                continue;
            };
            let Some(field) = kind
                .fields
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(name))
            else {
                continue;
            };
            let stored = match (field.field_type, value) {
                (FieldType::Text, Custom::Text(t)) => Some(json!(redact(t))),
                (FieldType::Number, Custom::Number(n)) => Some(json!(n)),
                (FieldType::Date, Custom::Date(d)) => Some(json!(d)),
                (FieldType::Choice, Custom::Choice(c)) => field
                    .options
                    .iter()
                    .find(|o| {
                        o.eq_ignore_ascii_case(&super::name_of(c, crate::app::kinds::NAME_LIMIT))
                    })
                    .map(|o| json!(o)),
                _ => None,
            };
            if let Some(stored) = stored {
                fields.insert(field.name.clone(), stored);
            }
        }
        Ok(fields)
    }

    /// Writes what differs onto the card; a step that fails is a warning, and the rest still goes on.
    fn apply(&mut self, card_id: i64, issue: &Issue, mapped: &Mapped) {
        let app = self.app;
        let who = self.who;
        let note = "the import from Jira";
        let mut failed: Vec<(&str, AppError)> = Vec::new();
        let mut run = |what: &'static str, result: Result<()>| {
            if let Err(e) = result {
                failed.push((what, e));
            }
        };
        run(
            "title and description",
            app.edit_card(who, card_id, &mapped.title, &mapped.body, note)
                .map(|_| ()),
        );
        run(
            "kind",
            app.set_card_kind(card_id, mapped.kind.as_deref())
                .map(|_| ()),
        );
        if mapped.kind.is_some() {
            let present = app.card(card_id).map(|c| c.fields).unwrap_or_default();
            let mut changes: BTreeMap<String, Value> =
                present.keys().map(|k| (k.clone(), Value::Null)).collect();
            changes.extend(mapped.fields.clone());
            run("fields", app.set_card_fields(card_id, &changes).map(|_| ()));
        }
        run(
            "labels",
            app.set_labels(card_id, &mapped.labels).map(|_| ()),
        );
        run(
            "priority",
            app.set_priority(card_id, mapped.priority).map(|_| ()),
        );
        run(
            "due date",
            app.set_due(card_id, mapped.due.as_deref()).map(|_| ()),
        );
        run(
            "estimate",
            app.set_estimate(card_id, mapped.estimate).map(|_| ()),
        );
        run("assignee", self.assign(card_id, issue, mapped));
        run("state", self.state(card_id, mapped.done));
        for (what, e) in failed {
            self.warn(format!("{}: the {what}: {e}", issue.key));
        }
    }

    /// Gives the card to the person Jira's assignee is, or to no one when Jira has none. An assignee who stays a name
    /// leaves the card as it is. The person may be disabled, which the app's own way of assigning refuses.
    fn assign(&mut self, card_id: i64, issue: &Issue, mapped: &Mapped) -> Result<()> {
        let app = self.app;
        let card = app.card(card_id)?;
        let want: Vec<String> = match (&issue.assignee, &mapped.assignee) {
            (None, _) => Vec::new(),
            (Some(_), Some(handle)) => vec![handle.clone()],
            (Some(_), None) => return Ok(()),
        };
        if card.assignees == want {
            return Ok(());
        }
        let people: BTreeSet<i64> = want
            .iter()
            .filter_map(|h| app.store.person_by_handle(h).ok().flatten())
            .map(|p| p.id)
            .collect();
        app.store.set_card_assignees(card_id, &people)?;
        app.bus.publish(Event::CardAssigneesChanged {
            card: card_id,
            assignees: want,
        });
        Ok(())
    }

    /// Closes the card when Jira's issue is done and reopens it when Jira reopened it.
    fn state(&mut self, card_id: i64, done: bool) -> Result<()> {
        let card = self.app.card(card_id)?;
        if done && !card.closed {
            self.app.close_issue(self.who, card_id, "Done in Jira")?;
        } else if !done
            && card.closed
            // A duplicate is closed by Moebius, whatever its status in Jira.
            && self.app.store.links_from(card_id, crate::store::DUPLICATE)?.is_empty()
        {
            self.app
                .reopen_issue(self.who, card_id, "Open again in Jira")?;
        }
        Ok(())
    }
}

/// What the second pass needs of an issue's links, once each link has both its ends.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Edge {
    kind: &'static str,
    /// The blocker, the duplicate, or the lesser key of two related issues.
    from: String,
    /// The issue blocked, the original, or the greater key of two related issues.
    to: String,
}

impl Edge {
    fn id(&self) -> String {
        format!("{}:{}>{}", self.kind, self.from, self.to)
    }
}

fn edge_of(issue: &str, link: &Link) -> Edge {
    let (kind, from, to) = match link {
        Link::Blocks(o) => ("blocks", issue, o.as_str()),
        Link::BlockedBy(o) => ("blocks", o.as_str(), issue),
        Link::DuplicateOf(o) => ("duplicate", issue, o.as_str()),
        Link::DuplicatedBy(o) => ("duplicate", o.as_str(), issue),
        Link::Related(o) => {
            let (a, b) = if issue <= o.as_str() {
                (issue, o.as_str())
            } else {
                (o.as_str(), issue)
            };
            ("related", a, b)
        }
    };
    Edge {
        kind,
        from: from.to_string(),
        to: to.to_string(),
    }
}

impl Import<'_> {
    /// The second pass: what refers from a card to another record, once they all exist.
    pub(super) fn import_relations(
        &mut self,
        read: &Read,
        cards: &Cards,
        milestones: &BTreeMap<String, i64>,
        cycles: &BTreeMap<i64, i64>,
        touched: &Touched,
    ) -> Result<()> {
        let numbers: BTreeSet<i64> = cards.values().map(|(_, n)| *n).collect();
        for issue in read.issues.iter().filter(|i| !i.is_epic) {
            let Some(&(card_id, _)) = cards.get(&issue.key) else {
                continue;
            };
            // What a person moved on a card Jira did not change stays where they put it.
            let mut results = vec![("comments", self.comments(card_id, issue))];
            if touched.contains(&issue.key) {
                results.push(("parent", self.parent(card_id, issue, cards, &numbers)));
                results.push(("epic", self.epic(card_id, issue, milestones)));
                results.push(("sprint", self.sprint(card_id, issue, cycles)));
            }
            for (what, result) in results {
                if let Err(e) = result {
                    self.warn(format!("{}: the {what}: {e}", issue.key));
                }
            }
        }
        self.links(read, cards)
    }

    fn parent(
        &mut self,
        card_id: i64,
        issue: &Issue,
        cards: &Cards,
        numbers: &BTreeSet<i64>,
    ) -> Result<()> {
        let want = match &issue.parent {
            Some(key) => match cards.get(key) {
                Some((_, number)) => Some(*number),
                None => {
                    self.warn(format!("{}: its parent {key} is not imported", issue.key));
                    return Ok(());
                }
            },
            None => None,
        };
        let card = self.app.card(card_id)?;
        // A parent a person gave, which is not an imported card, stays.
        if card.parent != want
            && (want.is_some() || card.parent.is_some_and(|p| numbers.contains(&p)))
        {
            self.app.set_parent(card_id, want)?;
        }
        Ok(())
    }

    fn epic(
        &mut self,
        card_id: i64,
        issue: &Issue,
        milestones: &BTreeMap<String, i64>,
    ) -> Result<()> {
        let imported: BTreeSet<i64> = milestones.values().copied().collect();
        let want = match &issue.epic {
            Some(key) => match milestones.get(key) {
                Some(id) => Some(*id),
                None => {
                    self.warn(format!("{}: its epic {key} is not imported", issue.key));
                    return Ok(());
                }
            },
            None => None,
        };
        let current: Vec<i64> = self
            .app
            .store
            .card_milestones(card_id)?
            .iter()
            .map(|m| m.id)
            .collect();
        let mut next: Vec<i64> = current
            .iter()
            .copied()
            .filter(|m| !imported.contains(m))
            .collect();
        next.extend(want);
        let (mut a, mut b) = (current.clone(), next.clone());
        a.sort_unstable();
        b.sort_unstable();
        if a != b {
            self.app.set_card_milestones(self.who, card_id, &next)?;
        }
        Ok(())
    }

    /// Puts the card into the cycle of its latest sprint (the active one, else the newest) while that is open.
    fn sprint(&mut self, card_id: i64, issue: &Issue, cycles: &BTreeMap<i64, i64>) -> Result<()> {
        let latest = issue
            .sprints
            .iter()
            .filter(|s| cycles.contains_key(&s.id))
            .max_by_key(|s| (s.state == "active", s.id));
        let Some(cycle) = latest.and_then(|s| cycles.get(&s.id)) else {
            return Ok(());
        };
        if self
            .app
            .store
            .cycle(*cycle)?
            .is_none_or(|c| c.closed_at.is_some())
        {
            return Ok(());
        }
        if self.app.store.card_cycle(card_id)?.map(|c| c.id) != Some(*cycle) {
            self.app.set_card_cycle(self.who, card_id, Some(*cycle))?;
        }
        Ok(())
    }

    fn comments(&mut self, card_id: i64, issue: &Issue) -> Result<()> {
        let app = self.app;
        for comment in &issue.comments {
            let author = match &comment.author {
                Some(user) => self.who_is(user)?.author,
                None => "Jira".to_string(),
            };
            let body = redact(&comment.body);
            let seen = serde_json::to_string(&[&author, &body]).unwrap_or_default();
            let id = format!("{}/{}", issue.key, comment.id);
            match app
                .store
                .imported(self.project_id, SOURCE, "comment", &id)?
            {
                Some(b) if b.note == seen => self.report.comments.unchanged += 1,
                Some(b) => {
                    app.store.set_comment_body(b.local_id, &body)?;
                    app.store.set_imported(
                        self.project_id,
                        SOURCE,
                        "comment",
                        &id,
                        b.local_id,
                        &seen,
                    )?;
                    self.report.comments.updated += 1;
                }
                None => {
                    let Some(made) = app
                        .store
                        .add_comment(card_id, &author, &body, comment.at, None)?
                    else {
                        continue;
                    };
                    app.store.set_imported(
                        self.project_id,
                        SOURCE,
                        "comment",
                        &id,
                        made.id,
                        &seen,
                    )?;
                    app.bus.publish(Event::CardCommented {
                        card: card_id,
                        comment: made,
                    });
                    self.report.comments.created += 1;
                }
            }
        }
        Ok(())
    }

    /// Makes the blockers, duplicates and related cards the issues' links say, and takes away those an earlier import
    /// made that Jira no longer has.
    fn links(&mut self, read: &Read, cards: &Cards) -> Result<()> {
        let app = self.app;
        let mut wanted: BTreeMap<String, Edge> = BTreeMap::new();
        let mut skipped = BTreeSet::new();
        for issue in read.issues.iter().filter(|i| !i.is_epic) {
            for link in &issue.links {
                let edge = edge_of(&issue.key, link);
                if cards.contains_key(&edge.from) && cards.contains_key(&edge.to) {
                    wanted.insert(edge.id(), edge);
                } else {
                    skipped.insert(edge.id());
                }
            }
        }
        self.report.links.skipped = skipped.len();
        let made: BTreeMap<String, _> = app
            .store
            .imported_of_kind(self.project_id, SOURCE, "link")?
            .into_iter()
            .collect();
        for (id, edge) in &wanted {
            if made.contains_key(id) {
                self.report.links.unchanged += 1;
                continue;
            }
            let (from, to) = (cards[&edge.from], cards[&edge.to]);
            let result = match edge.kind {
                "blocks" => app.add_blocker(to.0, from.1).map(|_| ()),
                "duplicate" => app.mark_duplicate(self.who, from.0, to.1).map(|_| ()),
                _ => app.add_related(from.0, to.1).map(|_| ()),
            };
            match result {
                Ok(()) => {
                    app.store
                        .set_imported(self.project_id, SOURCE, "link", id, 0, "")?;
                    self.report.links.added += 1;
                }
                Err(e) => self.warn(format!("the link {id}: {e}")),
            }
        }
        for id in made.keys().filter(|id| !wanted.contains_key(*id)) {
            let Some((kind, ends)) = id.split_once(':') else {
                continue;
            };
            let Some((from, to)) = ends.split_once('>') else {
                continue;
            };
            if let (Some(from), Some(to)) = (cards.get(from), cards.get(to)) {
                let result = match kind {
                    "blocks" => app.remove_blocker(to.0, from.1).map(|_| ()),
                    "duplicate" => app.unmark_duplicate(from.0).map(|_| ()),
                    _ => app.remove_related(from.0, to.1).map(|_| ()),
                };
                if let Err(e) = result {
                    self.warn(format!("the link {id} is gone from Jira but stays: {e}"));
                }
            }
            app.store
                .forget_imported(self.project_id, SOURCE, "link", id)?;
            self.report.links.removed += 1;
        }
        Ok(())
    }
}
