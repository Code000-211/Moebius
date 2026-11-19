//! Saved views: a filter of the filter bar with a layout and a sort, kept for one person or shared with everyone. A
//! view is a question, not an answer: its cards are worked out each time it is opened, for the person who opens it, so
//! a shared view shows every member the same filter within what each may see (`App::sight`), and a card of a private
//! project the person is no member of is never in it. A personal view is not even known to anyone else. The records are
//! in `store::views`.

use std::cmp::Ordering;
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{App, AppError, Result};
use crate::decide::Actor;
use crate::store::{
    Card, Cycle, LAYOUTS, NewView, Priority, Role, SORTS, View, ViewFilter, is_planning_label,
};

/// The most characters a view's name has.
pub const NAME_LIMIT: usize = 80;
/// The most values one part of a filter has, and the most characters its text has.
pub const FILTER_LIMIT: usize = 100;

fn default_layout() -> String {
    "list".into()
}

fn default_sort() -> String {
    "created".into()
}

/// The fields a person gives a view.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ViewFields {
    pub name: String,
    /// Whether every member sees the view; by default only its owner does.
    #[serde(default)]
    pub shared: bool,
    #[serde(default)]
    pub filter: ViewFilter,
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default)]
    pub descending: bool,
}

/// What a person changes of a view: every field is optional, and one left out stays as it is, so that renaming a view
/// does not unshare it or reset its filter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ViewEdit {
    pub name: Option<String>,
    pub shared: Option<bool>,
    pub filter: Option<ViewFilter>,
    pub layout: Option<String>,
    pub sort: Option<String>,
    pub descending: Option<bool>,
}

impl ViewEdit {
    /// The fields of `before` with what this edit sets.
    fn onto(&self, before: &View) -> ViewFields {
        let edit = self.clone();
        ViewFields {
            name: edit.name.unwrap_or_else(|| before.name.clone()),
            shared: edit.shared.unwrap_or(before.shared),
            filter: edit.filter.unwrap_or_else(|| before.filter.clone()),
            layout: edit.layout.unwrap_or_else(|| before.layout.clone()),
            sort: edit.sort.unwrap_or_else(|| before.sort.clone()),
            descending: edit.descending.unwrap_or(before.descending),
        }
    }
}

/// A card of a view. `cycle` is the open cycle the card is in, filled in only for the `timeline` layout, which sets
/// the cards against the cycles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewCard {
    #[serde(flatten)]
    pub card: Card,
    pub cycle: Option<Cycle>,
}

/// A view with the cards it shows to the person who asked, in the view's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewCards {
    pub view: View,
    pub cards: Vec<ViewCard>,
}

fn check(fields: &ViewFields) -> Result<()> {
    let name = &fields.name;
    if name.trim() != name
        || name.is_empty()
        || name.chars().count() > NAME_LIMIT
        || name.chars().any(char::is_control)
    {
        return Err(AppError::Invalid(format!(
            "{name:?} is not a view name: 1 to {NAME_LIMIT} characters, no control character, not starting or ending with a space"
        )));
    }
    if !LAYOUTS.contains(&fields.layout.as_str()) {
        return Err(AppError::Invalid(format!(
            "layout is one of {}, not {:?}",
            LAYOUTS.join(", "),
            fields.layout
        )));
    }
    if !SORTS.contains(&fields.sort.as_str()) {
        return Err(AppError::Invalid(format!(
            "sort is one of {}, not {:?}",
            SORTS.join(", "),
            fields.sort
        )));
    }
    let f = &fields.filter;
    let lengths = [
        f.projects.len(),
        f.repositories.len(),
        f.labels.len(),
        f.assignees.len(),
        f.priorities.len(),
        f.cycles.len(),
        f.milestones.len(),
        f.columns.len(),
        f.authors.len(),
        f.text.chars().count(),
    ];
    if lengths.iter().any(|n| *n > FILTER_LIMIT) {
        return Err(AppError::Invalid(format!(
            "a filter has at most {FILTER_LIMIT} values for each part, and its text {FILTER_LIMIT} characters"
        )));
    }
    for priority in &f.priorities {
        if Priority::parse(priority).is_none() {
            return Err(AppError::Invalid(format!(
                "{priority:?} is not a priority: urgent, high, medium, low or none"
            )));
        }
    }
    Ok(())
}

