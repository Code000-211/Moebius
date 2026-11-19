//! What concerns a person, and the cards they follow (ADR 0018). Everything that tells someone something goes through
//! here, so that each person gets exactly one notice for one happening:
//!
//! - assigning a card or an inbox item tells those newly given it (`assigned`), but not the person who did it;
//! - a comment (less than a day old: the poll reads old ones too) tells the people it mentions with `@handle` (`mentioned`), and the other followers of its card
//!   (`changed`), never its author, and never one person twice;
//! - a card that moves tells its followers (`changed`), except the person who moved it; closing and reopening move a card, so they are told that way.
//!
//! The notices are the person's own to read ([`App::read_notice`] marks one read for that person alone) and are
//! announced on the bus as `notice_added` and `notice_read`, which the event stream sends only to the person they
//! are for. Telling someone is never a reason for the change to fail: a notice that cannot be stored is printed and
//! dropped. A person is told only about what they may see: nothing of a card of a private project they are no member of
//! (ADR 0018), and the desktop API leaves out a notice they no longer see.

use super::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::{ASSIGNED, CHANGED, Card, Comment, MENTIONED, NewNotice, Notice, Person};

/// How many characters of a comment a notice keeps.
const EXCERPT: usize = 140;

/// The handles `@handle` names in `text`, lowercase and in the order they first appear. A handle is lowercase letters,
/// digits, `-` and `_` (in any case here) and starts with a letter or a digit; an `@` inside a word (an address) names
/// no one. Whether it is somebody's is for [`App::mentioned_in`] to say.
pub fn handles_in(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut before: Option<char> = None;
    for (at, c) in text.char_indices() {
        let starts =
            c == '@' && before.is_none_or(|b| !(b.is_alphanumeric() || b == '_' || b == '@'));
        before = Some(c);
        if !starts {
            continue;
        }
        let rest = &text[at + 1..];
        let handle: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if handle.starts_with(|c: char| c.is_ascii_alphanumeric()) {
            let handle = handle.trim_end_matches(['-', '_']).to_ascii_lowercase();
            if !found.contains(&handle) {
                found.push(handle);
            }
        }
    }
    found
}

/// The handle of the person acting, for a notice's `by`.
pub(super) fn handle_of(by: &Actor) -> Option<String> {
    by_handle(Some(by))
}

/// The handle of the person acting, if it is a person.
fn by_handle(by: Option<&Actor>) -> Option<String> {
    match by {
        Some(Actor::Member { handle, .. }) => Some(handle.clone()),
        _ => None,
    }
}

fn is_by(by: &Option<String>, handle: &str) -> bool {
    by.as_deref()
        .is_some_and(|b| b.eq_ignore_ascii_case(handle))
}

/// How old a comment may be and still be news: the poll reads the whole history of an issue it sees for the first
/// time, and a comment from before this is not told to anyone.
const NEWS: i64 = 24 * 60 * 60;

/// `text` cut to [`EXCERPT`] characters, on one line.
fn excerpt(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= EXCERPT {
        return line;
    }
    let cut: String = line.chars().take(EXCERPT).collect();
    format!("{}…", cut.trim_end())
}

/// The handle of the person an author string names (`person:<handle>`; the bare `person` of before the migration
/// names no one in particular).
fn author_handle(author: &str) -> Option<&str> {
    author.strip_prefix("person:")
}

impl App {
    /// The people of the organisation that `text` mentions with `@handle`, in the order of the text. Disabled people
    /// are left out. This is what a comment, and later a page, tells.
    pub fn mentioned_in(&self, text: &str) -> Result<Vec<Person>> {
        let mut people = Vec::new();
        for handle in handles_in(text) {
            if let Some(person) = self.store.person_by_handle(&handle)?
                && !person.disabled
            {
                people.push(person);
            }
        }
        Ok(people)
    }

    /// The notices of the person acting, newest first. Agents and scripts have none.
    pub fn notices(&self, by: &Actor) -> Result<Vec<Notice>> {
        match by {
            Actor::Member { id, .. } => Ok(self.store.notices(*id)?),
            _ => Err(AppError::Forbidden("notices belong to a person".into())),
        }
    }

