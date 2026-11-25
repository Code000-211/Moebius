//! The software factory (ADR 0031), against the fake GitHub: the daily digest and its clock, the stale handling of
//! issues taken and pull requests followed, and the backlog sync.

use std::sync::Arc;

use serde_json::{Value, json};

use super::fake_github::Fixture;
use super::taking_tests::{following, open_issue};
use crate::app::{App, BacklogEdit, DIGEST_ITEM};
use crate::config::TimeOfDay;
use crate::decide::Actor;
use crate::repos::git;
use crate::store::{InboxItem, now};

const DAY: i64 = 86_400;

/// Runs `work` on a thread that may block, as the server does for what talks to a forge.
async fn blocking<T: Send + 'static>(
    f: &Fixture,
    work: impl FnOnce(Arc<App>) -> T + Send + 'static,
) -> T {
    let app = f.server.app.clone();
    tokio::task::spawn_blocking(move || work(app))
        .await
        .unwrap()
}

impl Fixture {
    /// Polls the issues once `login` wrote `body` on issue `number`.
    async fn writes(&self, number: u64, login: &str, body: &str) {
        self.fake.comment(number, login, body);
        self.server.poll_github().await;
    }

    /// Two open issues with comment threads, polled, with commands on.
    async fn two_issues(&self) {
        self.fake
            .set_issues("\"v1\"", vec![open_issue(1, "One."), open_issue(2, "Two.")]);
        self.fake.threads.lock().unwrap().insert(1, Vec::new());
        self.fake.threads.lock().unwrap().insert(2, Vec::new());
        self.server.poll_github().await;
        self.server
            .app
            .store
            .set_repository_commands(self.repository.id, true, 3, true)
            .unwrap();
    }

    fn today(&self) -> String {
        self.server.app.store.today().unwrap()
    }

    /// Commits `files` to the default branch of the remote, as a push to the forge does.
    fn push_files(&self, files: &[(&str, Option<&str>)]) {
        let work = tempfile::tempdir().unwrap();
        let dir = work.path().join("work");
        git(
            work.path(),
            &["clone", "--quiet", self.remote.to_str().unwrap(), "work"],
        )
        .unwrap();
        for (path, text) in files {
            let target = dir.join(path);
            match text {
                Some(text) => {
                    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                    std::fs::write(&target, text).unwrap();
                }
                None => {
                    std::fs::remove_file(&target).unwrap();
                }
            }
        }
        git(&dir, &["add", "-A"]).unwrap();
        git(&dir, &["commit", "--quiet", "-m", "backlog"]).unwrap();
        git(&dir, &["push", "--quiet", "origin", "main"]).unwrap();
    }
}

fn digest_of(f: &Fixture) -> Vec<InboxItem> {
    f.server
        .app
        .store
        .inbox()
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == DIGEST_ITEM)
        .collect()
}

