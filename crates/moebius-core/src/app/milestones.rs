//! Milestones (ADR 0019): goals of the organisation with a target date, holding cards of any projects, and the
//! progress each shows. A person counts and sees only the cards of projects they may see: every number and every card
//! this module hands out passes [`App::may_see_project`], so a card of a project the person is not in is neither listed
//! nor counted, and a milestone that holds only such cards shows no card at all. The records are in `store::milestones`.

use serde::{Deserialize, Serialize};
use time::{Date, format_description};

use super::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{Milestone, MilestoneCard};

/// The most characters a milestone's name has.
pub const NAME_LIMIT: usize = 80;
/// The most characters a milestone's description has.
pub const DESCRIPTION_LIMIT: usize = 10_000;

/// How far a milestone is within one project: the project's cards it holds, and how many of them are done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneProject {
    pub id: i64,
    pub name: String,
    pub key: String,
    pub cards: i64,
    pub done: i64,
}

/// A milestone with its progress, counting only the cards of the projects the asker may see. A card is done when its
/// issue is closed or it is in `Done`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneView {
    #[serde(flatten)]
    pub milestone: Milestone,
    pub cards: i64,
    pub done: i64,
    /// The visible projects it holds cards of, in the order the projects were made.
    pub projects: Vec<MilestoneProject>,
}

/// One milestone with the cards the asker may see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneDetail {
    #[serde(flatten)]
    pub view: MilestoneView,
    pub cards: Vec<MilestoneCard>,
}

/// The fields a person gives a milestone.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MilestoneFields {
    pub name: String,
    pub target_date: String,
    #[serde(default)]
    pub description: String,
}

fn check(fields: &MilestoneFields) -> Result<()> {
    let name = &fields.name;
    if name.trim() != name
        || name.is_empty()
        || name.chars().count() > NAME_LIMIT
        || name.chars().any(char::is_control)
    {
        return Err(AppError::Invalid(format!(
            "{name:?} is not a milestone name: 1 to {NAME_LIMIT} characters, no control character, not starting or ending with a space"
        )));
    }
    let day =
        format_description::parse_borrowed::<2>("[year]-[month]-[day]").expect("a valid format");
    if Date::parse(&fields.target_date, &day).is_err() {
        return Err(AppError::Invalid(format!(
            "{:?} is not a target date: write it as YYYY-MM-DD",
            fields.target_date
        )));
    }
    if fields.description.chars().count() > DESCRIPTION_LIMIT {
        return Err(AppError::Invalid(format!(
            "a description has at most {DESCRIPTION_LIMIT} characters"
        )));
    }
    Ok(())
}

impl App {
    /// Whether `who` may see the cards of the project `project_id`. This is the one place that decides it: every card
    /// a milestone lists or counts goes through it. No project is private yet, so every actor may see every project;
    /// private projects (ADR 0019) make it true only for their members.
    pub fn may_see_project(&self, who: &Actor, project_id: i64) -> bool {
        let _ = (who, project_id);
        true
    }

    /// Makes a milestone. The name is free in any case.
    pub fn add_milestone(&self, fields: &MilestoneFields) -> Result<Milestone> {
        check(fields)?;
        if self.store.milestone_by_name(&fields.name)?.is_some() {
            return Err(AppError::Exists(format!(
                "a milestone named {}",
                fields.name
            )));
        }
        let milestone = self
            .store
            .add_milestone(&fields.name, &fields.target_date, &fields.description)
            .map_err(|e| {
                super::projects::taken(e, &format!("a milestone named {}", fields.name))
            })?;
        self.bus.publish(Event::MilestoneChanged(milestone.clone()));
        Ok(milestone)
    }

    /// Replaces the name, target date and description of the milestone `id`.
    pub fn edit_milestone(&self, id: i64, fields: &MilestoneFields) -> Result<Milestone> {
        let before = self.milestone(id)?;
        check(fields)?;
        if self
            .store
            .milestone_by_name(&fields.name)?
            .is_some_and(|other| other.id != id)
        {
            return Err(AppError::Exists(format!(
                "a milestone named {}",
                fields.name
            )));
        }
        let unchanged = before.name == fields.name
            && before.target_date == fields.target_date
            && before.description == fields.description;
        if unchanged {
            return Ok(before);
        }
        self.store
            .edit_milestone(id, &fields.name, &fields.target_date, &fields.description)
            .map_err(|e| {
                super::projects::taken(e, &format!("a milestone named {}", fields.name))
            })?;
        let milestone = self.milestone(id)?;
        self.bus.publish(Event::MilestoneChanged(milestone.clone()));
        Ok(milestone)
    }

