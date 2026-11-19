//! Cycles (ADR 0019): time boxes of one project, each with a name, a start and an end, holding cards of that project.
//! This module keeps the rules (a card is in at most one open cycle and only one of its own project's), what a cycle's
//! page shows (its progress, its figures and its burn-up, all following its cards) and closing, which carries the
//! cards that are not done to the next cycle or lets them go, as the person closing it chooses. The records are in
//! `store::cycles`.

use serde::{Deserialize, Serialize};
use time::{Date, Duration, OffsetDateTime, format_description};

use super::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{Cycle, CycleCard};

/// The most characters a cycle's name has.
pub const NAME_LIMIT: usize = 80;
/// The most days a cycle spans, both ends counted.
pub const DAYS_LIMIT: i64 = 366;

/// The fields a person gives a cycle.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CycleFields {
    pub name: String,
    pub starts_on: String,
    pub ends_on: String,
}

/// What becomes of the cards that are not done when a cycle is closed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "carry", rename_all = "snake_case")]
pub enum Carry {
    /// They move to the open cycle `to` of the same project, or, with none given, to the one that starts next.
    Next { to: Option<i64> },
    /// They are in no open cycle.
    Out,
}

/// A cycle with how far its cards are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleView {
    #[serde(flatten)]
    pub cycle: Cycle,
    pub cards: i64,
    pub done: i64,
    /// The points of the cards that have an estimate.
    pub points: i64,
    pub points_done: i64,
}

/// The figures of a cycle's page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleFigures {
    /// The done cards an agent ran code for.
    pub by_agents: i64,
    /// The tokens the runs of the cycle's cards used.
    pub tokens: i64,
    /// Tokens per done point, rounded down; none while no point is done.
    pub tokens_per_point: Option<i64>,
}

/// One day of the burn-up: the points planned and the points done by the end of that day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BurnUpDay {
    pub day: String,
    pub scope: i64,
    pub done: i64,
}

/// A cycle with its figures, burn-up and cards: its progress as a `CycleView` has it, with the cards listed in place
/// of their count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleDetail {
    #[serde(flatten)]
    pub cycle: Cycle,
    pub done: i64,
    pub points: i64,
    pub points_done: i64,
    pub figures: CycleFigures,
    pub burn_up: Vec<BurnUpDay>,
    pub cards: Vec<CycleCard>,
}

/// What closing a cycle did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Closed {
    pub cycle: Cycle,
    /// The cycle the unfinished cards went to, if they did.
    pub carried_to: Option<Cycle>,
    /// The ids of the cards carried.
    pub carried: Vec<i64>,
}

fn day_format() -> Vec<format_description::BorrowedFormatItem<'static>> {
    format_description::parse_borrowed::<2>("[year]-[month]-[day]").expect("a valid format")
}

fn parse_day(text: &str, what: &str) -> Result<Date> {
    Date::parse(text, &day_format())
        .map_err(|_| AppError::Invalid(format!("{text:?} is not {what}: write it as YYYY-MM-DD")))
}

fn check(fields: &CycleFields) -> Result<()> {
    let name = &fields.name;
    if name.trim() != name
        || name.is_empty()
        || name.chars().count() > NAME_LIMIT
        || name.chars().any(char::is_control)
    {
        return Err(AppError::Invalid(format!(
            "{name:?} is not a cycle name: 1 to {NAME_LIMIT} characters, no control character, not starting or ending with a space"
        )));
    }
    let start = parse_day(&fields.starts_on, "a start")?;
    let end = parse_day(&fields.ends_on, "an end")?;
    if end < start {
        return Err(AppError::Invalid(format!(
            "a cycle cannot end ({}) before it starts ({})",
            fields.ends_on, fields.starts_on
        )));
    }
    if (end - start).whole_days() + 1 > DAYS_LIMIT {
        return Err(AppError::Invalid(format!(
            "a cycle spans at most {DAYS_LIMIT} days"
        )));
    }
    Ok(())
}

fn day_of(seconds: i64) -> Date {
    OffsetDateTime::from_unix_timestamp(seconds).map_or(Date::MIN, OffsetDateTime::date)
}