/// The ordering of two cards under `sort`; `descending` turns it round, but a card without a due date or an estimate
/// stays after those that have one.
fn compare(sort: &str, descending: bool, a: &Card, b: &Card) -> Ordering {
    let turn = |o: Ordering| if descending { o.reverse() } else { o };
    let order = match sort {
        "updated" => turn(a.moved_at.cmp(&b.moved_at)),
        "priority" => turn(a.priority.cmp(&b.priority)),
        "title" => turn(a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        "due" => optional(&a.due, &b.due, descending),
        "estimate" => optional(&a.estimate, &b.estimate, descending),
        _ => turn(a.id.cmp(&b.id)),
    };
    order.then(a.id.cmp(&b.id))
}

/// Orders two values that a card may lack: a card without one comes after a card with one, whichever way round.
fn optional<T: Ord>(a: &Option<T>, b: &Option<T>, descending: bool) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) if descending => y.cmp(x),
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

impl App {
    /// The person behind `who`, or the refusal of anyone else: views belong to people.
    fn view_person(who: &Actor) -> Result<i64> {
        match who {
            Actor::Member { id, .. } => Ok(*id),
            _ => Err(AppError::Forbidden("only a person has views".to_string())),
        }
    }

    /// The views `who` may open: the shared ones and their own, in the order they were made.
    pub fn views(&self, who: &Actor) -> Result<Vec<View>> {
        let sight = self.sight(who)?;
        let person = Self::view_person(who)?;
        Ok(self
            .store
            .views_of(person)?
            .into_iter()
            .map(|view| self.unnamed(&sight, person, view))
            .collect())
    }

    /// The view `id` for `who`. A personal view of someone else is not found, as if there were none.
    pub fn view(&self, who: &Actor, id: i64) -> Result<View> {
        let person = Self::view_person(who)?;
        let sight = self.sight(who)?;
        self.store
            .view(id)?
            .filter(|view| view.shared || view.owner_id == person)
            .map(|view| self.unnamed(&sight, person, view))
            .ok_or_else(|| AppError::NotFound(format!("view {id}")))
    }

    /// `view` without the ids of the projects, repositories and cycles `sight` may not see, so that a shared view
    /// does not tell a non-member that a private project exists. The cards were never at risk: they pass `sight`.
    /// It also says whether `person` may change it.
    fn unnamed(&self, sight: &crate::app::Sight, person: i64, mut view: View) -> View {
        view.can_change = self.may_change_view(person, &view).unwrap_or(false);
        view.filter.projects.retain(|id| sight.project(*id));
        view.filter.repositories.retain(|id| sight.repository(*id));
        view.filter.cycles.retain(|id| {
            self.store
                .cycle(*id)
                .ok()
                .flatten()
                .is_some_and(|c| sight.project(c.project_id))
        });
        view
    }

    /// Whether `person` may change `view`: its owner may, and so may an owner of the organisation.
    fn may_change_view(&self, person: i64, view: &View) -> Result<bool> {
        if view.owner_id == person {
            return Ok(true);
        }
        Ok(self
            .store
            .person(person)?
            .is_some_and(|p| p.role == Role::Owner && !p.disabled))
    }

    /// Checks what a filter names: a project, repository, cycle or milestone has to exist, and one `who` may not see
    /// is as good as missing, so that a view cannot be used to find out about a private project.
    fn check_filter(&self, who: &Actor, filter: &ViewFilter) -> Result<()> {
        let sight = self.sight(who)?;
        for id in &filter.projects {
            if !sight.project(*id) || self.store.project(*id)?.is_none() {
                return Err(AppError::Invalid(format!("no project {id}")));
            }
        }
        for id in &filter.repositories {
            if !sight.repository(*id) || self.store.repository(*id)?.is_none() {
                return Err(AppError::Invalid(format!("no repository {id}")));
            }
        }
        for id in &filter.cycles {
            match self.store.cycle(*id)? {
                Some(cycle) if sight.project(cycle.project_id) => {}
                _ => return Err(AppError::Invalid(format!("no cycle {id}"))),
            }
        }
        for id in &filter.milestones {
            if self.store.milestone(*id)?.is_none() {
                return Err(AppError::Invalid(format!("no milestone {id}")));
            }
        }
        Ok(())
    }

