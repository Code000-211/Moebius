//! Giving cards and inbox items to people (ADR 0018). The handles a person names are checked against the people of the
//! organisation: one that is nobody's is refused, and so is a disabled person who was not assigned before. Assigning
//! replaces the whole set or adds or takes off one person (the change is applied to the people the card has now,
//! so two made in quick succession both stay), is the same on every forge (Moebius keeps assignees itself and never writes the forge's),
//! and is announced on the bus as `card_assignees_changed` or `inbox_assignees_changed`; those newly given one are told
//! (`notices`).

use std::collections::BTreeSet;

use super::{App, AppError, Result};
use crate::bus::Event;

/// The handles in `now` that were not in `before`.
fn newly(before: &[String], now: &[String]) -> Vec<String> {
    now.iter()
        .filter(|h| !before.contains(h))
        .cloned()
        .collect()
}

impl App {
    /// Replaces the people the card is given to with those named by `handles` (with or without the `@`, in any case).
    /// Announces `card_assignees_changed` when the set changed.
    pub fn set_assignees(&self, card_id: i64, handles: &[String]) -> Result<crate::store::Card> {
        self.assign_card(None, card_id, handles)
    }

    /// [`App::set_assignees`] done by `by`, who is not told of it when they gave the card to themselves.
    pub fn set_assignees_by(
        &self,
        by: &crate::decide::Actor,
        card_id: i64,
        handles: &[String],
    ) -> Result<crate::store::Card> {
        self.assign_card(Some(by), card_id, handles)
    }

    fn assign_card(
        &self,
        by: Option<&crate::decide::Actor>,
        card_id: i64,
        handles: &[String],
    ) -> Result<crate::store::Card> {
        let card = self.card(card_id)?;
        let people = self.people_named(handles, &card.assignees)?;
        let assignees = self.handles_of(&people)?;
        if assignees == card.assignees {
            return Ok(card);
        }
        self.store.set_card_assignees(card_id, &people)?;
        self.bus.publish(Event::CardAssigneesChanged {
            card: card_id,
            assignees: assignees.clone(),
        });
        self.notify_assigned(&card, &newly(&card.assignees, &assignees), by);
        self.card(card_id)
    }

    /// Replaces the people the inbox item is given to; like [`App::set_assignees`] for an item of the inbox.
    /// Announces `inbox_assignees_changed` when the set changed.
    pub fn set_inbox_assignees(
        &self,
        item_id: i64,
        handles: &[String],
    ) -> Result<crate::store::InboxItem> {
        self.assign_item(None, item_id, handles)
    }

    /// [`App::set_inbox_assignees`] done by `by`, who is not told of it when they gave the item to themselves.
    pub fn set_inbox_assignees_by(
        &self,
        by: &crate::decide::Actor,
        item_id: i64,
        handles: &[String],
    ) -> Result<crate::store::InboxItem> {
        self.assign_item(Some(by), item_id, handles)
    }

    fn assign_item(
        &self,
        by: Option<&crate::decide::Actor>,
        item_id: i64,
        handles: &[String],
    ) -> Result<crate::store::InboxItem> {
        let item = self
            .store
            .inbox_item(item_id)?
            .ok_or_else(|| AppError::NotFound(format!("inbox item {item_id}")))?;
        let people = self.people_named(handles, &item.assignees)?;
        let assignees = self.handles_of(&people)?;
        if assignees == item.assignees {
            return Ok(item);
        }
        self.store.set_inbox_assignees(item_id, &people)?;
        self.bus.publish(Event::InboxAssigneesChanged {
            item: item_id,
            assignees: assignees.clone(),
        });
        self.notify_item_assigned(&item, &newly(&item.assignees, &assignees), by);
        Ok(self.store.inbox_item(item_id)?.unwrap_or(item))
    }

    /// Gives the card to one more person, whatever the card was given to meanwhile. Announces
    /// `card_assignees_changed` when the person was not on it.
    pub fn add_assignee(&self, card_id: i64, handle: &str) -> Result<crate::store::Card> {
        self.change_assignee(None, card_id, handle, true)
    }

    /// [`App::add_assignee`] done by `by`, who is not told of it when they gave the card to themselves.
    pub fn add_assignee_by(
        &self,
        by: &crate::decide::Actor,
        card_id: i64,
        handle: &str,
    ) -> Result<crate::store::Card> {
        self.change_assignee(Some(by), card_id, handle, true)
    }

    /// Takes the card off one person (also a disabled one); nothing happens when the card was not given to them.
    pub fn remove_assignee(&self, card_id: i64, handle: &str) -> Result<crate::store::Card> {
        self.change_assignee(None, card_id, handle, false)
    }