/// The burn-up of a cycle: for each day from its start to its end, or to the day it was closed or `now` if earlier,
/// the points of the cards added by then and of those done by then. A card counts for the points it has now.
fn burn_up(cycle: &Cycle, cards: &[CycleCard], now: i64) -> Vec<BurnUpDay> {
    let (Ok(start), Ok(end)) = (
        Date::parse(&cycle.starts_on, &day_format()),
        Date::parse(&cycle.ends_on, &day_format()),
    ) else {
        return Vec::new();
    };
    let last = end.min(day_of(cycle.closed_at.unwrap_or(now)));
    let mut days = Vec::new();
    let mut day = start;
    while day <= last {
        let (mut scope, mut done) = (0, 0);
        for card in cards {
            let points = i64::from(card.estimate.unwrap_or(0));
            let added = day_of(card.added_at);
            if added <= day {
                scope += points;
                if card.done && card.done_at.map(|at| day_of(at).max(added)) <= Some(day) {
                    done += points;
                }
            }
        }
        days.push(BurnUpDay {
            day: day.to_string(),
            scope,
            done,
        });
        day += Duration::days(1);
    }
    days
}

fn view(cycle: Cycle, cards: &[CycleCard]) -> CycleView {
    let points = |only_done: bool| {
        cards
            .iter()
            .filter(|c| c.done || !only_done)
            .map(|c| i64::from(c.estimate.unwrap_or(0)))
            .sum()
    };
    CycleView {
        cycle,
        cards: cards.len() as i64,
        done: cards.iter().filter(|c| c.done).count() as i64,
        points: points(false),
        points_done: points(true),
    }
}

impl App {
    /// The cycle `id` as `who` sees it: not found when the project is one they may not see.
    pub fn cycle(&self, who: &Actor, id: i64) -> Result<Cycle> {
        let cycle = self
            .store
            .cycle(id)?
            .ok_or_else(|| AppError::NotFound(format!("cycle {id}")))?;
        if !self.may_see_project(who, cycle.project_id) {
            return Err(AppError::NotFound(format!("cycle {id}")));
        }
        Ok(cycle)
    }

    /// Makes an open cycle of the project. The name is free in any case within the project.
    pub fn add_cycle(&self, who: &Actor, project_id: i64, fields: &CycleFields) -> Result<Cycle> {
        let project = self.project(project_id)?;
        if !self.may_see_project(who, project.id) {
            return Err(AppError::NotFound(format!("project {project_id}")));
        }
        check(fields)?;
        let taken =
            || AppError::Exists(format!("a cycle named {} in {}", fields.name, project.name));
        if self
            .store
            .cycle_by_name(project.id, &fields.name)?
            .is_some()
        {
            return Err(taken());
        }
        let cycle = self
            .store
            .add_cycle(project.id, &fields.name, &fields.starts_on, &fields.ends_on)
            .map_err(|e| super::projects::taken(e, &format!("a cycle named {}", fields.name)))?;
        self.bus.publish(Event::CycleChanged(cycle.clone()));
        Ok(cycle)
    }

    /// Replaces the name and dates of the open cycle `id`.
    pub fn edit_cycle(&self, who: &Actor, id: i64, fields: &CycleFields) -> Result<Cycle> {
        let before = self.cycle(who, id)?;
        if before.closed_at.is_some() {
            return Err(AppError::Invalid(format!(
                "{} is closed and cannot be changed",
                before.name
            )));
        }
        check(fields)?;
        if self
            .store
            .cycle_by_name(before.project_id, &fields.name)?
            .is_some_and(|other| other.id != id)
        {
            return Err(AppError::Exists(format!("a cycle named {}", fields.name)));
        }
        if before.name == fields.name
            && before.starts_on == fields.starts_on
            && before.ends_on == fields.ends_on
        {
            return Ok(before);
        }
        self.store
            .edit_cycle(id, &fields.name, &fields.starts_on, &fields.ends_on)
            .map_err(|e| super::projects::taken(e, &format!("a cycle named {}", fields.name)))?;
        let cycle = self.cycle(who, id)?;
        self.bus.publish(Event::CycleChanged(cycle.clone()));
        Ok(cycle)
    }