    /// Deletes the milestone `id` and answers it; the cards it held stay.
    pub fn delete_milestone(&self, id: i64) -> Result<Milestone> {
        let milestone = self.milestone(id)?;
        if self.store.delete_milestone(id)? {
            self.bus.publish(Event::MilestoneRemoved { milestone: id });
        }
        Ok(milestone)
    }

    /// The milestone `id`, or `NotFound`.
    pub fn milestone(&self, id: i64) -> Result<Milestone> {
        self.store
            .milestone(id)?
            .ok_or_else(|| AppError::NotFound(format!("milestone {id}")))
    }

    /// Every milestone with its progress as `who` sees it, the nearest target date first.
    pub fn milestone_views(&self, who: &Actor) -> Result<Vec<MilestoneView>> {
        self.store
            .milestones()?
            .into_iter()
            .map(|m| {
                self.progress(m, |p| self.may_see_project(who, p))
                    .map(|d| d.view)
            })
            .collect()
    }

    /// The milestone `id` with its progress and the cards `who` may see.
    pub fn milestone_detail(&self, who: &Actor, id: i64) -> Result<MilestoneDetail> {
        self.progress(self.milestone(id)?, |p| self.may_see_project(who, p))
    }

    /// The milestone with the cards of the projects `visible` accepts, counted and listed.
    fn progress(
        &self,
        milestone: Milestone,
        visible: impl Fn(i64) -> bool,
    ) -> Result<MilestoneDetail> {
        let cards: Vec<MilestoneCard> = self
            .store
            .milestone_cards(milestone.id)?
            .into_iter()
            .filter(|c| visible(c.project_id))
            .collect();
        let mut projects = Vec::new();
        for project in self.store.projects()? {
            let of: Vec<_> = cards
                .iter()
                .filter(|c| c.project_id == project.id)
                .collect();
            if !of.is_empty() {
                projects.push(MilestoneProject {
                    id: project.id,
                    name: project.name,
                    key: project.key,
                    cards: of.len() as i64,
                    done: of.iter().filter(|c| c.done).count() as i64,
                });
            }
        }
        Ok(MilestoneDetail {
            view: MilestoneView {
                milestone,
                cards: cards.len() as i64,
                done: cards.iter().filter(|c| c.done).count() as i64,
                projects,
            },
            cards,
        })
    }

    /// The milestones the card `card_id` is in, the nearest target date first. A card of a project `who` may not see
    /// is not found.
    pub fn card_milestones(&self, who: &Actor, card_id: i64) -> Result<Vec<Milestone>> {
        self.visible_card(who, card_id)?;
        Ok(self.store.card_milestones(card_id)?)
    }