    fn change_assignee(
        &self,
        by: Option<&crate::decide::Actor>,
        card_id: i64,
        handle: &str,
        add: bool,
    ) -> Result<crate::store::Card> {
        let card = self.card(card_id)?;
        let person = self.person_to_change(handle, &card.assignees, add)?;
        let changed = if add {
            self.store.add_card_assignee(card_id, person)?
        } else {
            self.store.remove_card_assignee(card_id, person)?
        };
        let card = self.card(card_id)?;
        if changed {
            self.bus.publish(Event::CardAssigneesChanged {
                card: card_id,
                assignees: card.assignees.clone(),
            });
            if add {
                self.notify_assigned(
                    &card,
                    &[handle.trim().trim_start_matches('@').to_string()],
                    by,
                );
            }
        }
        Ok(card)
    }

    /// Gives the inbox item to one more person; like [`App::add_assignee`].
    pub fn add_inbox_assignee(
        &self,
        item_id: i64,
        handle: &str,
    ) -> Result<crate::store::InboxItem> {
        self.change_inbox_assignee(None, item_id, handle, true)
    }

    /// [`App::add_inbox_assignee`] done by `by`, who is not told of it when they gave the item to themselves.
    pub fn add_inbox_assignee_by(
        &self,
        by: &crate::decide::Actor,
        item_id: i64,
        handle: &str,
    ) -> Result<crate::store::InboxItem> {
        self.change_inbox_assignee(Some(by), item_id, handle, true)
    }

    /// Takes the inbox item off one person; like [`App::remove_assignee`].
    pub fn remove_inbox_assignee(
        &self,
        item_id: i64,
        handle: &str,
    ) -> Result<crate::store::InboxItem> {
        self.change_inbox_assignee(None, item_id, handle, false)
    }

    fn change_inbox_assignee(
        &self,
        by: Option<&crate::decide::Actor>,
        item_id: i64,
        handle: &str,
        add: bool,
    ) -> Result<crate::store::InboxItem> {
        let item = self
            .store
            .inbox_item(item_id)?
            .ok_or_else(|| AppError::NotFound(format!("inbox item {item_id}")))?;
        let person = self.person_to_change(handle, &item.assignees, add)?;
        let changed = if add {
            self.store.add_inbox_assignee(item_id, person)?
        } else {
            self.store.remove_inbox_assignee(item_id, person)?
        };
        let item = self.store.inbox_item(item_id)?.unwrap_or(item);
        if changed {
            self.bus.publish(Event::InboxAssigneesChanged {
                item: item_id,
                assignees: item.assignees.clone(),
            });
            if add {
                let who = handle.trim().trim_start_matches('@').to_string();
                self.notify_item_assigned(&item, &[who], by);
            }
        }
        Ok(item)
    }

    /// The id of the person `handle` names. Adding a disabled person who was not assigned before is refused.
    fn person_to_change(&self, handle: &str, current: &[String], add: bool) -> Result<i64> {
        let handle = handle.trim().trim_start_matches('@');
        let person = self
            .store
            .person_by_handle(handle)?
            .ok_or_else(|| AppError::Invalid(format!("@{handle} is nobody")))?;
        let kept = current
            .iter()
            .any(|c| c.eq_ignore_ascii_case(&person.handle));
        if add && person.disabled && !kept {
            return Err(AppError::Invalid(format!("@{} is disabled", person.handle)));
        }
        Ok(person.id)
    }

    /// The ids of the people `handles` name; `current` are the handles already assigned, who stay even when disabled.
    fn people_named(&self, handles: &[String], current: &[String]) -> Result<BTreeSet<i64>> {
        let mut people = BTreeSet::new();
        for handle in handles {
            let handle = handle.trim().trim_start_matches('@');
            let person = self
                .store
                .person_by_handle(handle)?
                .ok_or_else(|| AppError::Invalid(format!("@{handle} is nobody")))?;
            let kept = current
                .iter()
                .any(|c| c.eq_ignore_ascii_case(&person.handle));
            if person.disabled && !kept {
                return Err(AppError::Invalid(format!("@{} is disabled", person.handle)));
            }
            people.insert(person.id);
        }
        Ok(people)
    }

    /// The handles of people, in the order the people were added.
    fn handles_of(&self, people: &BTreeSet<i64>) -> Result<Vec<String>> {
        let mut handles = Vec::new();
        for id in people {
            if let Some(person) = self.store.person(*id)? {
                handles.push(person.handle);
            }
        }
        Ok(handles)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use crate::app::App;
    use crate::store::{Role, Store};

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        (dir, app)
    }