    /// Marks the notice of the person acting read, for them alone. Announces `notice_read`.
    pub fn read_notice(&self, by: &Actor, id: i64) -> Result<Notice> {
        let Actor::Member { id: person, handle } = by else {
            return Err(AppError::Forbidden("notices belong to a person".into()));
        };
        let notice = self
            .store
            .read_notice(*person, id)?
            .ok_or_else(|| AppError::NotFound(format!("notice {id}")))?;
        self.bus.publish(Event::NoticeRead {
            person: handle.clone(),
            notice: notice.clone(),
        });
        Ok(notice)
    }

    /// The handles of the people who follow the card, in the order the people were added.
    pub fn followers(&self, card_id: i64) -> Result<Vec<String>> {
        let card = self.card(card_id)?;
        Ok(self
            .store
            .followers(card_id, author_handle(&card.author))?
            .into_iter()
            .map(|person| person.handle)
            .collect())
    }

    /// Makes the person acting follow the card, or stop following it; answers who follows it now.
    pub fn set_following(&self, by: &Actor, card_id: i64, following: bool) -> Result<Vec<String>> {
        let Actor::Member { id, .. } = by else {
            return Err(AppError::Forbidden("following belongs to a person".into()));
        };
        self.card(card_id)?;
        self.store.set_following(card_id, *id, following)?;
        self.followers(card_id)
    }

    /// Tells the people newly given the card.
    pub(super) fn notify_assigned(&self, card: &Card, added: &[String], by: Option<&Actor>) {
        let by = by_handle(by);
        for handle in added.iter().filter(|h| !is_by(&by, h)) {
            self.tell(
                handle,
                NewNotice {
                    kind: ASSIGNED,
                    by: by.as_deref(),
                    card_id: Some(card.id),
                    inbox_item_id: None,
                    title: &card.title,
                    body: "Given to you",
                },
            );
        }
    }

    /// Tells the people newly given the inbox item.
    pub(super) fn notify_item_assigned(
        &self,
        item: &crate::store::InboxItem,
        added: &[String],
        by: Option<&Actor>,
    ) {
        let by = by_handle(by);
        for handle in added.iter().filter(|h| !is_by(&by, h)) {
            self.tell(
                handle,
                NewNotice {
                    kind: ASSIGNED,
                    by: by.as_deref(),
                    card_id: item.card_id,
                    inbox_item_id: Some(item.id),
                    title: &item.title,
                    body: "Given to you",
                },
            );
        }
    }

    /// Tells what a new comment concerns: the people it mentions, and the other followers of its card. The author is
    /// told nothing, and a person who is both mentioned and a follower gets the mention alone.
    pub(super) fn notify_comment(&self, comment: &Comment) {
        if crate::store::now() - comment.at > NEWS {
            return;
        }
        let Ok(card) = self.card(comment.card_id) else {
            return;
        };
        let by = author_handle(&comment.author);
        let body = excerpt(&comment.body);
        let mut told: Vec<String> = by.map(str::to_ascii_lowercase).into_iter().collect();
        let mentioned = self.mentioned_in(&comment.body).unwrap_or_default();
        let followers = self
            .store
            .followers(card.id, author_handle(&card.author))
            .unwrap_or_default();
        for (kind, people) in [(MENTIONED, mentioned), (CHANGED, followers)] {
            for person in people {
                let handle = person.handle.to_ascii_lowercase();
                if told.contains(&handle) {
                    continue;
                }
                told.push(handle);
                self.tell_person(
                    &person,
                    NewNotice {
                        kind,
                        by,
                        card_id: Some(card.id),
                        inbox_item_id: None,
                        title: &card.title,
                        body: &body,
                    },
                );
            }
        }
    }

    /// Tells the followers of the card that it moved from one column to another, except the person who moved it.
    pub(super) fn notify_moved(&self, card_id: i64, from: &str, to: &str, by: Option<&Actor>) {
        let Ok(card) = self.card(card_id) else {
            return;
        };
        let followers = self
            .store
            .followers(card.id, author_handle(&card.author))
            .unwrap_or_default();
        let by = by_handle(by);
        for person in followers.iter().filter(|p| !is_by(&by, &p.handle)) {
            self.tell_person(
                person,
                NewNotice {
                    kind: CHANGED,
                    by: by.as_deref(),
                    card_id: Some(card.id),
                    inbox_item_id: None,
                    title: &card.title,
                    body: &format!("Moved from {from} to {to}"),
                },
            );
        }
    }

    /// Tells the person with `handle`, if they are somebody and not disabled.
    fn tell(&self, handle: &str, notice: NewNotice<'_>) {
        match self.store.person_by_handle(handle) {
            Ok(Some(person)) if !person.disabled => self.tell_person(&person, notice),
            Ok(_) => {}
            Err(error) => eprintln!("moebius: cannot tell @{handle}: {error}"),
        }
    }