    /// Replaces the milestones the card is in with `milestones` (ids; a repeated one counts once). Announces
    /// `card_milestones_changed` when the set changed.
    pub fn set_card_milestones(
        &self,
        who: &Actor,
        card_id: i64,
        milestones: &[i64],
    ) -> Result<Vec<Milestone>> {
        self.visible_card(who, card_id)?;
        let mut ids = Vec::new();
        for id in milestones {
            self.milestone(*id)?;
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        let before = self.store.card_milestones(card_id)?;
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        let mut held: Vec<i64> = before.iter().map(|m| m.id).collect();
        held.sort_unstable();
        if sorted == held {
            return Ok(before);
        }
        self.store.set_card_milestones(card_id, &ids)?;
        let now = self.store.card_milestones(card_id)?;
        self.bus.publish(Event::CardMilestonesChanged {
            card: card_id,
            milestones: now.iter().map(|m| m.id).collect(),
        });
        Ok(now)
    }

    /// The card, unless `who` may not see its project: then it is not found, as if there were none.
    fn visible_card(&self, who: &Actor, card_id: i64) -> Result<crate::store::Card> {
        let card = self.card(card_id)?;
        if self.may_see_project(who, card.project_id) {
            Ok(card)
        } else {
            Err(AppError::NotFound(format!("card {card_id}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::Store;

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        (dir, app)
    }

    fn fields(name: &str, date: &str) -> MilestoneFields {
        MilestoneFields {
            name: name.into(),
            target_date: date.into(),
            description: String::new(),
        }
    }

    #[test]
    fn a_milestone_needs_a_free_name_and_a_real_date_and_announces_only_a_change() {
        let (_dir, app) = app();
        let mut events = app.bus.subscribe();
        let launch = app.add_milestone(&fields("Launch", "2027-01-31")).unwrap();
        assert_eq!(events.try_recv().unwrap().kind, "milestone_changed");
        for bad in ["2027-02-30", "2027-1-5", "soon", ""] {
            assert!(
                matches!(
                    app.add_milestone(&fields("Other", bad)),
                    Err(AppError::Invalid(_))
                ),
                "{bad}"
            );
        }
        for bad in ["", " x", &"x".repeat(81)] {
            assert!(matches!(
                app.add_milestone(&fields(bad, "2027-01-31")),
                Err(AppError::Invalid(_))
            ));
        }
        assert!(matches!(
            app.add_milestone(&fields("LAUNCH", "2027-01-31")),
            Err(AppError::Exists(_))
        ));
        let again = app
            .edit_milestone(launch.id, &fields("Launch", "2027-01-31"))
            .unwrap();
        assert_eq!(again, launch);
        assert!(events.try_recv().is_err());
        let moved = app
            .edit_milestone(launch.id, &fields("Launch 2", "2027-03-01"))
            .unwrap();
        assert_eq!(moved.target_date, "2027-03-01");
        assert_eq!(events.try_recv().unwrap().kind, "milestone_changed");
        app.delete_milestone(launch.id).unwrap();
        assert_eq!(events.try_recv().unwrap().kind, "milestone_removed");
        assert!(matches!(
            app.delete_milestone(launch.id),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn progress_counts_the_cards_of_two_projects_and_only_those_the_asker_may_see() {
        let (_dir, app) = app();
        let web = app
            .store
            .add_repository("web", "/w", "none", "main")
            .unwrap();
        let api = app
            .store
            .add_repository("api", "/a", "none", "main")
            .unwrap();
        let one = app
            .store
            .add_card(web.id, "a", "", "person", "Done")
            .unwrap();
        let two = app
            .store
            .add_card(web.id, "b", "", "person", "Not started")
            .unwrap();
        let three = app
            .store
            .add_card(api.id, "c", "", "person", "Not started")
            .unwrap();
        let launch = app.add_milestone(&fields("Launch", "2027-01-31")).unwrap();
        let who = Actor::first_owner();
        for card in [&one, &two, &three] {
            app.set_card_milestones(&who, card.id, &[launch.id])
                .unwrap();
        }

        let view = &app.milestone_views(&who).unwrap()[0];
        assert_eq!((view.cards, view.done), (3, 1));
        assert_eq!(
            view.projects
                .iter()
                .map(|p| (p.key.as_str(), p.cards, p.done))
                .collect::<Vec<_>>(),
            [("WEB", 2, 1), ("API", 1, 0)]
        );

        // Seen by someone who may see only the api project: the web cards are not listed and not counted.
        let hidden = app
            .progress(launch.clone(), |p| p == api.home_project_id)
            .unwrap();
        assert_eq!((hidden.view.cards, hidden.view.done), (1, 0));
        assert_eq!(
            hidden.cards.iter().map(|c| c.id).collect::<Vec<_>>(),
            [three.id]
        );
        assert_eq!(hidden.view.projects.len(), 1);
        let none = app.progress(launch, |_| false).unwrap();
        assert_eq!(
            (none.view.cards, none.view.done, none.cards.len()),
            (0, 0, 0)
        );
    }

    #[test]
    fn a_card_s_milestones_are_replaced_checked_and_announced_once() {
        let (_dir, app) = app();
        let repo = app
            .store
            .add_repository("web", "/w", "none", "main")
            .unwrap();
        let card = app
            .store
            .add_card(repo.id, "a", "", "person", "Not started")
            .unwrap();
        let late = app.add_milestone(&fields("Late", "2027-06-01")).unwrap();
        let early = app.add_milestone(&fields("Early", "2027-01-01")).unwrap();
        let who = Actor::first_owner();
        let mut events = app.bus.subscribe();
        let held = app
            .set_card_milestones(&who, card.id, &[late.id, early.id, late.id])
            .unwrap();
        assert_eq!(
            held.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            ["Early", "Late"]
        );
        assert_eq!(events.try_recv().unwrap().kind, "card_milestones_changed");
        app.set_card_milestones(&who, card.id, &[early.id, late.id])
            .unwrap();
        assert!(events.try_recv().is_err());
        assert!(matches!(
            app.set_card_milestones(&who, card.id, &[99]),
            Err(AppError::NotFound(_))
        ));
        assert_eq!(app.card_milestones(&who, card.id).unwrap().len(), 2);
        app.set_card_milestones(&who, card.id, &[]).unwrap();
        assert!(app.card_milestones(&who, card.id).unwrap().is_empty());
    }
}