    /// Makes a view for `who`. Only a member or an owner may; it is shared when `fields.shared` says so.
    pub fn add_view(&self, who: &Actor, fields: &ViewFields) -> Result<View> {
        let person = Self::view_person(who)?;
        check(fields)?;
        self.check_filter(who, &fields.filter)?;
        let mut view = self.store.add_view(person, &new_view(fields))?;
        view.can_change = true;
        Ok(view)
    }

    /// Sets the fields `edit` gives of the view `id` and keeps the others. Its owner and an owner of the organisation
    /// may change it; anyone else who can see it is refused.
    pub fn edit_view(&self, who: &Actor, id: i64, edit: &ViewEdit) -> Result<View> {
        let person = Self::view_person(who)?;
        let before = self.view(who, id)?;
        if !self.may_change_view(person, &before)? {
            return Err(AppError::Forbidden(
                "only the owner of a view or of the organisation changes it".into(),
            ));
        }
        // What is kept is the stored filter, with the ids `who` may not see in it.
        let stored = self
            .store
            .view(id)?
            .ok_or_else(|| AppError::NotFound(format!("view {id}")))?;
        let fields = &edit.onto(&stored);
        check(fields)?;
        // Only what the edit adds is checked, so a view whose project the person has since lost can still be renamed.
        let mut added = fields.filter.clone();
        added
            .projects
            .retain(|p| !stored.filter.projects.contains(p));
        added
            .repositories
            .retain(|r| !stored.filter.repositories.contains(r));
        added.cycles.retain(|c| !stored.filter.cycles.contains(c));
        added
            .milestones
            .retain(|m| !stored.filter.milestones.contains(m));
        self.check_filter(who, &added)?;
        self.store.edit_view(id, &new_view(fields))?;
        self.view(who, id).or_else(|_| {
            // Made personal by an owner of the organisation, who is not its owner: it is no longer theirs to see.
            let mut view = self
                .store
                .view(id)?
                .ok_or_else(|| AppError::NotFound(format!("view {id}")))?;
            // Reaching here means `may_change_view` passed.
            view.can_change = true;
            Ok(view)
        })
    }

    /// Deletes the view `id` and answers it; who may is as for [`App::edit_view`].
    pub fn delete_view(&self, who: &Actor, id: i64) -> Result<View> {
        let person = Self::view_person(who)?;
        let view = self.view(who, id)?;
        if !self.may_change_view(person, &view)? {
            return Err(AppError::Forbidden(
                "only the owner of a view or of the organisation deletes it".into(),
            ));
        }
        self.store.delete_view(id)?;
        Ok(view)
    }

    /// The cards the view `id` shows `who`: the cards of the projects they may see that pass its filter, in its order.
    pub fn view_cards(&self, who: &Actor, id: i64) -> Result<ViewCards> {
        let view = self.view(who, id)?;
        let sight = self.sight(who)?;
        let stored = self.store.view(id)?.unwrap_or_else(|| view.clone());
        let filter = &stored.filter;
        let mut in_cycles = HashSet::new();
        for cycle in &filter.cycles {
            in_cycles.extend(self.store.cycle_cards(*cycle)?.into_iter().map(|c| c.id));
        }
        let mut in_milestones = HashSet::new();
        for milestone in &filter.milestones {
            in_milestones.extend(
                self.store
                    .milestone_cards(*milestone)?
                    .into_iter()
                    .map(|c| c.id),
            );
        }
        let priorities: Vec<Priority> = filter
            .priorities
            .iter()
            .filter_map(|p| Priority::parse(p))
            .collect();
        let any_of = |wanted: &[String], have: &str| {
            wanted.is_empty() || wanted.iter().any(|w| w.eq_ignore_ascii_case(have))
        };
        let text = filter.text.trim().to_lowercase();
        let mut cards: Vec<Card> = self
            .store
            .all_cards()?
            .into_iter()
            .filter(|card| sight.card(card))
            .filter(|card| {
                (filter.projects.is_empty() || filter.projects.contains(&card.project_id))
                    && (filter.repositories.is_empty()
                        || filter.repositories.contains(&card.repository_id))
                    && (filter.labels.is_empty()
                        || card
                            .labels
                            .iter()
                            .filter(|l| !is_planning_label(l))
                            .any(|l| filter.labels.iter().any(|w| w.eq_ignore_ascii_case(l))))
                    && (filter.assignees.is_empty()
                        || card.assignees.iter().any(|a| any_of(&filter.assignees, a)))
                    && (priorities.is_empty() || priorities.contains(&card.priority))
                    && (filter.cycles.is_empty() || in_cycles.contains(&card.id))
                    && (filter.milestones.is_empty() || in_milestones.contains(&card.id))
                    && any_of(&filter.columns, &card.column)
                    && any_of(&filter.authors, &card.author)
                    && (text.is_empty()
                        || card.title.to_lowercase().contains(&text)
                        || card.body.to_lowercase().contains(&text))
            })
            .collect();
        cards.sort_by(|a, b| compare(&view.sort, view.descending, a, b));
        let cards = cards
            .into_iter()
            .map(|card| {
                let cycle = if view.layout == "timeline" {
                    self.store.card_cycle(card.id)?
                } else {
                    None
                };
                Ok(ViewCard { card, cycle })
            })
            .collect::<Result<_>>()?;
        Ok(ViewCards { view, cards })
    }
}

