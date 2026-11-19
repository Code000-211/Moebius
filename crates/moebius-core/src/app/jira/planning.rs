//! Jira's epics as milestones and its sprints as cycles. A milestone belongs to the organisation and has a target date,
//! which an epic without a due date borrows from its latest issue, else sets 90 days ahead. A cycle belongs to the
//! project and needs both dates, so a sprint without them is left out. A closed sprint closes its cycle once its cards
//! are in it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::{Date, Duration, format_description};

use super::read::{Read, Sprint};
use super::{Import, SOURCE, name_of};
use crate::app::cycles::NAME_LIMIT as CYCLE_NAME_LIMIT;
use crate::app::milestones::{DESCRIPTION_LIMIT, NAME_LIMIT as MILESTONE_NAME_LIMIT};
use crate::app::{AppError, Carry, CycleFields, MilestoneFields, Result};

/// The days ahead a milestone's target is set when its epic and its issues have no due date.
const DEFAULT_DAYS: i64 = 90;

/// What a milestone or cycle looked like when it was read: the import changes it only when this changes.
#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Note(Vec<String>);

fn note(parts: &[&str]) -> String {
    serde_json::to_string(&Note(parts.iter().map(|p| p.to_string()).collect())).unwrap_or_default()
}

fn plus_days(today: &str, days: i64) -> Option<String> {
    let format = format_description::parse_borrowed::<2>("[year]-[month]-[day]").ok()?;
    Date::parse(today, &format)
        .ok()?
        .checked_add(Duration::days(days))?
        .format(&format)
        .ok()
}

