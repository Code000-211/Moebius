//! Whose a chat is (ADR 0018): a chat belongs to the person who started it, and another person reads it only once
//! its owner shared it. Writing to a chat, ending, offering or deleting it stay with the owner. A chat without an
//! owner (one from before chats had owners, or an automation's) belongs to everyone, and so does a run that is no
//! chat. The Moebius agent and the policies of the board are not people and are not held to it. The Retro reads the
//! store, not the list of a person, so it reads every chat.

use super::{App, AppError, Chat, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::store::Run;

/// Whether `by` may read, or with `write` also change, the run `run`.
pub fn may_use(by: &Actor, run: &Run, write: bool) -> bool {
    let Actor::Member { id, .. } = by else {
        return true;
    };
    if run.purpose != "chat" {
        return true;
    }
    match run.owner_id {
        None => true,
        Some(owner) => owner == *id || (run.shared && !write),
    }
}

/// Which chats a person's list holds: those they started, those others shared with them, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatScope {
    All,
    Mine,
    Shared,
}

impl ChatScope {
    /// The scope the query value `scope` names; every chat the person may read without one.
    pub fn parse(scope: Option<&str>) -> Result<Self> {
        match scope {
            None | Some("") | Some("all") => Ok(Self::All),
            Some("mine") => Ok(Self::Mine),
            Some("shared") => Ok(Self::Shared),
            Some(other) => Err(AppError::Invalid(format!(
                "a scope is mine, shared or all, not {other}"
            ))),
        }
    }

    /// Whether the chat `run` is in the list of `by` under this scope.
    pub(super) fn holds(self, by: &Actor, run: &Run) -> bool {
        if !may_use(by, run, false) {
            return false;
        }
        let mine = is_owner(by, run);
        match self {
            Self::All => true,
            Self::Mine => mine,
            Self::Shared => !mine,
        }
    }
}

/// Whether `by` is the person who started the chat `run`.
pub(super) fn is_owner(by: &Actor, run: &Run) -> bool {
    matches!(by, Actor::Member { id, .. } if run.owner_id == Some(*id))
}

impl App {
    /// Refuses `by` the run `run_id` when it is a chat of another person that is not shared, or, with `write`, one
    /// that is not theirs at all. A run that is no chat is open to everyone.
    pub fn chat_access(&self, by: &Actor, run_id: i64, write: bool) -> Result<()> {
        let run = self.run(run_id)?;
        if may_use(by, &run, write) {
            return Ok(());
        }
        let owner = self.chat_owner_handle(&run).unwrap_or_default();
        Err(AppError::Forbidden(if run.shared {
            format!("chat {run_id} is shared by @{owner} to read; only they write to it")
        } else {
            format!("chat {run_id} belongs to @{owner}, who has not shared it")
        }))
    }

    /// The handle of the person a chat belongs to.
    pub(super) fn chat_owner_handle(&self, run: &Run) -> Option<String> {
        let person = self.store.person(run.owner_id?).ok()??;
        Some(person.handle)
    }