    /// Deletes the cycle `id` and answers it; its cards stay.
    pub fn delete_cycle(&self, who: &Actor, id: i64) -> Result<Cycle> {
        let cycle = self.cycle(who, id)?;
        let held = self.store.cycle_cards(id)?;
        if self.store.delete_cycle(id)? {
            self.bus.publish(Event::CycleRemoved { cycle: id });
            if cycle.closed_at.is_none() {
                for card in held {
                    self.bus.publish(Event::CardCycleChanged {
                        card: card.id,
                        cycle: None,
                    });
                }
            }
        }
        Ok(cycle)
    }

    /// The cycles of the project with their progress, the latest start first.
    pub fn cycle_views(&self, who: &Actor, project_id: i64) -> Result<Vec<CycleView>> {
        let project = self.project(project_id)?;
        if !self.may_see_project(who, project.id) {
            return Err(AppError::NotFound(format!("project {project_id}")));
        }
        self.store
            .cycles(project.id)?
            .into_iter()
            .map(|cycle| {
                let cards = self.store.cycle_cards(cycle.id)?;
                Ok(view(cycle, &cards))
            })
            .collect()
    }

    /// The cycle `id` with its figures, burn-up and cards.
    pub fn cycle_detail(&self, who: &Actor, id: i64) -> Result<CycleDetail> {
        let cycle = self.cycle(who, id)?;
        let cards = self.store.cycle_cards(id)?;
        let burn = burn_up(&cycle, &cards, crate::store::now());
        let view = view(cycle, &cards);
        let tokens: i64 = cards.iter().map(|c| c.tokens).sum();
        let figures = CycleFigures {
            by_agents: cards.iter().filter(|c| c.done && c.by_agent).count() as i64,
            tokens,
            tokens_per_point: (view.points_done > 0).then(|| tokens / view.points_done),
        };
        Ok(CycleDetail {
            cycle: view.cycle,
            done: view.done,
            points: view.points,
            points_done: view.points_done,
            figures,
            burn_up: burn,
            cards,
        })
    }

    /// The open cycle the card is in, if any.
    pub fn card_cycle(&self, who: &Actor, card_id: i64) -> Result<Option<Cycle>> {
        self.visible_card(who, card_id)?;
        Ok(self.store.card_cycle(card_id)?)
    }

    /// Puts the card into the open cycle `cycle` of its own project, or takes it out of its open cycle when none. A card
    /// in another open cycle leaves it. Announces `card_cycle_changed` when the card's cycle changed.
    pub fn set_card_cycle(
        &self,
        who: &Actor,
        card_id: i64,
        cycle: Option<i64>,
    ) -> Result<Option<Cycle>> {
        let card = self.visible_card(who, card_id)?;
        let target = cycle
            .map(|id| {
                let target = self.cycle(who, id)?;
                if target.project_id != card.project_id {
                    return Err(AppError::Invalid(format!(
                        "{} is a cycle of another project than the card's",
                        target.name
                    )));
                }
                if target.closed_at.is_some() {
                    return Err(AppError::Invalid(format!(
                        "{} is closed; a card is added to an open cycle",
                        target.name
                    )));
                }
                Ok(target)
            })
            .transpose()?;
        let before = self.store.card_cycle(card_id)?;
        if before.as_ref().map(|c| c.id) == target.as_ref().map(|c| c.id) {
            return Ok(before);
        }
        self.store
            .set_card_cycle(card_id, target.as_ref().map(|c| c.id))?;
        self.bus.publish(Event::CardCycleChanged {
            card: card_id,
            cycle: target.as_ref().map(|c| c.id),
        });
        Ok(target)
    }

