//! Which servers have a webview of their own in the window (ADR 0021, D2), and which of them to let go.
//!
//! Opening a server's web app takes a whole load (the page, its modules, the snapshot, the event stream), and the page's
//! state (the scroll, the filters, the open card) is lost with it. So the window keeps one webview per server that was
//! opened, shows the one the person chose and hides the others, and switching back is a matter of showing a webview that
//! is there. A webview costs memory, so the ones not looked at for a long time, or beyond a few, are dropped; the page
//! each was on is noted by the rail, which opens the server there again when it comes back.
//!
//! This holds the bookkeeping only (the program's `pages` module makes and shows the webviews), so that it is tested
//! without a window.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How many webviews of servers that are not shown are kept.
pub const KEPT_HIDDEN: usize = 3;

/// How long a webview of a server that is not shown is kept without being looked at.
pub const KEPT_FOR: Duration = Duration::from_secs(30 * 60);

/// The webviews of the window, by server id.
pub struct Views {
    kept: usize,
    idle: Duration,
    /// The origin each server's webview was made for, and when it was last shown or left.
    seen: HashMap<String, (String, Instant)>,
    shown: Option<String>,
    /// The servers whose webview has drawn its rail, so that it is worth showing.
    drawn: HashSet<String>,
    /// The webview that is visible, which is the one shown once it has drawn; none is the connect page.
    front: Option<String>,
}

impl Views {
    /// Keeps at most `kept` webviews of servers that are not shown, each for at most `idle` after it was left.
    pub fn new(kept: usize, idle: Duration) -> Self {
        Self {
            kept,
            idle,
            seen: HashMap::new(),
            shown: None,
            drawn: HashSet::new(),
            front: None,
        }
    }

    /// Whether the server `id` has a webview.
    pub fn has(&self, id: &str) -> bool {
        self.seen.contains_key(id)
    }

    /// The server whose webview is shown, none while the window shows the connect page.
    pub fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    /// The server whose webview is visible: the one shown, or the one that was before while the one shown loads.
    pub fn front(&self) -> Option<&str> {
        self.front.as_deref()
    }

    /// Notes which webview is visible now, none for the connect page.
    pub fn set_front(&mut self, id: Option<&str>) {
        self.front = id.map(str::to_string);
    }

    /// Notes that the webview of `id` has drawn its rail.
    pub fn mark_drawn(&mut self, id: &str) {
        if self.has(id) {
            self.drawn.insert(id.to_string());
        }
    }

    /// Whether the webview of `id` has drawn its rail.
    pub fn is_drawn(&self, id: &str) -> bool {
        self.drawn.contains(id)
    }

    /// Notes that the server `id`, whose web app is at `origin`, is shown now, with a webview that was there already
    /// or one the caller makes; says which. A webview made for another origin (the server's tunnel was built again, on
    /// another port) is no use, so the caller makes one. The one shown before is left at `now`.
    pub fn show(&mut self, id: &str, origin: &str, now: Instant) -> bool {
        let there = self
            .seen
            .get(id)
            .is_some_and(|(made_for, _)| made_for == origin);
        self.leave(now);
        if !there {
            self.drawn.remove(id);
        }
        self.seen.insert(id.to_string(), (origin.to_string(), now));
        self.shown = Some(id.to_string());
        there
    }

    fn leave(&mut self, now: Instant) {
        if let Some(before) = self.shown.take()
            && let Some((_, at)) = self.seen.get_mut(&before)
        {
            *at = now;
        }
    }

    /// Notes that the window shows the connect page, leaving the server that was shown at `now`.
    pub fn show_none(&mut self, now: Instant) {
        self.leave(now);
    }

    /// Forgets the webview of the server `id`, which is gone or could not be made.
    pub fn forget(&mut self, id: &str) {
        self.seen.remove(id);
        self.drawn.remove(id);
        if self.shown.as_deref() == Some(id) {
            self.shown = None;
        }
        if self.front.as_deref() == Some(id) {
            self.front = None;
        }
    }

