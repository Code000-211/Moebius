//! What the engine remembers between script calls so that a tick of an idle board costs next to nothing:
//!
//! - [`Compiled`]: the Luau bytecode of each script text, so a script is compiled once and not for every card.
//! - [`Quiet`]: the cards whose last settle changed nothing, with the state of the store when it ran, so that the tick
//!   leaves them alone until something was written or [`RECHECK`] has passed.
//! - [`ScriptMemo`]: the text of the project scripts read during one tick, so that the harness configuration
//!   repository's files are read once per project and column and not once per card.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use mlua::Compiler;

/// How long a card whose script found nothing to do is left alone while nothing is written to the store. Scripts may
/// read the clock, the harness configuration repository or the forge's checks, none of which write to the store when
/// they change, so a quiet card has its script run again after this long.
pub const RECHECK: Duration = Duration::from_secs(60);

/// The most script texts whose bytecode is kept; a project that edits its scripts all day cannot make it grow.
const MOST_SCRIPTS: usize = 256;

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The bytecode of the script texts that were compiled, by text.
#[derive(Default)]
pub(super) struct Compiled {
    by_text: Mutex<HashMap<String, Arc<Vec<u8>>>>,
}

impl Compiled {
    /// The bytecode of `source`; none when it does not compile, for the caller to load the text and report why.
    pub(super) fn bytecode(&self, source: &str) -> Option<Arc<Vec<u8>>> {
        if let Some(found) = locked(&self.by_text).get(source) {
            return Some(found.clone());
        }
        let code = Arc::new(Compiler::new().compile(source).ok()?);
        let mut by_text = locked(&self.by_text);
        if by_text.len() >= MOST_SCRIPTS {
            by_text.clear();
        }
        by_text.insert(source.to_string(), code.clone());
        Some(code)
    }

    /// How many script texts are kept.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        locked(&self.by_text).len()
    }
}

/// What the store and the limits were when a card was settled, and when.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stamp {
    /// The rows the store has changed since it was opened.
    pub writes: u64,
    /// A digest of the limits scripts see.
    pub limits: u64,
}

/// The cards that were settled and did nothing, by id.
#[derive(Default)]
pub(super) struct Quiet {
    cards: Mutex<HashMap<i64, (Stamp, Instant)>>,
}

impl Quiet {
    /// Remembers that settling `card` with the state `stamp` did nothing.
    pub(super) fn remember(&self, card: i64, stamp: Stamp) {
        locked(&self.cards).insert(card, (stamp, Instant::now()));
    }

    /// Forgets `card`: its next tick runs its script.
    pub(super) fn forget(&self, card: i64) {
        locked(&self.cards).remove(&card);
    }

    /// Whether `card` was settled with nothing to do, in the state `stamp` is, less than `recheck` ago.
    pub(super) fn is_quiet(&self, card: i64, stamp: Stamp, recheck: Duration) -> bool {
        locked(&self.cards)
            .get(&card)
            .is_some_and(|(then, at)| *then == stamp && at.elapsed() < recheck)
    }

    /// Forgets the cards that are not in `alive`, so that the deleted ones do not stay.
    pub(super) fn keep_only(&self, alive: &std::collections::HashSet<i64>) {
        locked(&self.cards).retain(|id, _| alive.contains(id));
    }
}

/// The scripts read in one tick, by project and file name; none for a project that has no such file.
type Read = HashMap<(i64, String), Option<String>>;

/// The project scripts read during one tick, `Some` only while a tick runs.
#[derive(Default)]
pub(super) struct ScriptMemo {
    read: Mutex<Option<Read>>,
}

impl ScriptMemo {
    /// Starts a tick: scripts read from now on are kept until [`ScriptMemo::end`].
    pub(super) fn begin(&self) {
        *locked(&self.read) = Some(HashMap::new());
    }

    pub(super) fn end(&self) {
        *locked(&self.read) = None;
    }

    /// The script of `project` called `file`, read by `read` unless this tick has read it already.
    pub(super) fn get(
        &self,
        project: i64,
        file: &str,
        read: impl FnOnce() -> Option<String>,
    ) -> Option<String> {
        if let Some(found) = locked(&self.read)
            .as_ref()
            .and_then(|m| m.get(&(project, file.to_string())))
        {
            return found.clone();
        }
        let text = read();
        if let Some(memo) = locked(&self.read).as_mut() {
            memo.insert((project, file.to_string()), text.clone());
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAMP: Stamp = Stamp {
        writes: 3,
        limits: 7,
    };

    #[test]
    fn a_script_is_compiled_once_per_text() {
        let compiled = Compiled::default();
        let a = compiled.bytecode("function on_card(c) end").unwrap();
        let b = compiled.bytecode("function on_card(c) end").unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        compiled.bytecode("function on_card(c) return end").unwrap();
        assert_eq!(compiled.len(), 2);
        assert!(compiled.bytecode("function (").is_none());
        assert_eq!(compiled.len(), 2);
    }

    #[test]
    fn a_quiet_card_stays_quiet_only_while_the_state_is_the_same_and_the_recheck_is_not_due() {
        let quiet = Quiet::default();
        assert!(!quiet.is_quiet(1, STAMP, RECHECK));
        quiet.remember(1, STAMP);
        assert!(quiet.is_quiet(1, STAMP, RECHECK));
        assert!(!quiet.is_quiet(2, STAMP, RECHECK));
        let written = Stamp { writes: 4, ..STAMP };
        assert!(!quiet.is_quiet(1, written, RECHECK));
        let limited = Stamp { limits: 8, ..STAMP };
        assert!(!quiet.is_quiet(1, limited, RECHECK));
        assert!(!quiet.is_quiet(1, STAMP, Duration::ZERO));
        quiet.forget(1);
        assert!(!quiet.is_quiet(1, STAMP, RECHECK));
    }

    #[test]
    fn project_scripts_are_read_once_per_tick() {
        let memo = ScriptMemo::default();
        let mut reads = 0;
        let mut read = |text: Option<&str>| {
            memo.get(1, "code.luau", || {
                reads += 1;
                text.map(String::from)
            })
        };
        // Outside a tick nothing is kept.
        assert_eq!(read(Some("a")).as_deref(), Some("a"));
        assert_eq!(read(Some("b")).as_deref(), Some("b"));
        memo.begin();
        assert_eq!(read(Some("c")).as_deref(), Some("c"));
        assert_eq!(read(Some("d")).as_deref(), Some("c"));
        memo.end();
        assert_eq!(read(Some("e")).as_deref(), Some("e"));
        assert_eq!(reads, 4);
    }
}
