//! Giving cards and inbox items to people (ADR 0018). The handles a person names are checked against the people of the
//! organisation: one that is nobody's is refused, and so is a disabled person who was not assigned before. Assigning
//! replaces the whole set, is the same on every forge (Moebius keeps assignees itself and never writes the forge's),
//! and is announced on the bus as `card_assignees_changed` or `inbox_assignees_changed`.

use std::collections::BTreeSet;

use super::{App, AppError, Result};
use crate::bus::Event;

impl App {
    /// Replaces the people the card is given to with those named by `handles` (with or without the `@`, in any case).
    /// Announces `card_assignees_changed` when the set changed.
    pub fn set_assignees(&self, card_id: i64, handles: &[String]) -> Result<crate::store::Card> {
        let card = self.card(card_id)?;
        let people = self.people_named(handles, &card.assignees)?;
        let assignees = self.handles_of(&people)?;
        if assignees == card.assignees {
            return Ok(card);
        }
        self.store.set_card_assignees(card_id, &people)?;
        self.bus.publish(Event::CardAssigneesChanged {
            card: card_id,
            assignees,
        });
        self.card(card_id)
    }

    /// Replaces the people the inbox item is given to; like [`App::set_assignees`] for an item of the inbox.
    /// Announces `inbox_assignees_changed` when the set changed.
    pub fn set_inbox_assignees(
        &self,
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
            assignees,
        });
        Ok(self.store.inbox_item(item_id)?.unwrap_or(item))
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
    }
}