fn section<'a>(item: &'a InboxItem, name: &str) -> &'a Value {
    item.data["repositories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no section for {name} in {}", item.data))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_digest_tells_what_happened_per_repository() {
    let f = Fixture::new().await;
    f.two_issues().await;
    f.writes(1, "alice", "/take").await;
    f.writes(2, "bob", "/take").await;
    f.writes(2, "bob", "/release").await;
    let (store, repository) = (&f.server.app.store, f.repository.clone());
    let (one, two) = (f.card(1).unwrap(), f.card(2).unwrap());
    // One change merged with a measurement, one merge that was refused, one revert.
    let merged = store
        .add_change(repository.id, Some(one.id), "moebius/1", Some(9), "abc")
        .unwrap();
    store.set_change_state(merged.id, "merged").unwrap();
    store
        .note_activity(
            "measurement",
            Some(repository.id),
            Some(9),
            None,
            "",
            &json!({ "change": merged.id }),
        )
        .unwrap();
    store
        .note_activity(
            "merge_failed",
            Some(repository.id),
            Some(2),
            None,
            "the checks failed",
            &json!({ "title": "Two." }),
        )
        .unwrap();
    store
        .note_activity(
            "revert",
            Some(repository.id),
            Some(9),
            None,
            "it broke the build",
            &json!({}),
        )
        .unwrap();
    // Something waits for a person, and a card is held.
    store
        .add_inbox_item("merge_stopped", Some(two.id), "Two.", &json!({}))
        .unwrap();
    store
        .hold_card(two.id, "person", Some("waiting for design"))
        .unwrap();

    let day = f.today();
    let item = blocking(&f, move |app| app.make_digest(&day, Some(1000)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.kind, DIGEST_ITEM);
    assert_eq!(
        item.title,
        format!("Daily digest {}: 1 merged, 1 need you", f.today())
    );
    let widgets = section(&item, "widgets");
    assert_eq!(widgets["merged"][0]["who"], "moebius");
    assert_eq!(widgets["merged"][0]["measured"], true);
    assert_eq!(widgets["merged"][0]["title"], "Issue 1");
    assert_eq!(widgets["refused"][0]["detail"], "the checks failed");
    assert_eq!(widgets["reverts"][0]["detail"], "it broke the build");
    assert_eq!(widgets["taken"].as_array().unwrap().len(), 2);
    assert_eq!(widgets["taken"][0]["who"], "alice");
    assert_eq!(widgets["released"][0]["who"], "bob");
    assert_eq!(widgets["frozen"][0]["detail"], "waiting for design");
    assert_eq!(item.data["tokens"]["day_budget"], 1000);
    let needs = &item.data["needs_a_person"][0];
    assert_eq!(needs["reason"], "A merge stopped");
    assert_eq!(needs["items"][0]["number"], 2);
    assert!(item.data["text"].as_str().unwrap().contains("merged"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_day_has_one_digest_and_the_next_tells_only_what_came_after() {
    let f = Fixture::new().await;
    f.two_issues().await;
    f.writes(1, "alice", "/take").await;
    // Before `digest_at` nothing is made; from it on, one, however often the clock looks.
    blocking(&f, |app| app.on_digest_schedule(TimeOfDay(24 * 60), None))
        .await
        .unwrap();
    assert!(digest_of(&f).is_empty());
    for _ in 0..3 {
        blocking(&f, |app| app.on_digest_schedule(TimeOfDay(0), None))
            .await
            .unwrap();
    }
    assert_eq!(digest_of(&f).len(), 1);
    // A restart reads the same store: the day still has its digest.
    let day = f.today();
    assert!(f.server.app.store.digest_of(&day).unwrap().is_some());
    let again = blocking(&f, move |app| app.make_digest(&day, None))
        .await
        .unwrap();
    assert!(again.is_none());

    f.writes(2, "bob", "/take").await;
    let item = blocking(&f, |app| app.make_digest("2099-01-02", None))
        .await
        .unwrap()
        .unwrap();
    let taken = section(&item, "widgets")["taken"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(taken.len(), 1, "alice was told the day before: {taken:?}");
    assert_eq!(taken[0]["who"], "bob");
    // A person asking for it again replaces nothing and makes a second item.
    assert_eq!(digest_of(&f).len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_quiet_day_still_has_a_digest() {
    let f = Fixture::new().await;
    let day = f.today();
    let item = blocking(&f, move |app| app.make_digest(&day, None))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.data["repositories"], json!([]));
    assert!(item.title.ends_with("0 merged, 0 need you"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_issue_taken_for_long_with_no_pull_request_is_released() {
    let f = Fixture::new().await;
    f.two_issues().await;
    f.writes(1, "alice", "/take").await;
    let card = f.card(1).unwrap();
    let store = &f.server.app.store;
    // Too soon: nothing.
    store.set_assigned_since(card.id, now() - 13 * DAY).unwrap();
    let done = blocking(&f, |app| app.run_upkeep()).await.unwrap();
    assert_eq!(done.released, 0);
    assert!(f.fake.unassigned.lock().unwrap().is_empty());

    store.set_assigned_since(card.id, now() - 15 * DAY).unwrap();
    let done = blocking(&f, |app| app.run_upkeep()).await.unwrap();
    assert_eq!(done.released, 1);
    assert_eq!(
        *f.fake.unassigned.lock().unwrap(),
        vec![(1, "alice".to_string())]
    );
    let reply = f.fake.posted.lock().unwrap().last().cloned().unwrap();
    assert_eq!(reply.0, 1);
    assert!(
        reply
            .1
            .starts_with("Released: @alice took this issue 15 days ago"),
        "{}",
        reply.1
    );
    // It can be taken again.
    assert!(f.card(1).unwrap().taken_by.is_none());
    f.writes(1, "bob", "/take").await;
    assert_eq!(f.card(1).unwrap().taken_by.as_deref(), Some("bob"));
    // The digest tells it once, as the stale handling's, and not also as a release.
    let day = f.today();
    let item = blocking(&f, move |app| app.make_digest(&day, None))
        .await
        .unwrap()
        .unwrap();
    let widgets = section(&item, "widgets");
    assert!(
        widgets["upkeep"][0]["detail"]
            .as_str()
            .unwrap()
            .starts_with("released")
    );
    assert!(widgets["released"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_released_for_a_followed_pull_request_a_held_card_or_while_all_agents_are_paused()
 {
    let f = Fixture::new().await;
    let card = following(&f).await;
    let store = &f.server.app.store;
    store.set_assigned_since(card.id, now() - 40 * DAY).unwrap();
    // Alice has a pull request that is followed: she is working.
    let done = blocking(&f, |app| app.run_upkeep()).await.unwrap();
    assert_eq!(done.released, 0);

    // A second issue, taken long ago by bob with no pull request.
    let mut other = open_issue(2, "Two.");
    other["assignees"] = json!([{ "login": "bob" }]);
    let mut first = open_issue(1, "One.");
    first["assignees"] = json!([{ "login": "alice" }]);
    f.fake.set_issues("\"v2\"", vec![first, other]);
    f.server.poll_github().await;
    let two = f.card(2).unwrap();
    assert_eq!(two.taken_by.as_deref(), Some("bob"));
    store.set_assigned_since(two.id, now() - 40 * DAY).unwrap();
    store.hold_card(two.id, "person", Some("on hold")).unwrap();
    assert_eq!(
        blocking(&f, |app| app.run_upkeep()).await.unwrap().released,
        0
    );
    store.release_card(two.id).unwrap();
    store.pause_runs("person").unwrap();
    assert_eq!(
        blocking(&f, |app| app.run_upkeep()).await.unwrap().released,
        0
    );
    store.resume_runs().unwrap();
    assert_eq!(
        blocking(&f, |app| app.run_upkeep()).await.unwrap().released,
        1
    );
    assert_eq!(
        *f.fake.unassigned.lock().unwrap(),
        vec![(2, "bob".to_string())]
    );
}

/// The followed pull request of alice, which a review asked to change and nobody pushed to for `days`.
async fn stale_request(f: &Fixture, days: i64) -> crate::store::Change {
    let card = following(f).await;
    let store = &f.server.app.store;
    let change = store.open_change(card.id).unwrap().unwrap();
    store
        .set_change_status(change.id, None, "deadbeef", None, Some("changes_requested"))
        .unwrap();
    store
        .set_change_head(change.id, "deadbeef", now() - days * DAY)
        .unwrap();
    store.change(change.id).unwrap().unwrap()
}

fn posted_on(f: &Fixture, number: u64) -> Vec<String> {
    f.fake
        .posted
        .lock()
        .unwrap()
        .iter()
        .filter(|(n, _)| *n == number)
        .map(|(_, text)| text.clone())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_followed_pull_request_with_no_push_after_changes_were_asked_is_reminded_once() {
    let f = Fixture::new().await;
    stale_request(&f, 13).await;
    assert_eq!(
        blocking(&f, |app| app.run_upkeep()).await.unwrap().reminded,
        0
    );

    let change = stale_request_aged(&f, 15);
    let done = blocking(&f, |app| app.run_upkeep()).await.unwrap();
    assert_eq!((done.reminded, done.closed), (1, 0));
    let comments = posted_on(&f, 7);
    assert_eq!(comments.len(), 1);
    assert!(
        comments[0].starts_with("A reminder for @alice"),
        "{}",
        comments[0]
    );
    assert!(comments[0].contains("closed after 30 days"));
    // The same head is reminded once.
    assert_eq!(
        blocking(&f, |app| app.run_upkeep()).await.unwrap().reminded,
        0
    );
    assert_eq!(posted_on(&f, 7).len(), 1);
    assert_eq!(f.change(change.id).state, "open");
}

/// Ages the push of the followed request again.
fn stale_request_aged(f: &Fixture, days: i64) -> crate::store::Change {
    let store = &f.server.app.store;
    let card = f.card(1).unwrap();
    let change = store.open_change(card.id).unwrap().unwrap();
    store
        .set_change_head(change.id, "deadbeef", now() - days * DAY)
        .unwrap();
    change
}

#[tokio::test(flavor = "multi_thread")]
async fn a_followed_pull_request_stale_for_long_is_closed_politely_and_its_issue_released() {
    let f = Fixture::new().await;
    let change = stale_request(&f, 31).await;
    let done = blocking(&f, |app| app.run_upkeep()).await.unwrap();
    assert_eq!((done.reminded, done.closed), (0, 1));
    let comments = posted_on(&f, 7);
    assert!(
        comments[0].starts_with("Closing this pull request politely"),
        "{}",
        comments[0]
    );
    assert!(
        f.fake
            .state_writes
            .lock()
            .unwrap()
            .contains(&(7, "closed".to_string()))
    );
    assert_eq!(f.change(change.id).state, "closed");
    assert!(
        f.fake
            .unassigned
            .lock()
            .unwrap()
            .contains(&(1, "alice".to_string()))
    );
    assert!(f.card(1).unwrap().taken_by.is_none());
    // The digest has the action.
    let day = f.today();
    let item = blocking(&f, move |app| app.make_digest(&day, None))
        .await
        .unwrap()
        .unwrap();
    let upkeep = section(&item, "widgets")["upkeep"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        upkeep
            .iter()
            .any(|l| l["detail"].as_str().unwrap().starts_with("closed:"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_that_was_not_asked_to_change_or_is_held_is_left_alone() {
    let f = Fixture::new().await;
    let change = stale_request(&f, 40).await;
    let store = &f.server.app.store;
    // Approved, not changes requested.
    store
        .set_change_status(change.id, None, "deadbeef", None, Some("approved"))
        .unwrap();
    let done = blocking(&f, |app| app.run_upkeep()).await.unwrap();
    assert_eq!((done.reminded, done.closed), (0, 0));
    // Changes requested but the card is held.
    store
        .set_change_status(change.id, None, "deadbeef", None, Some("changes_requested"))
        .unwrap();
    store
        .hold_card(
            change.card_id.unwrap(),
            "person",
            Some("frozen for release"),
        )
        .unwrap();
    let done = blocking(&f, |app| app.run_upkeep()).await.unwrap();
    assert_eq!((done.reminded, done.closed), (0, 0));
    assert!(posted_on(&f, 7).is_empty());
    assert_eq!(f.change(change.id).state, "open");
    // The settings can turn each off: a repository that sets 0 days is never reminded.
    store.release_card(change.card_id.unwrap()).unwrap();
    store
        .set_upkeep(
            f.repository.id,
            &crate::store::Upkeep {
                release_after_days: 0,
                stale_after_days: 0,
                close_after_days: 0,
            },
        )
        .unwrap();
    let done = blocking(&f, |app| app.run_upkeep()).await.unwrap();
    assert_eq!((done.reminded, done.closed, done.released), (0, 0, 0));
}

const BACKLOG: &str = "# Backlog\n\n## K-47: Cache the plan\nDifficulty: hard\n\nCache it.\n\n## P-16: Shorter prompts\nLane required\n\nCut them.\n\n## M-01: Measure more\nDifficulty: easy\n\nMeasure.\n";

async fn with_backlog(f: &Fixture, cap: i64) -> i64 {
    f.push_files(&[("notes/backlog/optimisation-backlog.md", Some(BACKLOG))]);
    let project = f.repository.home_project_id;
    let edit = BacklogEdit {
        repository: Some("widgets".into()),
        path: Some("notes/backlog/optimisation-backlog.md".into()),
        cap: Some(cap),
        ..BacklogEdit::default()
    };
    blocking(f, move |app| {
        app.edit_backlog(&Actor::first_owner(), project, &edit, "")
    })
    .await
    .unwrap();
    project
}

async fn sync(f: &Fixture, project: i64, dry_run: bool) -> crate::app::SyncReport {
    blocking(f, move |app| {
        app.sync_backlog(&Actor::first_owner(), project, dry_run)
    })
    .await
    .unwrap()
}

fn titles(f: &Fixture) -> Vec<String> {
    f.fake
        .created
        .lock()
        .unwrap()
        .iter()
        .map(|(_, title, _)| title.clone())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sync_makes_an_issue_for_each_item_and_a_second_makes_none() {
    let f = Fixture::new().await;
    let project = with_backlog(&f, 15).await;
    let report = sync(&f, project, false).await;
    assert_eq!(report.items, 3);
    assert_eq!(report.created.len(), 3, "{report:?}");
    assert_eq!(
        titles(&f),
        [
            "K-47: Cache the plan",
            "P-16: Shorter prompts",
            "M-01: Measure more"
        ]
    );
    // The body quotes the item in full.
    let body = f.fake.created.lock().unwrap()[0].2.clone();
    assert!(
        body.starts_with("> ## K-47: Cache the plan\n> Difficulty: hard\n>\n> Cache it."),
        "{body}"
    );
    assert!(body.contains("notes/backlog/optimisation-backlog.md"));
    // Run again, also after the poll read the issues back: nothing is made twice.
    f.server.poll_github().await;
    let again = sync(&f, project, false).await;
    assert!(
        again.created.is_empty() && again.updated.is_empty(),
        "{again:?}"
    );
    assert_eq!(again.unchanged, 3);
    assert_eq!(f.fake.created.lock().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_writes_nothing() {
    let f = Fixture::new().await;
    let project = with_backlog(&f, 15).await;
    let report = sync(&f, project, true).await;
    assert!(report.dry_run);
    assert_eq!(report.created.len(), 3);
    assert!(f.fake.created.lock().unwrap().is_empty());
    assert!(f.card(100).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cap_limits_the_issues_one_sync_makes_and_the_next_makes_the_rest() {
    let f = Fixture::new().await;
    let project = with_backlog(&f, 2).await;
    let report = sync(&f, project, false).await;
    assert_eq!(report.created.len(), 2);
    assert_eq!(report.deferred, ["M-01"]);
    let next = sync(&f, project, false).await;
    assert_eq!(next.created.len(), 1);
    assert_eq!(next.created[0].id, "M-01");
    assert!(next.deferred.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_item_updates_the_body_of_its_issue() {
    let f = Fixture::new().await;
    let project = with_backlog(&f, 15).await;
    sync(&f, project, false).await;
    let changed = BACKLOG.replace("Cache it.", "Cache it, but measure first.");
    f.push_files(&[("notes/backlog/optimisation-backlog.md", Some(&changed))]);
    let report = sync(&f, project, false).await;
    assert_eq!(report.updated.len(), 1, "{report:?}");
    assert_eq!(report.updated[0].id, "K-47");
    let card = f.card(100).unwrap();
    assert!(card.body.contains("Cache it, but measure first."));
    assert_eq!(card.title, "K-47: Cache the plan");
    assert!(report.created.is_empty());
    // And once more: it is as it should be.
    assert_eq!(sync(&f, project, false).await.updated.len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_item_closes_its_issue_as_not_planned() {
    let f = Fixture::new().await;
    let project = with_backlog(&f, 15).await;
    sync(&f, project, false).await;
    // One item is marked closed in the text, another has a file under closed/.
    let marked = BACKLOG.replace(
        "## P-16: Shorter prompts",
        "## P-16: Shorter prompts [closed]",
    );
    f.push_files(&[
        ("notes/backlog/optimisation-backlog.md", Some(&marked)),
        ("notes/backlog/closed/M-01.md", Some("Done in #5.\n")),
    ]);
    let report = sync(&f, project, false).await;
    let closed: Vec<&str> = report.closed.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(closed, ["P-16", "M-01"], "{report:?}");
    let reasons = f.fake.state_reasons.lock().unwrap().clone();
    assert!(
        reasons.contains(&(101, "not_planned".to_string())),
        "{reasons:?}"
    );
    assert!(
        reasons.contains(&(102, "not_planned".to_string())),
        "{reasons:?}"
    );
    assert!(f.card(101).unwrap().closed);
    assert!(!f.card(100).unwrap().closed);
    // A closed item with no issue makes none, and a second sync closes nothing again.
    assert!(sync(&f, project, false).await.closed.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_item_that_has_no_issue_makes_none() {
    let f = Fixture::new().await;
    f.push_files(&[(
        "notes/backlog/optimisation-backlog.md",
        Some("## K-1: Gone [closed]\ntext\n\n## K-2: Here\ntext\n"),
    )]);
    let project = f.repository.home_project_id;
    let edit = BacklogEdit {
        repository: Some("widgets".into()),
        path: Some("notes/backlog/optimisation-backlog.md".into()),
        ..BacklogEdit::default()
    };
    blocking(&f, move |app| {
        app.edit_backlog(&Actor::first_owner(), project, &edit, "")
    })
    .await
    .unwrap();
    let report = sync(&f, project, false).await;
    assert_eq!(titles(&f), ["K-2: Here"]);
    assert_eq!(report.created.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn difficulty_and_lane_become_labels_with_the_configured_names() {
    let f = Fixture::new().await;
    let project = with_backlog(&f, 15).await;
    sync(&f, project, false).await;
    let writes = f.fake.label_writes.lock().unwrap().clone();
    assert!(
        writes.contains(&(100, vec!["difficulty:hard".to_string()])),
        "{writes:?}"
    );
    assert!(
        writes.contains(&(101, vec!["lane-required".to_string()])),
        "{writes:?}"
    );
    assert!(
        writes.contains(&(102, vec!["difficulty:easy".to_string()])),
        "{writes:?}"
    );
    // The names are the project's to choose, and an issue made earlier gets the new ones.
    let edit = BacklogEdit {
        difficulty_label: Some("effort/".into()),
        lane_label: Some("needs-lane".into()),
        ..BacklogEdit::default()
    };
    blocking(&f, move |app| {
        app.edit_backlog(&Actor::first_owner(), project, &edit, "")
    })
    .await
    .unwrap();
    let report = sync(&f, project, false).await;
    assert_eq!(report.labelled.len(), 3, "{report:?}");
    let writes = f.fake.label_writes.lock().unwrap().clone();
    let last = writes.iter().rev().find(|(n, _)| *n == 101).unwrap();
    assert!(last.1.contains(&"needs-lane".to_string()), "{last:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_backlog_needs_a_repository_of_the_project_and_a_pattern_with_an_id() {
    let f = Fixture::new().await;
    let project = f.repository.home_project_id;
    let owner = Actor::first_owner();
    let bad = |edit: BacklogEdit| {
        let owner = owner.clone();
        blocking(&f, move |app| app.edit_backlog(&owner, project, &edit, ""))
    };
    assert!(bad(BacklogEdit::default()).await.is_err());
    assert!(
        bad(BacklogEdit {
            repository: Some("other".into()),
            path: Some("b.md".into()),
            ..BacklogEdit::default()
        })
        .await
        .is_err()
    );
    let wrong = bad(BacklogEdit {
        repository: Some("widgets".into()),
        path: Some("b.md".into()),
        pattern: Some(r"^## (?P<name>.*)$".into()),
        ..BacklogEdit::default()
    })
    .await
    .unwrap_err();
    assert!(wrong.to_string().contains("group named id"), "{wrong}");
    let capped = bad(BacklogEdit {
        repository: Some("widgets".into()),
        path: Some("b.md".into()),
        cap: Some(0),
        ..BacklogEdit::default()
    })
    .await;
    assert!(capped.is_err());
    let sync = blocking(&f, move |app| {
        app.sync_backlog(&Actor::first_owner(), project, false)
    })
    .await;
    assert!(sync.is_err(), "no backlog is set");
}