fn new_view(fields: &ViewFields) -> NewView<'_> {
    NewView {
        name: &fields.name,
        shared: fields.shared,
        filter: &fields.filter,
        layout: &fields.layout,
        sort: &fields.sort,
        descending: fields.descending,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::time::Duration as Wait;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::{Store, with_due, with_estimate, with_priority};

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Wait::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        (dir, app)
    }

    fn fields(name: &str) -> ViewFields {
        ViewFields {
            name: name.into(),
            shared: false,
            filter: ViewFilter::default(),
            layout: "list".into(),
            sort: "created".into(),
            descending: false,
        }
    }

    /// An edit that only renames.
    fn rename(name: &str) -> ViewEdit {
        ViewEdit {
            name: Some(name.into()),
            ..ViewEdit::default()
        }
    }

    fn titles(app: &App, who: &Actor, fields: ViewFields) -> Vec<String> {
        let view = app.add_view(who, &fields).unwrap();
        app.view_cards(who, view.id)
            .unwrap()
            .cards
            .into_iter()
            .map(|c| c.card.title)
            .collect()
    }

    fn owner() -> Actor {
        Actor::first_owner()
    }

    /// Two repositories (so two projects), four cards that differ in every part a filter looks at.
    struct World {
        app: App,
        _dir: tempfile::TempDir,
        web: (i64, i64),
        api: (i64, i64),
        cycle: i64,
        milestone: i64,
    }

    fn world() -> World {
        let (dir, app) = app();
        let s = &app.store;
        let web = s.add_repository("web", "/w", "none", "main").unwrap();
        let api = s.add_repository("api", "/a", "none", "main").unwrap();
        let ben = s
            .add_person("ben", "Ben", crate::store::Role::Member)
            .unwrap();
        let login = s
            .add_card(web.id, "Login fails", "stack trace", "lea", "Ready")
            .unwrap();
        let logout = s.add_card(web.id, "Logout", "", "ben", "Backlog").unwrap();
        let pay = s
            .add_card(api.id, "Pay", "Login token", "lea", "Ready")
            .unwrap();
        let docs = s.add_card(api.id, "Docs", "", "ben", "Done").unwrap();
        let labels = |card: &crate::store::Card, labels: Vec<String>| {
            s.set_labels(card.id, &labels.into_iter().collect::<BTreeSet<_>>())
                .unwrap();
        };
        labels(
            &login,
            with_due(
                &with_priority(&["bug".to_string()], Priority::Urgent),
                Some("2026-12-02"),
            ),
        );
        labels(
            &logout,
            with_estimate(&with_priority(&[], Priority::Low), Some(3)),
        );
        labels(
            &pay,
            with_due(
                &with_priority(&["bug".to_string(), "money".to_string()], Priority::High),
                Some("2026-12-01"),
            ),
        );
        s.set_card_assignees(login.id, &BTreeSet::from([ben.id]))
            .unwrap();
        s.set_card_assignees(logout.id, &BTreeSet::from([1]))
            .unwrap();
        let cycle = s
            .add_cycle(web.home_project_id, "Cycle 1", "2026-11-02", "2026-11-15")
            .unwrap();
        s.set_card_cycle(login.id, Some(cycle.id)).unwrap();
        let milestone = s.add_milestone("Launch", "2027-01-31", "").unwrap();
        s.set_card_milestones(pay.id, &[milestone.id]).unwrap();
        s.set_card_milestones(docs.id, &[milestone.id]).unwrap();
        World {
            app,
            _dir: dir,
            web: (web.id, web.home_project_id),
            api: (api.id, api.home_project_id),
            cycle: cycle.id,
            milestone: milestone.id,
        }
    }

    fn with(f: impl FnOnce(&mut ViewFilter)) -> ViewFields {
        let mut view = fields("v");
        f(&mut view.filter);
        view
    }

    #[test]
    fn every_filter_shows_the_cards_that_pass_it() {
        let w = world();
        let who = owner();
        let by = |filter: ViewFields| titles(&w.app, &who, filter);
        assert_eq!(by(fields("all")).len(), 4);
        assert_eq!(
            by(with(|f| f.projects = vec![w.api.1])),
            ["Pay", "Docs"],
            "projects"
        );
        assert_eq!(
            by(with(|f| f.repositories = vec![w.web.0])),
            ["Login fails", "Logout"],
            "repositories"
        );
        assert_eq!(
            by(with(|f| f.labels = vec!["BUG".into()])),
            ["Login fails", "Pay"],
            "labels, in any case"
        );
        assert_eq!(
            by(with(|f| f.assignees = vec!["Ben".into(), "owner".into()])),
            ["Login fails", "Logout"],
            "assignees"
        );
        assert_eq!(
            by(with(|f| f.priorities = vec!["urgent".into(), "low".into()])),
            ["Login fails", "Logout"],
            "priority"
        );
        assert_eq!(
            by(with(|f| f.cycles = vec![w.cycle])),
            ["Login fails"],
            "cycle"
        );
        assert_eq!(
            by(with(|f| f.milestones = vec![w.milestone])),
            ["Pay", "Docs"],
            "milestone"
        );
        assert_eq!(
            by(with(|f| f.columns = vec!["ready".into()])),
            ["Login fails", "Pay"],
            "column"
        );
        assert_eq!(
            by(with(|f| f.authors = vec!["BEN".into()])),
            ["Logout", "Docs"],
            "author"
        );
        assert_eq!(
            by(with(|f| f.text = "login".into())),
            ["Login fails", "Pay"],
            "text, in the title or the body"
        );
        assert_eq!(
            by(with(|f| {
                f.labels = vec!["bug".into()];
                f.columns = vec!["Ready".into()];
                f.projects = vec![w.api.1];
            })),
            ["Pay"],
            "every part must match"
        );
        assert!(by(with(|f| f.text = "nothing like it".into())).is_empty());
        // The priority, due date and estimate of a card are kept as labels, and have filters of their own: a label
        // filter does not match them.
        for label in [
            "priority:urgent",
            "due:2026-12-02",
            "estimate:3",
            "Priority:High",
        ] {
            assert!(
                by(with(|f| f.labels = vec![label.into()])).is_empty(),
                "{label}"
            );
        }
        assert_eq!(
            by(with(|f| f.priorities = vec!["urgent".into()])),
            ["Login fails"]
        );
    }

    #[test]
    fn every_sort_orders_the_cards_and_turns_round() {
        let w = world();
        let who = owner();
        let sorted = |sort: &str, descending: bool| {
            let mut view = fields("v");
            view.sort = sort.into();
            view.descending = descending;
            titles(&w.app, &who, view)
        };
        assert_eq!(
            sorted("created", false),
            ["Login fails", "Logout", "Pay", "Docs"]
        );
        assert_eq!(
            sorted("created", true),
            ["Docs", "Pay", "Logout", "Login fails"]
        );
        assert_eq!(
            sorted("title", false),
            ["Docs", "Login fails", "Logout", "Pay"]
        );
        assert_eq!(
            sorted("priority", false),
            ["Login fails", "Pay", "Logout", "Docs"]
        );
        assert_eq!(
            sorted("priority", true),
            ["Docs", "Logout", "Pay", "Login fails"]
        );
        // The card without a due date stays last, either way round.
        assert_eq!(
            sorted("due", false),
            ["Pay", "Login fails", "Logout", "Docs"]
        );
        assert_eq!(
            sorted("due", true),
            ["Login fails", "Pay", "Logout", "Docs"]
        );
        assert_eq!(
            sorted("estimate", false),
            ["Logout", "Login fails", "Pay", "Docs"]
        );
        assert_eq!(sorted("updated", false).len(), 4);
        let by_moved = |descending| {
            let mut view = fields("v");
            view.sort = "updated".into();
            view.descending = descending;
            let made = w.app.add_view(&who, &view).unwrap();
            let cards = w.app.view_cards(&who, made.id).unwrap().cards;
            cards.iter().map(|c| c.card.moved_at).collect::<Vec<_>>()
        };
        let up = by_moved(false);
        assert!(up.windows(2).all(|p| p[0] <= p[1]), "{up:?}");
        let down = by_moved(true);
        assert!(down.windows(2).all(|p| p[0] >= p[1]), "{down:?}");
    }

    #[test]
    fn every_layout_is_kept_and_the_timeline_sets_cards_against_their_cycle() {
        let w = world();
        let who = owner();
        for layout in ["list", "board", "calendar", "timeline"] {
            let mut view = fields(layout);
            view.layout = layout.into();
            let made = w.app.add_view(&who, &view).unwrap();
            let shown = w.app.view_cards(&who, made.id).unwrap();
            assert_eq!(shown.view.layout, layout);
            let cycles: Vec<Option<String>> = shown
                .cards
                .iter()
                .map(|c| c.cycle.as_ref().map(|c| c.name.clone()))
                .collect();
            if layout == "timeline" {
                assert_eq!(cycles[0].as_deref(), Some("Cycle 1"));
                assert_eq!(cycles.iter().flatten().count(), 1);
            } else {
                assert!(cycles.iter().all(Option::is_none), "{layout}");
            }
        }
    }

    #[test]
    fn fields_that_cannot_be_kept_are_refused() {
        let w = world();
        let who = owner();
        let refused =
            |view: ViewFields| matches!(w.app.add_view(&who, &view), Err(AppError::Invalid(_)));
        assert!(refused(fields(" padded")));
        assert!(refused(fields("")));
        let mut layout = fields("v");
        layout.layout = "gantt".into();
        assert!(refused(layout));
        let mut sort = fields("v");
        sort.sort = "colour".into();
        assert!(refused(sort));
        assert!(refused(with(|f| f.priorities = vec!["soon".into()])));
        assert!(refused(with(|f| f.projects = vec![999])));
        assert!(refused(with(|f| f.repositories = vec![999])));
        assert!(refused(with(|f| f.cycles = vec![999])));
        assert!(refused(with(|f| f.milestones = vec![999])));
        assert!(refused(with(|f| f.labels = vec!["x".into(); 101])));
        let agent = Actor::Agent {
            name: "Moebius".into(),
            run: 1,
        };
        assert!(matches!(
            w.app.add_view(&agent, &fields("v")),
            Err(AppError::Forbidden(_))
        ));
    }

    #[test]
    fn a_personal_view_is_its_persons_and_a_shared_one_is_everyones() {
        let w = world();
        let app = &w.app;
        let lea = app
            .store
            .add_person("lea", "Lea", crate::store::Role::Member)
            .unwrap();
        let ben = app
            .store
            .add_person("ben2", "Ben", crate::store::Role::Member)
            .unwrap();
        let (lea, ben) = (Actor::person(lea.id, "lea"), Actor::person(ben.id, "ben2"));
        let mine = app.add_view(&lea, &fields("Mine")).unwrap();
        let mut team = fields("Team");
        team.shared = true;
        let team = app.add_view(&lea, &team).unwrap();

        let seen = app.views(&ben).unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!((seen[0].id, seen[0].can_change), (team.id, false));
        assert!(matches!(
            app.view(&ben, mine.id),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            app.view_cards(&ben, mine.id),
            Err(AppError::NotFound(_))
        ));
        assert_eq!(app.view_cards(&ben, team.id).unwrap().cards.len(), 4);
        // Seeing a shared view is not changing it; its owner and an owner of the organisation may.
        assert!(matches!(
            app.edit_view(&ben, team.id, &rename("Mine now")),
            Err(AppError::Forbidden(_))
        ));
        assert!(matches!(
            app.delete_view(&ben, team.id),
            Err(AppError::Forbidden(_))
        ));
        assert!(matches!(
            app.delete_view(&ben, mine.id),
            Err(AppError::NotFound(_))
        ));
        // A rename alone keeps the view shared; unsharing says so.
        let renamed = app.edit_view(&lea, team.id, &rename("Renamed")).unwrap();
        assert_eq!((renamed.name.as_str(), renamed.shared), ("Renamed", true));
        let unshared = ViewEdit {
            shared: Some(false),
            ..ViewEdit::default()
        };
        let renamed = app.edit_view(&lea, team.id, &unshared).unwrap();
        assert_eq!((renamed.name.as_str(), renamed.shared), ("Renamed", false));
        assert!(app.views(&ben).unwrap().is_empty());
        // Not even an owner of the organisation sees a personal view; they may delete a shared one.
        assert!(matches!(
            app.delete_view(&owner(), mine.id),
            Err(AppError::NotFound(_))
        ));
        let again = ViewEdit {
            name: Some("Team".into()),
            shared: Some(true),
            ..ViewEdit::default()
        };
        app.edit_view(&lea, team.id, &again).unwrap();
        app.delete_view(&owner(), team.id).unwrap();
        assert_eq!(app.views(&lea).unwrap(), [mine]);
    }

    #[test]
    fn a_shared_view_shows_each_member_the_same_filter_within_what_they_may_see() {
        let w = world();
        let app = &w.app;
        let lea = app
            .store
            .add_person("lea", "Lea", crate::store::Role::Member)
            .unwrap();
        let ben = app
            .store
            .add_person("ben2", "Ben", crate::store::Role::Member)
            .unwrap();
        app.store
            .set_project_access(w.api.1, true, &[lea.id])
            .unwrap();
        let (lea, ben) = (Actor::person(lea.id, "lea"), Actor::person(ben.id, "ben2"));
        let mut shared = fields("Bugs");
        shared.shared = true;
        shared.filter.labels = vec!["bug".into()];
        let view = app.add_view(&lea, &shared).unwrap();
        let shown = |who: &Actor| {
            app.view_cards(who, view.id)
                .unwrap()
                .cards
                .into_iter()
                .map(|c| c.card.title)
                .collect::<Vec<_>>()
        };
        assert_eq!(shown(&lea), ["Login fails", "Pay"]);
        assert_eq!(shown(&ben), ["Login fails"]);
        // A view a member made over a private project does not name it to a non-member.
        let mut naming = fields("Private");
        naming.shared = true;
        naming.filter.projects = vec![w.api.1];
        naming.filter.repositories = vec![w.api.0];
        let named = app.add_view(&lea, &naming).unwrap();
        assert_eq!(app.view(&lea, named.id).unwrap().filter.projects, [w.api.1]);
        let seen = app.view(&ben, named.id).unwrap();
        assert!(seen.filter.projects.is_empty() && seen.filter.repositories.is_empty());
        assert!(
            app.views(&ben)
                .unwrap()
                .iter()
                .all(|v| v.filter.projects.is_empty())
        );
        assert!(app.view_cards(&ben, named.id).unwrap().cards.is_empty());
        // Losing sight of a project does not stop its view being renamed.
        app.store.set_project_access(w.api.1, true, &[]).unwrap();
        assert!(app.edit_view(&lea, named.id, &rename("Renamed")).is_ok());
        // The rename kept the project lea can no longer see, and a filter sent in a PATCH replaces it.
        assert_eq!(
            app.store.view(named.id).unwrap().unwrap().filter.projects,
            [w.api.1]
        );
        let emptied = ViewEdit {
            filter: Some(ViewFilter::default()),
            ..ViewEdit::default()
        };
        app.edit_view(&lea, named.id, &emptied).unwrap();
        assert!(
            app.store
                .view(named.id)
                .unwrap()
                .unwrap()
                .filter
                .projects
                .is_empty()
        );
        // A private project is not named by a person who cannot see it.
        let mut naming = fields("Secret");
        naming.filter.projects = vec![w.api.1];
        assert!(matches!(
            app.add_view(&ben, &naming),
            Err(AppError::Invalid(_))
        ));
        let mut by_cycle = fields("Cycle");
        by_cycle.filter.repositories = vec![w.api.0];
        assert!(matches!(
            app.add_view(&ben, &by_cycle),
            Err(AppError::Invalid(_))
        ));
    }
}