    /// The owner of the chat `run_id` shares it with every person of the organisation, or takes it back.
    pub fn share_chat(&self, by: &Actor, run_id: i64, shared: bool) -> Result<Chat> {
        let run = self.run(run_id)?;
        if run.purpose != "chat" {
            return Err(AppError::Invalid(format!("run {run_id} is not a chat")));
        }
        if run.owner_id.is_none() {
            return Err(AppError::Invalid(format!(
                "chat {run_id} has no owner and belongs to everyone already"
            )));
        }
        if !is_owner(by, &run) {
            return Err(AppError::Forbidden(format!(
                "only the person who started chat {run_id} shares it"
            )));
        }
        self.store.set_chat_shared(run_id, shared)?;
        self.bus.publish(Event::ChatShared {
            run: run_id,
            shared,
        });
        self.chat_view(&Run { shared, ..run }, Some(by))
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

    /// A person with `handle` and a chat they started.
    fn chat_of(app: &App, handle: &str, role: Role) -> (Actor, Run) {
        let person = app.store.add_person(handle, handle, role).unwrap();
        let run = app.store.add_run(None, "Moebius", "fake", "chat").unwrap();
        app.store.set_chat_owner(run.id, person.id).unwrap();
        (Actor::person(person.id, handle), app.run(run.id).unwrap())
    }

    #[test]
    fn a_chat_is_the_owners_until_shared_and_then_read_by_everyone_and_written_by_the_owner() {
        let (_dir, app) = app();
        let (lea, chat) = chat_of(&app, "lea", Role::Member);
        let other = app.store.add_person("ben", "Ben", Role::Member).unwrap();
        let ben = Actor::person(other.id, "ben");
        for write in [false, true] {
            assert!(app.chat_access(&lea, chat.id, write).is_ok());
            let refused = app.chat_access(&ben, chat.id, write).unwrap_err();
            assert!(matches!(refused, AppError::Forbidden(_)), "{refused}");
        }

        let shared = app.share_chat(&lea, chat.id, true).unwrap();
        assert!(shared.shared && shared.mine);
        assert!(app.chat_access(&ben, chat.id, false).is_ok());
        assert!(matches!(
            app.chat_access(&ben, chat.id, true),
            Err(AppError::Forbidden(_))
        ));
        assert!(app.chat_access(&lea, chat.id, true).is_ok());
        // Only the owner shares it and takes it back.
        assert!(matches!(
            app.share_chat(&ben, chat.id, false),
            Err(AppError::Forbidden(_))
        ));
        app.share_chat(&lea, chat.id, false).unwrap();
        assert!(app.chat_access(&ben, chat.id, false).is_err());
    }

    #[test]
    fn mine_and_shared_list_the_right_chats() {
        let (_dir, app) = app();
        let (lea, mine) = chat_of(&app, "lea", Role::Member);
        let (ben, hers) = chat_of(&app, "ben", Role::Member);
        let (_, secret) = chat_of(&app, "mia", Role::Viewer);
        let nobodys = app.store.add_run(None, "Moebius", "fake", "chat").unwrap();
        app.share_chat(&ben, hers.id, true).unwrap();
        let ids = |scope| -> Vec<i64> {
            let mut ids: Vec<i64> = app
                .chats_for(&lea, true, scope)
                .unwrap()
                .iter()
                .map(|chat| chat.id)
                .collect();
            ids.sort();
            ids
        };
        assert_eq!(ids(ChatScope::Mine), [mine.id]);
        // A chat without an owner belongs to everyone, so it is shared.
        assert_eq!(ids(ChatScope::Shared), [hers.id, nobodys.id]);
        assert_eq!(ids(ChatScope::All), [mine.id, hers.id, nobodys.id]);
        assert!(!ids(ChatScope::All).contains(&secret.id));
        let mut listed = app.chats_for(&lea, true, ChatScope::Shared).unwrap();
        listed.sort_by_key(|chat| chat.id);
        assert_eq!(listed[0].owner.as_deref(), Some("ben"));
        assert!(!listed[0].mine && listed[0].shared);
    }

    #[test]
    fn a_run_that_is_no_chat_and_an_agent_are_not_held_to_it() {
        let (_dir, app) = app();
        let (_, chat) = chat_of(&app, "lea", Role::Member);
        let other = app.store.add_person("ben", "Ben", Role::Member).unwrap();
        let ben = Actor::person(other.id, "ben");
        let code = app.store.add_run(None, "Coder", "fake", "code").unwrap();
        assert!(app.chat_access(&ben, code.id, true).is_ok());
        let agent = Actor::Agent {
            name: "Moebius".into(),
            run: 1,
        };
        assert!(app.chat_access(&agent, chat.id, true).is_ok());
    }

    #[test]
    fn the_retro_reads_the_chats_of_every_person() {
        let (_dir, app) = app();
        let (_, first) = chat_of(&app, "lea", Role::Member);
        let (_, second) = chat_of(&app, "ben", Role::Member);
        for run in [&first, &second] {
            app.store.set_run_status(run.id, "failed").unwrap();
        }
        let found = app.store.failed_runs(0, i64::MAX, None).unwrap();
        let mut runs: Vec<_> = found.iter().filter_map(|f| f.run_id).collect();
        runs.sort();
        assert_eq!(runs, [first.id, second.id]);
        assert_eq!(app.chats(true).unwrap().len(), 2);
    }

    #[test]
    fn an_unknown_scope_is_refused() {
        assert!(ChatScope::parse(Some("everything")).is_err());
        assert_eq!(ChatScope::parse(None).unwrap(), ChatScope::All);
        assert_eq!(ChatScope::parse(Some("mine")).unwrap(), ChatScope::Mine);
    }
}