    /// Closes the open cycle `id`. The cards that are not done go where `carry` says; the done ones stay in the closed
    /// cycle, which keeps every card it held.
    pub fn close_cycle(&self, who: &Actor, id: i64, carry: &Carry) -> Result<Closed> {
        let cycle = self.cycle(who, id)?;
        if cycle.closed_at.is_some() {
            return Err(AppError::Invalid(format!(
                "{} is already closed",
                cycle.name
            )));
        }
        let target = match carry {
            Carry::Out => None,
            Carry::Next { to: Some(to) } => {
                let target = self.cycle(who, *to)?;
                if target.id == id
                    || target.project_id != cycle.project_id
                    || target.closed_at.is_some()
                {
                    return Err(AppError::Invalid(format!(
                        "{} is not another open cycle of the same project",
                        target.name
                    )));
                }
                Some(target)
            }
            Carry::Next { to: None } => Some(
                self.store
                    .cycles(cycle.project_id)?
                    .into_iter()
                    .filter(|c| {
                        c.closed_at.is_none()
                            && (c.starts_on.as_str(), c.id) > (cycle.starts_on.as_str(), cycle.id)
                    })
                    .min_by(|a, b| (&a.starts_on, a.id).cmp(&(&b.starts_on, b.id)))
                    .ok_or_else(|| {
                        AppError::Invalid(format!(
                            "no open cycle starts after {}; make one or carry the cards out",
                            cycle.name
                        ))
                    })?,
            ),
        };
        let carried = self
            .store
            .close_cycle(id, target.as_ref().map(|c| c.id))?
            .ok_or_else(|| {
                AppError::Invalid(format!(
                    "{} was closed already, or the cycle to carry to is",
                    cycle.name
                ))
            })?;
        let closed = self.cycle(who, id)?;
        self.bus.publish(Event::CycleChanged(closed.clone()));
        for card in &carried {
            self.bus.publish(Event::CardCycleChanged {
                card: *card,
                cycle: target.as_ref().map(|c| c.id),
            });
        }
        Ok(Closed {
            cycle: closed,
            carried_to: target,
            carried,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration as Wait;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::Store;

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Wait::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        (dir, app)
    }

    fn fields(name: &str, start: &str, end: &str) -> CycleFields {
        CycleFields {
            name: name.into(),
            starts_on: start.into(),
            ends_on: end.into(),
        }
    }

    fn project(app: &App, name: &str) -> (i64, i64) {
        let repo = app
            .store
            .add_repository(name, &format!("/{name}"), "none", "main")
            .unwrap();
        (repo.id, repo.home_project_id)
    }

    #[test]
    fn a_cycle_needs_a_free_name_and_real_dates_and_announces_only_a_change() {
        let (_dir, app) = app();
        let (_, web) = project(&app, "web");
        let who = Actor::first_owner();
        let mut events = app.bus.subscribe();
        let one = app
            .add_cycle(&who, web, &fields("Cycle 1", "2026-11-02", "2026-11-15"))
            .unwrap();
        assert_eq!(events.try_recv().unwrap().kind, "cycle_changed");
        for (name, start, end) in [
            ("X", "2026-11-31", "2026-12-01"),
            ("X", "2026-11-10", "2026-11-09"),
            ("X", "2026-01-01", "2027-12-31"),
            ("X", "soon", "2026-12-01"),
            ("", "2026-11-01", "2026-11-02"),
            (" x", "2026-11-01", "2026-11-02"),
        ] {
            assert!(
                matches!(
                    app.add_cycle(&who, web, &fields(name, start, end)),
                    Err(AppError::Invalid(_))
                ),
                "{name} {start} {end}"
            );
        }
        assert!(matches!(
            app.add_cycle(&who, web, &fields("CYCLE 1", "2026-11-16", "2026-11-29")),
            Err(AppError::Exists(_))
        ));
        // Another project may use the name.
        let (_, api) = project(&app, "api");
        app.add_cycle(&who, api, &fields("Cycle 1", "2026-11-02", "2026-11-15"))
            .unwrap();
        events.try_recv().unwrap();
        let same = app
            .edit_cycle(&who, one.id, &fields("Cycle 1", "2026-11-02", "2026-11-15"))
            .unwrap();
        assert_eq!(same, one);
        assert!(events.try_recv().is_err());
        app.edit_cycle(&who, one.id, &fields("Sprint", "2026-11-02", "2026-11-16"))
            .unwrap();
        assert_eq!(events.try_recv().unwrap().kind, "cycle_changed");
        app.delete_cycle(&who, one.id).unwrap();
        assert_eq!(events.try_recv().unwrap().kind, "cycle_removed");
        assert!(matches!(
            app.delete_cycle(&who, one.id),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn a_card_is_in_at_most_one_open_cycle_and_only_of_its_own_project() {
        let (_dir, app) = app();
        let (web_repo, web) = project(&app, "web");
        let (_, api) = project(&app, "api");
        let who = Actor::first_owner();
        let card = app
            .store
            .add_card(web_repo, "a", "", "person", "Not started")
            .unwrap();
        let one = app
            .add_cycle(&who, web, &fields("One", "2026-11-02", "2026-11-15"))
            .unwrap();
        let two = app
            .add_cycle(&who, web, &fields("Two", "2026-11-16", "2026-11-29"))
            .unwrap();
        let other = app
            .add_cycle(&who, api, &fields("Other", "2026-11-02", "2026-11-15"))
            .unwrap();
        let mut events = app.bus.subscribe();
        assert!(matches!(
            app.set_card_cycle(&who, card.id, Some(other.id)),
            Err(AppError::Invalid(_))
        ));
        assert!(events.try_recv().is_err());
        app.set_card_cycle(&who, card.id, Some(one.id)).unwrap();
        assert_eq!(events.try_recv().unwrap().kind, "card_cycle_changed");
        app.set_card_cycle(&who, card.id, Some(one.id)).unwrap();
        assert!(events.try_recv().is_err());
        // Adding it to the second cycle takes it out of the first.
        app.set_card_cycle(&who, card.id, Some(two.id)).unwrap();
        assert_eq!(app.card_cycle(&who, card.id).unwrap().unwrap().id, two.id);
        assert_eq!(app.cycle_detail(&who, one.id).unwrap().cards.len(), 0);
        assert!(matches!(
            app.set_card_cycle(&who, card.id, Some(99)),
            Err(AppError::NotFound(_))
        ));
        app.set_card_cycle(&who, card.id, None).unwrap();
        assert!(app.card_cycle(&who, card.id).unwrap().is_none());
    }

    #[test]
    fn closing_carries_the_unfinished_cards_to_the_next_cycle_or_out() {
        let (_dir, app) = app();
        let (repo, web) = project(&app, "web");
        let who = Actor::first_owner();
        let done = app
            .store
            .add_card(repo, "done", "", "person", "Done")
            .unwrap();
        let open = app
            .store
            .add_card(repo, "open", "", "person", "Not started")
            .unwrap();
        let one = app
            .add_cycle(&who, web, &fields("One", "2026-11-02", "2026-11-15"))
            .unwrap();
        let three = app
            .add_cycle(&who, web, &fields("Three", "2026-12-01", "2026-12-14"))
            .unwrap();
        let two = app
            .add_cycle(&who, web, &fields("Two", "2026-11-16", "2026-11-29"))
            .unwrap();
        for card in [&done, &open] {
            app.set_card_cycle(&who, card.id, Some(one.id)).unwrap();
        }

        // No next one is an error, and nothing changed.
        let (_, other) = project(&app, "api");
        let lonely = app
            .add_cycle(&who, other, &fields("Only", "2026-11-02", "2026-11-15"))
            .unwrap();
        assert!(matches!(
            app.close_cycle(&who, lonely.id, &Carry::Next { to: None }),
            Err(AppError::Invalid(_))
        ));
        assert!(app.cycle(&who, lonely.id).unwrap().closed_at.is_none());
        assert!(matches!(
            app.close_cycle(
                &who,
                one.id,
                &Carry::Next {
                    to: Some(lonely.id)
                }
            ),
            Err(AppError::Invalid(_))
        ));

        let closed = app
            .close_cycle(&who, one.id, &Carry::Next { to: None })
            .unwrap();
        assert_eq!(closed.carried, [open.id]);
        assert_eq!(closed.carried_to.unwrap().id, two.id);
        assert!(closed.cycle.closed_at.is_some());
        assert_eq!(app.card_cycle(&who, open.id).unwrap().unwrap().id, two.id);
        assert!(app.card_cycle(&who, done.id).unwrap().is_none());
        // The closed cycle keeps what it held.
        assert_eq!(app.cycle_detail(&who, one.id).unwrap().cards.len(), 2);
        assert!(matches!(
            app.close_cycle(&who, one.id, &Carry::Out),
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            app.set_card_cycle(&who, done.id, Some(one.id)),
            Err(AppError::Invalid(_))
        ));

        // Closing and choosing a later one, or none.
        app.close_cycle(&who, two.id, &Carry::Next { to: Some(three.id) })
            .unwrap();
        assert_eq!(app.card_cycle(&who, open.id).unwrap().unwrap().id, three.id);
        let out = app.close_cycle(&who, three.id, &Carry::Out).unwrap();
        assert!(out.carried.is_empty() && out.carried_to.is_none());
        assert!(app.card_cycle(&who, open.id).unwrap().is_none());
    }

    #[test]
    fn progress_figures_and_burn_up_follow_the_cards() {
        let (_dir, app) = app();
        let (repo, web) = project(&app, "web");
        let who = Actor::first_owner();
        let a = app
            .store
            .add_card(repo, "a", "", "person", "Not started")
            .unwrap();
        let b = app
            .store
            .add_card(repo, "b", "", "person", "Not started")
            .unwrap();
        let c = app
            .store
            .add_card(repo, "c", "", "person", "Not started")
            .unwrap();
        app.set_estimate(a.id, Some(3)).unwrap();
        app.set_estimate(b.id, Some(5)).unwrap();
        let day = |n: i64| 1_793_000_000 + n * 86_400;
        let start = day_of(day(0)).to_string();
        let end = day_of(day(4)).to_string();
        let cycle = app
            .add_cycle(&who, web, &fields("One", &start, &end))
            .unwrap();
        for card in [&a, &b, &c] {
            app.set_card_cycle(&who, card.id, Some(cycle.id)).unwrap();
        }
        let detail = app.cycle_detail(&who, cycle.id).unwrap();
        assert_eq!(
            (
                detail.cards.len() as i64,
                detail.done,
                detail.points,
                detail.points_done
            ),
            (3, 0, 8, 0)
        );
        assert_eq!(detail.figures.tokens_per_point, None);

        // Done moves a card into the progress without any step of its own.
        app.store.move_card(a.id, "Done").unwrap();
        let detail = app.cycle_detail(&who, cycle.id).unwrap();
        assert_eq!(
            (detail.cards.len() as i64, detail.done, detail.points_done),
            (3, 1, 3)
        );
        assert_eq!(detail.figures.tokens_per_point, Some(0));

        // The burn-up as a pure function: scope when added, done when it got done, never past the end or the day.
        let cards = vec![
            CycleCard {
                id: 1,
                repository_id: 1,
                number: 1,
                title: "a".into(),
                column: "Done".into(),
                done: true,
                estimate: Some(3),
                added_at: day(0),
                done_at: Some(day(2)),
                by_agent: true,
                tokens: 0,
            },
            CycleCard {
                id: 2,
                repository_id: 1,
                number: 2,
                title: "b".into(),
                column: "Not started".into(),
                done: false,
                estimate: Some(5),
                added_at: day(1),
                done_at: None,
                by_agent: false,
                tokens: 0,
            },
        ];
        let open = Cycle {
            id: 1,
            project_id: 1,
            name: "One".into(),
            starts_on: start,
            ends_on: end,
            closed_at: None,
            created_at: 0,
        };
        let series = |now: i64| {
            burn_up(&open, &cards, now)
                .iter()
                .map(|d| (d.scope, d.done))
                .collect::<Vec<_>>()
        };
        assert_eq!(series(day(3)), [(3, 0), (8, 0), (8, 3), (8, 3)]);
        assert_eq!(series(day(30)).len(), 5);
        assert!(series(day(-3)).is_empty());
        let closed = Cycle {
            closed_at: Some(day(1)),
            ..open
        };
        assert_eq!(burn_up(&closed, &cards, day(30)).len(), 2);
    }
}