    /// The servers whose webviews to drop at `now`, which are forgotten here: those left more than the idle time ago,
    /// and the longest left of those beyond the number kept. The one shown is never among them.
    pub fn expired(&mut self, now: Instant) -> Vec<String> {
        let mut hidden: Vec<(String, Instant)> = self
            .seen
            .iter()
            .filter(|(id, _)| self.shown.as_deref() != Some(id.as_str()))
            .map(|(id, (_, at))| (id.clone(), *at))
            .collect();
        // Newest first, so that what is beyond the number kept is the longest left.
        hidden.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
        let dropped: Vec<String> = hidden
            .iter()
            .enumerate()
            .filter(|(position, (_, at))| {
                *position >= self.kept || now.saturating_duration_since(*at) > self.idle
            })
            .map(|(_, (id, _))| id.clone())
            .collect();
        for id in &dropped {
            self.seen.remove(id);
            self.drawn.remove(id);
        }
        dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> Instant {
        // One fixed start, so that the tests read as a timeline.
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *START.get_or_init(Instant::now) + Duration::from_secs(seconds)
    }

    #[test]
    fn a_server_opened_before_is_shown_from_its_webview_and_the_other_stays_alive() {
        let mut views = Views::new(3, Duration::from_secs(600));
        assert!(
            !views.show("a", "http://127.0.0.1:1", at(0)),
            "a first opening makes a webview"
        );
        assert!(!views.show("b", "http://127.0.0.1:2", at(1)));
        assert!(
            views.has("a") && views.has("b"),
            "b's opening kept a's webview"
        );
        assert!(
            views.show("a", "http://127.0.0.1:1", at(2)),
            "coming back finds a's webview"
        );
        assert_eq!(views.shown(), Some("a"));
    }

    #[test]
    fn a_webview_made_for_another_origin_is_made_again() {
        let mut views = Views::new(3, Duration::from_secs(600));
        views.show("a", "http://127.0.0.1:1", at(0));
        views.show("b", "http://127.0.0.1:2", at(1));
        // a's tunnel was built again, so its proxy listens on another port.
        assert!(!views.show("a", "http://127.0.0.1:9", at(2)));
        assert!(views.show("a", "http://127.0.0.1:9", at(3)));
    }

    #[test]
    fn a_webview_left_for_long_is_dropped_and_the_one_shown_never_is() {
        let mut views = Views::new(3, Duration::from_secs(600));
        views.show("a", "o", at(0));
        views.show("b", "o", at(10));
        assert!(views.expired(at(300)).is_empty());
        assert_eq!(views.expired(at(700)), vec!["a".to_string()]);
        assert!(
            !views.has("a"),
            "dropped, so it is made again when it comes back"
        );
        assert!(views.expired(at(100_000)).is_empty(), "b is shown");
        assert!(views.has("b"));
    }

    #[test]
    fn only_a_few_hidden_webviews_are_kept_and_the_longest_left_go_first() {
        let mut views = Views::new(2, Duration::from_secs(10_000));
        for (n, id) in ["a", "b", "c", "d"].into_iter().enumerate() {
            views.show(id, "o", at(n as u64));
        }
        // d is shown; a, b and c are hidden, left at 1, 2 and 3.
        assert_eq!(views.expired(at(10)), vec!["a".to_string()]);
        assert!(views.has("b") && views.has("c") && views.has("d"));
    }

    #[test]
    fn a_webview_is_worth_showing_once_it_has_drawn_and_not_again_after_it_is_gone() {
        let mut views = Views::new(3, Duration::from_secs(600));
        views.show("a", "o", at(0));
        assert!(!views.is_drawn("a"));
        views.mark_drawn("a");
        assert!(views.is_drawn("a"));
        views.set_front(Some("a"));
        views.forget("a");
        assert!(!views.is_drawn("a"));
        assert_eq!(views.front(), None, "the connect page is in front again");
        // A webview made again for another origin has not drawn.
        views.show("b", "o", at(1));
        views.mark_drawn("b");
        views.show("b", "other", at(2));
        assert!(!views.is_drawn("b"));
    }

    #[test]
    fn showing_the_connect_page_leaves_the_server_and_a_forgotten_one_is_made_again() {
        let mut views = Views::new(3, Duration::from_secs(600));
        views.show("a", "o", at(0));
        views.show_none(at(5));
        assert_eq!(views.shown(), None);
        assert!(views.has("a"));
        views.forget("a");
        assert!(!views.show("a", "o", at(6)));
    }
}