impl Import<'_> {
    /// Makes or changes the milestone of each epic; the ids by the epic's key.
    pub(super) fn import_epics(&mut self, read: &Read) -> Result<BTreeMap<String, i64>> {
        let app = self.app;
        let mut milestones = BTreeMap::new();
        for epic in read.issues.iter().filter(|i| i.is_epic) {
            let latest_due = read
                .issues
                .iter()
                .filter(|i| i.epic.as_deref() == Some(&epic.key))
                .filter_map(|i| i.due.clone())
                .max();
            let before = app
                .store
                .imported(self.project_id, SOURCE, "epic", &epic.key)?;
            let existing = match &before {
                Some(b) => app.milestone(b.local_id).ok(),
                None => None,
            };
            // Without a date in Jira the milestone keeps the one it has, so that the day of the import does not
            // change it again and again.
            let target = match (epic.due.clone().or(latest_due), &existing) {
                (Some(date), _) => date,
                (None, Some(m)) => m.target_date.clone(),
                (None, None) => plus_days(&app.store.today()?, DEFAULT_DAYS)
                    .ok_or_else(|| AppError::Invalid("today is not a date".into()))?,
            };
            let description: String = format!(
                "{}\n\nImported from Jira: {}/browse/{}",
                epic.description, self.options.site, epic.key
            )
            .trim()
            .chars()
            .take(DESCRIPTION_LIMIT)
            .collect();
            let name = name_of(&epic.summary, MILESTONE_NAME_LIMIT);
            let fields = MilestoneFields {
                name: name.clone(),
                target_date: target.clone(),
                description: description.clone(),
            };
            let seen = note(&[&name, &target, &description]);
            let before = app
                .store
                .imported(self.project_id, SOURCE, "epic", &epic.key)?;
            let existing = match &before {
                Some(b) => app.milestone(b.local_id).ok(),
                None => None,
            };
            let id = match (existing, before) {
                (Some(m), Some(b)) if b.note == seen => {
                    self.report.milestones.unchanged += 1;
                    m.id
                }
                (Some(m), _) => {
                    let result = app.edit_milestone(m.id, &fields).or_else(|e| match e {
                        AppError::Exists(_) => {
                            app.edit_milestone(m.id, &self.apart(&fields, &epic.key))
                        }
                        e => Err(e),
                    });
                    match result {
                        Ok(_) => self.report.milestones.updated += 1,
                        Err(e) => self.warn(format!("the milestone of {}: {e}", epic.key)),
                    }
                    m.id
                }
                (None, _) => {
                    let made = app.add_milestone(&fields).or_else(|e| match e {
                        AppError::Exists(_) => app.add_milestone(&self.apart(&fields, &epic.key)),
                        e => Err(e),
                    });
                    match made {
                        Ok(m) => {
                            self.report.milestones.created += 1;
                            m.id
                        }
                        Err(e) => {
                            self.warn(format!("the milestone of {}: {e}", epic.key));
                            continue;
                        }
                    }
                }
            };
            app.store
                .set_imported(self.project_id, SOURCE, "epic", &epic.key, id, &seen)?;
            milestones.insert(epic.key.clone(), id);
        }
        Ok(milestones)
    }

    /// The fields with the epic's key in the name, for a milestone whose name is taken (names are the organisation's).
    fn apart(&self, fields: &MilestoneFields, key: &str) -> MilestoneFields {
        MilestoneFields {
            name: name_of(&format!("{} ({key})", fields.name), MILESTONE_NAME_LIMIT),
            ..fields.clone()
        }
    }

    /// Makes or changes the cycle of each sprint with both its dates; the ids by the sprint's id.
    pub(super) fn import_sprints(&mut self, read: &Read) -> Result<BTreeMap<i64, i64>> {
        let app = self.app;
        let mut sprints: BTreeMap<i64, &Sprint> = BTreeMap::new();
        for sprint in read.issues.iter().flat_map(|i| &i.sprints) {
            sprints.insert(sprint.id, sprint);
        }
        let mut cycles = BTreeMap::new();
        for sprint in sprints.values() {
            let (Some(starts_on), Some(ends_on)) = (&sprint.starts_on, &sprint.ends_on) else {
                self.warn(format!(
                    "the sprint {} has no start or end: it is not a cycle, and its issues are in none",
                    sprint.name
                ));
                continue;
            };
            let name = name_of(&sprint.name, CYCLE_NAME_LIMIT);
            let fields = CycleFields {
                name: name.clone(),
                starts_on: starts_on.clone(),
                ends_on: ends_on.clone(),
            };
            let seen = note(&[&name, starts_on, ends_on]);
            let id = sprint.id.to_string();
            let before = app.store.imported(self.project_id, SOURCE, "sprint", &id)?;
            let existing = match &before {
                Some(b) => app.store.cycle(b.local_id)?,
                None => None,
            };
            let cycle = match (existing, before) {
                (Some(c), Some(b)) if b.note == seen || c.closed_at.is_some() => {
                    self.report.cycles.unchanged += 1;
                    c
                }
                (Some(c), _) => match app.edit_cycle(self.who, c.id, &fields) {
                    Ok(c) => {
                        self.report.cycles.updated += 1;
                        c
                    }
                    Err(e) => {
                        self.warn(format!("the cycle of {}: {e}", sprint.name));
                        c
                    }
                },
                (None, _) => {
                    let made =
                        app.add_cycle(self.who, self.project_id, &fields)
                            .or_else(|e| match e {
                                AppError::Exists(_) => {
                                    let apart = CycleFields {
                                        name: name_of(
                                            &format!("{name} ({})", sprint.id),
                                            CYCLE_NAME_LIMIT,
                                        ),
                                        ..fields.clone()
                                    };
                                    app.add_cycle(self.who, self.project_id, &apart)
                                }
                                e => Err(e),
                            });
                    match made {
                        Ok(c) => {
                            self.report.cycles.created += 1;
                            c
                        }
                        Err(e) => {
                            self.warn(format!("the cycle of {}: {e}", sprint.name));
                            continue;
                        }
                    }
                }
            };
            app.store
                .set_imported(self.project_id, SOURCE, "sprint", &id, cycle.id, &seen)?;
            cycles.insert(sprint.id, cycle.id);
        }
        Ok(cycles)
    }

    /// Closes the cycle of each closed sprint, now that its cards are in it; what is not done leaves it, as Jira's
    /// unfinished issues go back to the backlog.
    pub(super) fn close_sprints(&mut self, read: &Read, cycles: &BTreeMap<i64, i64>) -> Result<()> {
        let closed: Vec<(i64, i64)> = read
            .issues
            .iter()
            .flat_map(|i| &i.sprints)
            .filter(|s| s.state == "closed")
            .filter_map(|s| cycles.get(&s.id).map(|c| (s.id, *c)))
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect();
        for (sprint, cycle) in closed {
            if self
                .app
                .store
                .cycle(cycle)?
                .is_some_and(|c| c.closed_at.is_none())
                && let Err(e) = self.app.close_cycle(self.who, cycle, &Carry::Out)
            {
                self.warn(format!("the cycle of the sprint {sprint} stays open: {e}"));
            }
        }
        Ok(())
    }
}