    #[test]
    fn assigning_replaces_the_set_checks_the_handles_and_announces_only_a_change() {
        let (_dir, app) = app();
        let lea = app.store.add_person("lea", "Lea", Role::Member).unwrap();
        app.store.add_person("tom", "Tom", Role::Member).unwrap();
        let repo = app.store.add_repository("r", "/r", "none", "main").unwrap();
        let card = app
            .store
            .add_card(repo.id, "t", "b", "person", "Not started")
            .unwrap();
        let mut events = app.bus.subscribe();

        let card = app
            .set_assignees(card.id, &["@TOM".into(), "lea".into(), "lea".into()])
            .unwrap();
        assert_eq!(card.assignees, ["lea", "tom"]);
        let event = events.try_recv().unwrap();
        assert_eq!(event.kind, "card_assignees_changed");
        assert_eq!(
            event.data,
            serde_json::json!({"card": card.id, "assignees": ["lea", "tom"]})
        );

        // Those newly given the card are told (see `notices`); that is all there is on the bus.
        assert_eq!(events.try_recv().unwrap().kind, "notice_added");
        assert_eq!(events.try_recv().unwrap().kind, "notice_added");
        app.set_assignees(card.id, &["lea".into(), "tom".into()])
            .unwrap();
        assert!(events.try_recv().is_err(), "no change, no event");

        assert!(app.set_assignees(card.id, &["nobody".into()]).is_err());
        assert_eq!(app.card(card.id).unwrap().assignees, ["lea", "tom"]);

        app.store.set_person_disabled(lea.id, true).unwrap();
        let kept = app
            .set_assignees(card.id, &["lea".into(), "tom".into()])
            .unwrap();
        assert_eq!(kept.assignees, ["lea", "tom"], "a disabled person stays");
        app.set_assignees(card.id, &[]).unwrap();
        assert!(app.set_assignees(card.id, &["lea".into()]).is_err());
    }

    #[test]
    fn adding_and_taking_off_one_person_works_on_the_people_the_card_has_now() {
        let (_dir, app) = app();
        let lea = app.store.add_person("lea", "Lea", Role::Member).unwrap();
        app.store.add_person("tom", "Tom", Role::Member).unwrap();
        let repo = app.store.add_repository("r", "/r", "none", "main").unwrap();
        let card = app
            .store
            .add_card(repo.id, "t", "b", "person", "Not started")
            .unwrap();
        let mut events = app.bus.subscribe();

        app.add_assignee(card.id, "@Lea").unwrap();
        let card = app.add_assignee(card.id, "tom").unwrap();
        assert_eq!(
            card.assignees,
            ["lea", "tom"],
            "neither add undoes the other"
        );
        // Each add announces the set and then tells the person added (see `notices`).
        let mut sets = Vec::new();
        while let Ok(event) = events.try_recv() {
            if event.kind == "card_assignees_changed" {
                sets.push(event.data);
            }
        }
        assert_eq!(sets.len(), 2);
        assert_eq!(
            sets[1],
            serde_json::json!({"card": card.id, "assignees": ["lea", "tom"]})
        );

        app.add_assignee(card.id, "tom").unwrap();
        assert!(events.try_recv().is_err(), "no change, no event");
        assert!(app.add_assignee(card.id, "nobody").is_err());
        assert!(app.remove_assignee(card.id, "nobody").is_err());

        app.store.set_person_disabled(lea.id, true).unwrap();
        let card = app.remove_assignee(card.id, "lea").unwrap();
        assert_eq!(
            card.assignees,
            ["tom"],
            "a disabled person can be taken off"
        );
        events.try_recv().unwrap();
        app.remove_assignee(card.id, "lea").unwrap();
        assert!(events.try_recv().is_err());
        assert!(
            app.add_assignee(card.id, "lea").is_err(),
            "and not put back"
        );
    }

    #[test]
    fn an_inbox_item_is_given_to_people_too() {
        let (_dir, app) = app();
        app.store.add_person("lea", "Lea", Role::Member).unwrap();
        let item = app
            .store
            .add_inbox_item("proposal", None, "A proposal", &serde_json::json!({}))
            .unwrap();
        let mut events = app.bus.subscribe();
        let item = app.set_inbox_assignees(item.id, &["lea".into()]).unwrap();
        assert_eq!(item.assignees, ["lea"]);
        assert_eq!(events.try_recv().unwrap().kind, "inbox_assignees_changed");
        assert!(app.set_inbox_assignees(999, &[]).is_err());

        app.store.add_person("tom", "Tom", Role::Member).unwrap();
        let item = app.add_inbox_assignee(item.id, "tom").unwrap();
        assert_eq!(item.assignees, ["lea", "tom"]);
        let item = app.remove_inbox_assignee(item.id, "lea").unwrap();
        assert_eq!(item.assignees, ["tom"]);
        assert!(app.add_inbox_assignee(999, "tom").is_err());
    }
}
