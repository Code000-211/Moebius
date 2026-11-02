//! Replays (spec 7, ADR 0010): a tool call that a decider refused is run once in a throwaway copy of the worktree, and
//! what happened becomes the judgement's outcome, so that the decision can later be weighed by what it would have done.
//!
//! Only refused `tool_call` judgements are replayed, and only those that have no replay yet, whose run's
//! repository still exists and whose call is reversible (see [`crate::decide::guard::reversible`]): everything it does stays in
//! the worktree. Nothing else is ever run. The copy is a directory of its own under `<data_dir>/replays`, filled with
//! the files of the run's worktree if that still exists (never its `.git`, and symbolic links are left out), or else
//! with a checkout of the card's change branch, or of the repository's base. The file tools are applied by Moebius
//! itself; `bash`, `grep` and `find` run with an empty environment (`PATH` only, `HOME` the copy), no input, and a time
//! limit after which the process group is killed. Moebius lists the copy's files with their sizes and hashes before and
//! after, stores what the call printed and which files it created, changed and deleted, and removes the copy. The
//! original worktree and repository are never touched.
//!
//! The outcome is one sentence, `replayed: <how it ended>; <what happened to the files>`:
//!
//! - how it ended: `exit <code>`, `timed out` or `ended by a signal`;
//! - the files: `created N files`, `changed N files` and `deleted N files`, in that order, those that are not zero,
//!   joined by `, ` (`1 file` for one), or `no file changed`.
//!
//! The sentence replaces an outcome the judgement had (a card that is done gives each of its judgements the card's outcome,
//! which the card still tells): for a refusal, what the call would have done says more. A replay that could not be made
//! reads `replayed: not run; <why>` and is stored all the same, so that it is not tried again. The schedule asks [`on_schedule`] once a minute; inside the trial window, when no proposal is being tried, it
//! replays the oldest calls on a thread of its own (one at a time), at most [`PER_NIGHT`] per night. `POST /v1/judgements/{id}/replay` replays one at any hour.
//!
//! This module holds the entry points, the one replay a person asks for and the schedule's; `call` reads the refused
//! call from its judgement, `copy` makes the throwaway copy and lists its files, `process` runs the call in it, and
//! `outcome` makes a replay, its sentence and its record.

mod call;
mod copy;
mod outcome;
mod process;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::app::{App, AppError, Result};
use crate::bus::Event;
use crate::config::TimeOfDay;
use crate::improve::trial::in_window;
use crate::store::{Judgement, Replay, now};

use call::call_of;
use outcome::replay_judgement;

pub use copy::{Changes, Listing, compare, listing};
pub(crate) use process::run_apart;

/// How many calls the schedule replays in one night.
pub const PER_NIGHT: usize = 20;

/// How long a replayed command may run before it is killed.
pub const TIME_LIMIT: Duration = Duration::from_secs(60);

/// How old a refusal may be and still be replayed by the schedule (in seconds); older ones are left alone, so that the
/// calls that can never be replayed do not have to be looked at every minute for ever.
const MAX_AGE: i64 = 7 * 24 * 60 * 60;

/// The most replays one request returns.
pub const MAX_LISTED: i64 = 1000;

/// Replays the call of the judgement `id` now, whatever the hour. It fails with `Invalid` when the judgement is not a
/// refused tool call or its call is not reversible, and with `Conflict` when it was replayed already.
pub fn replay(app: &App, id: i64, limit: Duration) -> Result<Replay> {
    let judgement = app
        .store
        .judgement(id)?
        .ok_or_else(|| AppError::NotFound(format!("judgement {id}")))?;
    call_of(app, &judgement)?;
    if !app.store.replays(Some(id), None, 1)?.is_empty() {
        return Err(AppError::Conflict(format!(
            "judgement {id} was replayed already"
        )));
    }
    replay_judgement(app, &judgement, limit)
}

/// The refused calls the schedule would replay, at most `count`: the oldest of the last week that have no replay and can
/// be replayed.
pub fn due(app: &App, count: usize) -> Result<Vec<Judgement>> {
    let mut due = Vec::new();
    for judgement in app.store.replay_candidates(now() - MAX_AGE)? {
        if due.len() == count {
            break;
        }
        if call_of(app, &judgement).is_ok() {
            due.push(judgement);
        }
    }
    Ok(due)
}

/// Replays up to `count` due calls, one after the other, and stops when a proposal is being tried. A trial does not
/// start while replays go ([`crate::improve::trial::on_schedule`]), so a replay never runs beside a trial. Returns how many were replayed.
pub fn run_due(app: &App, count: usize, limit: Duration) -> Result<usize> {
    let mut replayed = 0;
    for judgement in due(app, count)? {
        if app
            .store
            .proposals(None)?
            .iter()
            .any(|p| p.status == "trying")
        {
            break;
        }
        replay_judgement(app, &judgement, limit)?;
        replayed += 1;
    }
    Ok(replayed)
}

/// Clears the flag that replays are going when dropped, also when the thread panics.
struct Replaying<'a>(&'a AtomicBool);

impl Drop for Replaying<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Replays up to `count` due calls on a thread of its own, so that the schedule goes on meanwhile. Returns `None` when
/// replays are going already. A failure is published as `retro_error`, as the schedule's other work reports it.
pub fn start_due(
    app: &Arc<App>,
    count: usize,
    limit: Duration,
) -> Option<std::thread::JoinHandle<()>> {
    if app.replaying.swap(true, Ordering::SeqCst) {
        return None;
    }
    let thread_app = app.clone();
    let started = std::thread::Builder::new()
        .name("replays".into())
        .spawn(move || {
            let _going = Replaying(&thread_app.replaying);
            if let Err(e) = run_due(&thread_app, count, limit) {
                thread_app.bus.publish(Event::RetroError {
                    message: e.to_string(),
                });
            }
        });
    match started {
        Ok(handle) => Some(handle),
        Err(_) => {
            app.replaying.store(false, Ordering::SeqCst);
            None
        }
    }
}

/// The schedule's check, once a minute: inside the trial window, and while the night's [`PER_NIGHT`] are not used up,
/// starts replaying the due calls on a thread of their own, unless replays are going already.
pub fn on_schedule(app: &Arc<App>, window: (TimeOfDay, TimeOfDay)) -> Result<()> {
    let minutes = app.store.minutes_into_the_day()?;
    if !in_window(TimeOfDay(minutes), window) {
        return Ok(());
    }
    // The night began this many minutes ago (a minute more, as the clock is read twice).
    let elapsed = i64::from((minutes + 24 * 60 - window.0.0) % (24 * 60));
    let done = app.store.replays_since(now() - elapsed * 60 - 60)?;
    start_due(app, PER_NIGHT.saturating_sub(done), TIME_LIMIT);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use moebius_plugin_api::Host;

    #[test]
    fn replays_go_on_a_thread_of_their_own_and_never_two_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(1)));
        let app = Arc::new(App::new(store, host, &dir.path().join("data")));
        app.replaying.store(true, Ordering::SeqCst);
        assert!(start_due(&app, 5, TIME_LIMIT).is_none());
        assert!(
            app.replaying.load(Ordering::SeqCst),
            "the other's flag stays"
        );
        app.replaying.store(false, Ordering::SeqCst);
        let handle = start_due(&app, 5, TIME_LIMIT).expect("starts");
        handle.join().unwrap();
        assert!(!app.replaying.load(Ordering::SeqCst));
        assert!(start_due(&app, 5, TIME_LIMIT).is_some());
    }
}