    /// Tells `person`, unless the notice is of a card of a private project they are no member of (ADR 0018): a mention
    /// there tells them nothing, not even its title.
    pub(super) fn tell_person(&self, person: &Person, notice: NewNotice<'_>) {
        if let Some(card) = notice.card_id {
            let seen = self
                .sight_of(Some(person.id))
                .and_then(|sight| self.sees_card(&sight, card));
            if !matches!(seen, Ok(true)) {
                return;
            }
        }
        match self.store.add_notice(person.id, &notice) {
            Ok(notice) => self.bus.publish(Event::NoticeAdded {
                person: person.handle.clone(),
                notice,
            }),
            Err(error) => eprintln!("moebius: cannot tell @{}: {error}", person.handle),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::{Role, Store};

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        (dir, app)
    }

    #[test]
    fn a_mention_is_a_handle_after_an_at_that_starts_a_word() {
        assert_eq!(
            handles_in("Hi @Lea, please ask @tom-2. mail me@example.com, @@x, @-no, (@ada)"),
            ["lea", "tom-2", "ada"]
        );
        assert_eq!(handles_in("@lea @LEA @lea_"), ["lea"]);
        assert!(handles_in("no one @ all, a@b").is_empty());
    }

    #[test]
    fn a_long_comment_is_cut_to_one_line() {
        assert_eq!(excerpt("a\n  b"), "a b");
        let long = "x".repeat(300);
        assert_eq!(excerpt(&long).chars().count(), EXCERPT + 1);
    }

    fn texts(app: &App, person: &str) -> Vec<(String, String)> {
        let id = app.store.person_by_handle(person).unwrap().unwrap().id;
        app.store
            .notices(id)
            .unwrap()
            .into_iter()
            .map(|n| (n.kind, n.body))
            .collect()
    }

    #[test]
    fn assigning_a_mention_and_a_change_each_tell_exactly_the_right_person_once() {
        let (_dir, app) = app();
        for (handle, role) in [
            ("lea", Role::Member),
            ("tom", Role::Member),
            ("ada", Role::Viewer),
            ("zed", Role::Member),
        ] {
            app.store.add_person(handle, handle, role).unwrap();
        }
        let repo = app.store.add_repository("r", "/r", "none", "main").unwrap();
        let card = app
            .store
            .add_card(repo.id, "Fix it", "b", "person:lea", "Backlog")
            .unwrap();
        let lea = Actor::person(
            app.store.person_by_handle("lea").unwrap().unwrap().id,
            "lea",
        );
        let mut events = app.bus.subscribe();

        // Assigning tells the person given the card, and not one who was given it before.
        app.set_assignees(card.id, &["tom".into()]).unwrap();
        assert_eq!(
            texts(&app, "tom"),
            [("assigned".into(), "Given to you".into())]
        );
        app.set_assignees(card.id, &["tom".into(), "ada".into()])
            .unwrap();
        assert_eq!(texts(&app, "tom").len(), 1);
        assert_eq!(texts(&app, "ada").len(), 1);
        assert!(texts(&app, "zed").is_empty());

        // A comment tells who it mentions once, the other followers (the author of the card, those it is given
        // to) as a change, and not its own author.
        app.comment_issue(&lea, card.id, "@tom and @TOM, see this @zed @nobody")
            .unwrap();
        assert_eq!(texts(&app, "tom").len(), 2, "tom: assigned, mentioned");
        assert_eq!(texts(&app, "tom")[0].0, "mentioned");
        assert_eq!(texts(&app, "zed")[0].0, "mentioned");
        assert_eq!(texts(&app, "ada")[0].0, "changed", "ada follows it");
        assert!(texts(&app, "lea").is_empty(), "not the author");

        // A move tells every follower, and nobody else.
        app.move_card(card.id, "Not started", None).unwrap();
        assert_eq!(
            texts(&app, "lea"),
            [("changed".into(), "Moved from Backlog to Not started".into())]
        );
        assert_eq!(texts(&app, "tom").len(), 3);
        assert_eq!(texts(&app, "zed").len(), 1, "zed does not follow it");

        // What was told reached the bus once for each.
        let mut told = Vec::new();
        while let Ok(event) = events.try_recv() {
            if event.kind == "notice_added" {
                told.push(event.data["person"].as_str().unwrap().to_string());
            }
        }
        told.sort();
        let expected = ["ada", "ada", "ada", "lea", "tom", "tom", "tom", "zed"];
        assert_eq!(told, expected);
    }

    #[test]
    fn a_person_who_moves_a_card_is_not_told_of_it_but_the_other_followers_are() {
        let (_dir, app) = app();
        let lea = app.store.add_person("lea", "Lea", Role::Member).unwrap();
        app.store.add_person("tom", "Tom", Role::Member).unwrap();
        let repo = app.store.add_repository("r", "/r", "none", "main").unwrap();
        let card = app
            .store
            .add_card(repo.id, "t", "b", "person:tom", "Backlog")
            .unwrap();
        app.set_following(&Actor::person(lea.id, "lea"), card.id, true)
            .unwrap();

        app.move_card(card.id, "Not started", Some(&Actor::person(lea.id, "lea")))
            .unwrap();
        assert!(texts(&app, "lea").is_empty(), "lea moved it herself");
        assert_eq!(texts(&app, "tom").len(), 1, "tom follows it");

        // The engine and scripts have no actor and tell everybody.
        app.move_card(card.id, "In progress", None).unwrap();
        assert_eq!(texts(&app, "lea").len(), 1);
        assert_eq!(texts(&app, "tom").len(), 2);
        app.move_card_from(card.id, "In progress", "In review", None, None)
            .unwrap();
        assert_eq!(texts(&app, "lea").len(), 2);
        assert_eq!(texts(&app, "tom").len(), 3);
    }

    #[test]
    fn a_person_who_merges_a_change_is_not_told_but_the_poll_tells_every_follower() {
        let (_dir, app) = app();
        let lea = app.store.add_person("lea", "Lea", Role::Member).unwrap();
        app.store.add_person("tom", "Tom", Role::Member).unwrap();
        let repo = app.store.add_repository("r", "/r", "none", "main").unwrap();
        let by = Actor::person(lea.id, "lea");
        for (merged_by, lea_told) in [(Some(&by), 0), (None, 1)] {
            let card = app
                .store
                .add_card(repo.id, "t", "b", "person:tom", "Ready to merge")
                .unwrap();
            app.set_following(&by, card.id, true).unwrap();
            let change = app
                .store
                .add_change(repo.id, Some(card.id), "b", None, "h")
                .unwrap();
            app.change_merged(&change, None, merged_by).unwrap();
            assert_eq!(texts(&app, "lea").len(), lea_told);
            assert_eq!(app.card(card.id).unwrap().column, "Done");
            for (_, notice) in texts(&app, "lea") {
                assert_eq!(notice, "Moved from Ready to merge to Done");
            }
            app.store.set_following(card.id, lea.id, false).unwrap();
        }
    }

    #[test]
    fn a_person_is_not_told_of_what_they_did_themselves_and_old_comments_are_history() {
        let (_dir, app) = app();
        let lea = app.store.add_person("lea", "Lea", Role::Member).unwrap();
        app.store.add_person("tom", "Tom", Role::Member).unwrap();
        let repo = app.store.add_repository("r", "/r", "none", "main").unwrap();
        let card = app
            .store
            .add_card(repo.id, "t", "b", "person:tom", "Backlog")
            .unwrap();
        let by = Actor::person(lea.id, "lea");
        app.set_assignees_by(&by, card.id, &["lea".into(), "tom".into()])
            .unwrap();
        assert!(texts(&app, "lea").is_empty(), "lea gave it to herself");
        assert_eq!(texts(&app, "tom").len(), 1);
        // Adding one person is the same: the person added is told, unless they did it themselves.
        app.set_assignees(card.id, &[]).unwrap();
        app.add_assignee_by(&by, card.id, "lea").unwrap();
        assert!(texts(&app, "lea").is_empty());
        app.add_assignee_by(&by, card.id, "tom").unwrap();
        assert_eq!(texts(&app, "tom").len(), 2);

        let old = Comment {
            id: 1,
            card_id: card.id,
            author: "someone".into(),
            body: "@lea an old one".into(),
            at: crate::store::now() - NEWS - 10,
        };
        app.notify_comment(&old);
        assert!(texts(&app, "lea").is_empty());
        app.notify_comment(&Comment {
            at: crate::store::now(),
            ..old
        });
        assert_eq!(texts(&app, "lea").len(), 1);
    }
}
