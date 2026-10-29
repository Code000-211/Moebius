//! The loops that keep the server going: the board's tick, which runs every card's column script, the polls of GitHub
//! and GitLab, and, every 30 seconds, the refresh of the clones of repositories without a forge and of the harness configuration
//! repository. `serve` runs them, and the schedule (`schedule`), side by side until it stops.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::time::{Interval, MissedTickBehavior};

use super::Server;
use crate::app::columns;
use crate::bus::AppEvent;
use crate::repos::poll;

/// How often the clones of repositories without a forge are brought up to date with where they were cloned from.
const PLAIN_REFRESH: Duration = Duration::from_secs(30);

impl Server {
    /// Runs every card's column script once; a script that fails is reported on the bus and the card stays put.
    pub fn tick(&self) {
        let Ok(repositories) = self.app.store.repositories() else {
            return;
        };
        for repository in repositories {
            let Ok(cards) = self.app.store.cards(repository.id) else {
                continue;
            };
            for card in cards {
                if card.column == columns::DONE
                    && self
                        .app
                        .store
                        .judgements_for_card(card.id)
                        .is_ok_and(|js| js.iter().all(|j| j.outcome.is_some()))
                {
                    continue;
                }
                if let Err(e) = self.engine.settle(card.id) {
                    // Kept as the card's waiting reason until a script runs cleanly again.
                    let reason = format!("script error: {e}");
                    self.app.set_waiting(card.id, Some(reason));
                    self.app.bus.publish(AppEvent::new(
                        "board_error",
                        json!({ "card": card.id, "message": e.to_string() }),
                    ));
                }
            }
        }
    }

    /// Polls every GitHub repository once (see `poll`).
    pub async fn poll_github(&self) {
        if let Some(github) = &self.github {
            poll::poll(&self.app, &**github).await;
        }
    }

    /// Polls every GitLab repository once (see `poll`).
    pub async fn poll_gitlab(&self) {
        if let Some(gitlab) = &self.gitlab {
            poll::poll(&self.app, &**gitlab).await;
        }
    }

    /// Refreshes the clone of every repository without a forge from where it was cloned; a failure is reported on the
    /// bus. The repositories on a forge are refreshed by its poll.
    pub fn refresh_plain_repositories(&self) {
        let Ok(repositories) = self.app.store.repositories() else {
            return;
        };
        for repository in repositories.iter().filter(|r| r.forge == "none") {
            if let Err(e) = self.app.refresh(repository) {
                self.app.bus.publish(AppEvent::new(
                    "forge_error",
                    json!({ "repository": repository.name, "message": e.to_string() }),
                ));
            }
        }
    }

    /// Refreshes the harness configuration repository, if there is one; a failure is reported on the bus.
    pub fn refresh_harness_repository(&self) {
        if let Some(harness_repo) = &self.harness_repo
            && let Err(e) = harness_repo.refresh(&self.app)
        {
            self.app.bus.publish(AppEvent::new(
                "harness_error",
                json!({ "message": e.to_string() }),
            ));
        }
    }
}

/// Runs every card's column script on the board's tick until the future is dropped. The tick is read each time, since
/// the settings can change it.
pub(super) async fn board(server: Arc<Server>) {
    loop {
        let started = Instant::now();
        let s = server.clone();
        let _ = tokio::task::spawn_blocking(move || s.tick()).await;
        let tick = server.live.get().board_tick;
        tokio::time::sleep(tick.saturating_sub(started.elapsed())).await;
    }
}

/// Polls GitHub as often as `github.poll_seconds` says, and GitLab as often as `gitlab.poll_seconds` says, until the
/// future is dropped; a forge that is not set up, never.
pub(super) async fn poll(server: Arc<Server>) {
    let github = server.config.github.as_ref().map(|g| g.poll);
    let gitlab = server.config.gitlab.as_ref().map(|g| g.poll);
    tokio::join!(
        poll_every(server.clone(), github, |s| Box::pin(s.poll_github())),
        poll_every(server.clone(), gitlab, |s| Box::pin(s.poll_gitlab())),
    );
}

/// Does `work` every `period` until the future is dropped; without a period, never.
async fn poll_every(
    server: Arc<Server>,
    period: Option<Duration>,
    work: for<'a> fn(&'a Server) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
) {
    let Some(period) = period else {
        return std::future::pending().await;
    };
    let mut interval = interval(period);
    loop {
        interval.tick().await;
        work(&server).await;
    }
}

/// Refreshes the clones of repositories without a forge and of the harness configuration repository every
/// [`PLAIN_REFRESH`] until the future is dropped.
pub(super) async fn refresh(server: Arc<Server>) {
    every(server, PLAIN_REFRESH, |server| {
        server.refresh_plain_repositories();
        server.refresh_harness_repository();
    })
    .await
}

/// Does `work` every `period` on a thread that may block, one at a time, until the future is dropped.
pub(super) async fn every(server: Arc<Server>, period: Duration, work: fn(&Server)) {
    let mut interval = interval(period);
    loop {
        interval.tick().await;
        let s = server.clone();
        let _ = tokio::task::spawn_blocking(move || work(&s)).await;
    }
}

/// An interval of `period` whose missed ticks are delayed, not made up for.
fn interval(period: Duration) -> Interval {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    interval
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use crate::config::Config;
    use crate::repos::forge::{Forge, GitHubForge};
    use crate::repos::git;
    use crate::server::Server;
    use crate::server::fake_github::{Fixture, check, issue};

    #[tokio::test]
    async fn a_poll_brings_the_clone_up_to_date_so_new_work_starts_from_the_merged_default_branch()
    {
        let f = Fixture::new().await;
        f.fake.set_issues("\"v1\"", vec![]);
        // A pull request merged on GitHub: the default branch moves on without the clone.
        let elsewhere = f._dir.path().join("elsewhere");
        git(
            f._dir.path(),
            &["clone", "--quiet", f.remote.to_str().unwrap(), "elsewhere"],
        )
        .unwrap();
        git(
            &elsewhere,
            &[
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "merged on GitHub",
            ],
        )
        .unwrap();
        git(&elsewhere, &["push", "--quiet", "origin", "main"]).unwrap();
        let merged = git(&elsewhere, &["rev-parse", "HEAD"]).unwrap();

        f.server.poll_github().await;
        let clone = Path::new(&f.repository.path);
        assert_eq!(f.repository.base_ref(), "origin/main");
        assert_eq!(git(clone, &["rev-parse", "origin/main"]).unwrap(), merged);
    }

    #[tokio::test]
    async fn issues_become_cards_and_an_unchanged_list_changes_nothing() {
        let f = Fixture::new().await;
        f.fake.set_issues(
            "\"v1\"",
            vec![
                issue(1, "open", "leon", "Do it."),
                issue(2, "open", "mallory", "Do it my way."),
                issue(3, "closed", "leon", "Long done."),
            ],
        );
        f.server.poll_github().await;

        assert!(f.card(3).is_none(), "a closed issue gets no card");
        let (trusted, outside) = (f.card(1).unwrap(), f.card(2).unwrap());
        assert_eq!(
            (
                trusted.title.as_str(),
                trusted.body.as_str(),
                trusted.author.as_str()
            ),
            ("Issue 1", "Do it.", "leon")
        );
        f.engine.settle(trusted.id).unwrap();
        f.engine.settle(outside.id).unwrap();
        let app = &f.server.app;
        assert_eq!(app.card(trusted.id).unwrap().column, "In progress");
        assert_eq!(app.card(outside.id).unwrap().column, "Backlog");
        assert!(
            app.store
                .has_open_inbox_item(outside.id, "approve_issue")
                .unwrap()
        );

        let mut events = app.bus.subscribe();
        f.server.poll_github().await;
        assert_eq!(f.fake.asked(), [None, Some("\"v1\"".to_string())]);
        assert!(events.try_recv().is_err(), "a 304 changes nothing");
        assert_eq!(app.store.cards(f.repository.id).unwrap().len(), 2);
        assert_eq!(app.card(trusted.id).unwrap().column, "In progress");
    }

    #[tokio::test]
    async fn a_closed_issue_moves_its_card_to_done_and_a_new_title_is_copied() {
        let f = Fixture::new().await;
        f.fake.set_issues(
            "\"v1\"",
            vec![
                issue(1, "open", "leon", ""),
                issue(2, "open", "mallory", ""),
            ],
        );
        f.server.poll_github().await;
        let id = f.card(1).unwrap().id;
        f.engine.settle(id).unwrap();

        let mut retitled = issue(2, "open", "mallory", "");
        retitled["title"] = json!("Better title");
        f.fake
            .set_issues("\"v2\"", vec![issue(1, "closed", "leon", ""), retitled]);
        f.server.poll_github().await;

        assert_eq!(f.fake.asked().last().unwrap().as_deref(), Some("\"v1\""));
        assert_eq!(f.card(1).unwrap().column, "Done");
        let outside = f.card(2).unwrap();
        assert_eq!(
            (outside.title.as_str(), outside.column.as_str()),
            ("Better title", "Backlog")
        );
    }

    #[tokio::test]
    async fn blocked_by_in_a_body_becomes_a_dependency() {
        let f = Fixture::new().await;
        f.fake.set_issues(
            "\"v1\"",
            vec![
                issue(5, "open", "leon", "Blocked by: #3, #4"),
                issue(6, "open", "leon", "Nothing in the way."),
            ],
        );
        f.server.poll_github().await;
        let store = &f.server.app.store;
        assert_eq!(store.dependencies(f.card(5).unwrap().id).unwrap(), [3, 4]);
        assert!(
            store
                .dependencies(f.card(6).unwrap().id)
                .unwrap()
                .is_empty()
        );

        f.fake
            .set_issues("\"v2\"", vec![issue(5, "open", "leon", "blocked by #4")]);
        f.server.poll_github().await;
        assert_eq!(store.dependencies(f.card(5).unwrap().id).unwrap(), [4]);
    }

    #[tokio::test]
    async fn a_repository_that_cannot_be_polled_is_reported_and_the_others_still_are() {
        let f = Fixture::new().await;
        let app = &f.server.app;
        let clone = &f.repository.path;
        app.store
            .add_github_repository("missing", clone, "main", "acme/missing", &[])
            .unwrap();
        f.fake
            .set_issues("\"v1\"", vec![issue(1, "open", "leon", "")]);
        let mut events = app.bus.subscribe();
        f.server.poll_github().await;

        assert!(f.card(1).is_some());
        assert_eq!(events.try_recv().unwrap().kind, "card_added");
        let error = events.try_recv().unwrap();
        assert_eq!(error.kind, "forge_error");
        assert_eq!(error.data["repository"], "acme/missing");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_offered_change_is_pushed_and_opened_as_a_pull_request() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;

        assert!(git(&f.remote, &["rev-parse", "--verify", "moebius/card-1"]).is_ok());
        assert_eq!(change.pull_request, Some(7));
        assert_eq!(f.change(change.id).pull_request, Some(7));
        let opened = f.fake.pulls.lock().unwrap().opened.clone();
        assert_eq!(opened.len(), 1);
        assert_eq!(
            (
                opened[0]["title"].as_str(),
                opened[0]["head"].as_str(),
                opened[0]["base"].as_str()
            ),
            (Some("Issue 1"), Some("moebius/card-1"), Some("main"))
        );
        assert!(
            opened[0]["body"]
                .as_str()
                .unwrap()
                .starts_with("Closes #1\n")
        );
        assert_eq!(f.card(1).unwrap().column, "Ready to merge");
        assert!(
            f.server
                .app
                .store
                .has_open_inbox_item(f.card(1).unwrap().id, "ready_to_merge")
                .unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pull_request_merged_on_github_moves_its_card_to_done() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;

        f.fake.set_pull("closed", true);
        f.server.poll_github().await;

        let change = f.change(change.id);
        assert_eq!(change.state, "merged");
        assert_eq!(f.card(1).unwrap().column, "Done");
        assert!(f.server.app.store.inbox().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pull_request_closed_without_a_merge_closes_the_change_and_tells_the_person_once() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;

        f.fake.set_pull("closed", false);
        f.server.poll_github().await;
        f.server.poll_github().await;

        assert_eq!(f.change(change.id).state, "closed");
        let inbox = f.server.app.store.inbox().unwrap();
        assert_eq!(
            inbox.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
            ["change_closed"]
        );
        assert_eq!(f.card(1).unwrap().column, "Ready to merge");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_repository_without_a_forge_gets_the_commits_of_where_it_was_cloned() {
        let f = Fixture::new().await;
        let origin = f._dir.path().join("origin");
        let plain = f
            .server
            .app
            .add_repository("plain", origin.to_str().unwrap())
            .unwrap();
        let clone = Path::new(&plain.path);
        let mut events = f.server.app.bus.subscribe();

        std::fs::write(origin.join("rules.md"), "new\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "rules"]).unwrap();
        f.server.refresh_plain_repositories();
        assert_eq!(
            std::fs::read_to_string(clone.join("rules.md")).unwrap(),
            "new\n"
        );

        // Moebius merged a change into the clone: the default branch is left, and nothing is wrong.
        git(clone, &["checkout", "--quiet", "-b", "feature"]).unwrap();
        git(
            clone,
            &["commit", "--quiet", "--allow-empty", "-m", "feature"],
        )
        .unwrap();
        git(clone, &["checkout", "--quiet", "main"]).unwrap();
        f.server.app.repos.merge(clone, "feature", "merge").unwrap();
        git(
            &origin,
            &["commit", "--quiet", "--allow-empty", "-m", "more"],
        )
        .unwrap();
        f.server.refresh_plain_repositories();
        assert!(
            git(clone, &["log", "--format=%s"])
                .unwrap()
                .contains("merge")
        );
        assert!(events.try_recv().is_err(), "no error was published");

        // A clone whose origin is gone is reported.
        std::fs::remove_dir_all(&origin).unwrap();
        f.server.refresh_plain_repositories();
        let event = events.try_recv().unwrap();
        assert_eq!(
            (event.kind.as_str(), event.data["repository"].as_str()),
            ("forge_error", Some("plain"))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_script_error_is_the_cards_waiting_reason_until_the_script_runs_cleanly() {
        let f = Fixture::new().await;
        let origin = f._dir.path().join("origin");
        let plain = f
            .server
            .app
            .add_repository("plain", origin.to_str().unwrap())
            .unwrap();
        let card = f
            .server
            .app
            .create_issue(plain.id, "Anything", "", "person")
            .unwrap();
        let clone = Path::new(&plain.path);
        let override_with = |script: &str| {
            std::fs::create_dir_all(clone.join(".moebius")).unwrap();
            std::fs::write(clone.join(".moebius/backlog.luau"), script).unwrap();
            git(clone, &["add", "."]).unwrap();
            git(clone, &["commit", "--quiet", "-m", "override"]).unwrap();
        };
        let waiting = || f.server.app.waiting(std::slice::from_ref(&card));

        override_with("function on_card(card) error('boom') end");
        f.server.tick();
        let reason = waiting().remove(&card.id).unwrap();
        assert!(reason.starts_with("script error: backlog.luau"), "{reason}");
        assert!(reason.contains("boom"), "{reason}");

        override_with("function on_card(card) end");
        f.server.tick();
        assert!(waiting().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_retry_after_a_closed_pull_request_opens_a_new_one() {
        let f = Fixture::new().await;
        let closed = f.offered_change().await;
        f.fake.set_pull("closed", false);
        f.server.poll_github().await;

        let card = f.card(1).unwrap();
        let app = f.server.app.clone();
        let retried = tokio::task::spawn_blocking(move || {
            app.retry_card(&crate::decide::Actor::Person, card.id, "Try again.")
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(retried.column, "In progress");
        assert_eq!(
            retried.note.as_deref(),
            Some("The pull request was closed without a merge.")
        );
        assert!(f.server.app.store.inbox().unwrap().is_empty());

        f.code(&card, "second.txt").await;
        let app = f.server.app.clone();
        let change = tokio::task::spawn_blocking(move || app.publish_change(card.id).unwrap())
            .await
            .unwrap();
        assert_ne!(change.id, closed.id);
        assert_eq!(change.state, "open");
        assert_eq!(f.change(closed.id).state, "closed");
        assert_eq!(f.fake.pulls.lock().unwrap().opened.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn checks_and_the_latest_review_are_recorded_when_they_change() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;
        let run = |status: &str, conclusion: Option<&str>| {
            json!({ "name": "ci", "status": status, "conclusion": conclusion,
                    "details_url": "https://ci/1", "output": { "title": "3 tests failed" } })
        };

        {
            let mut pulls = f.fake.pulls.lock().unwrap();
            pulls.check_runs = vec![run("completed", Some("success")), run("in_progress", None)];
            pulls.reviews = vec![
                json!({ "state": "COMMENTED" }),
                json!({ "state": "APPROVED" }),
            ];
        }
        let mut events = f.server.app.bus.subscribe();
        f.server.poll_github().await;
        let stored = f.change(change.id);
        assert_eq!(
            (stored.checks.as_deref(), stored.review.as_deref()),
            (Some("pending"), Some("approved"))
        );
        assert_eq!(events.try_recv().unwrap().kind, "change_updated");

        f.fake.pulls.lock().unwrap().check_runs = vec![
            run("completed", Some("success")),
            run("completed", Some("failure")),
        ];
        f.server.poll_github().await;
        let stored = f.change(change.id);
        assert_eq!(stored.checks.as_deref(), Some("failure"));
        assert_eq!(stored.checks_head.as_deref(), Some("deadbeef"));
        assert_eq!(
            stored.failed_checks.as_deref(),
            Some("ci: failure, 3 tests failed (https://ci/1)")
        );
        assert_eq!(events.try_recv().unwrap().kind, "change_updated");

        f.server.poll_github().await;
        assert!(events.try_recv().is_err(), "nothing changed");
        assert_eq!(f.change(change.id).state, "open");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_person_merges_a_pull_request_from_the_app() {
        let f = Fixture::new().await;
        let change = f.offered_change().await;

        let app = f.server.app.clone();
        let commit = tokio::task::spawn_blocking(move || {
            app.merge_change(&crate::decide::Actor::Person, change.id, "looks good")
        })
        .await
        .unwrap()
        .unwrap();

        assert_eq!(commit, "c0ffee");
        let merged = f.fake.pulls.lock().unwrap().merged.clone();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["commit_title"], "Merge #1: Issue 1");
        assert_eq!(f.change(change.id).state, "merged");
        assert_eq!(f.card(1).unwrap().column, "Done");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn git_reaches_a_web_address_with_the_installation_token_and_a_local_one_without() {
        let f = Fixture::new().await;
        let (repos, github) = (f.server.app.repos.clone(), f.server.github.clone().unwrap());
        let (web, local) = tokio::task::spawn_blocking(move || {
            let forge = GitHubForge::new(github, repos, tokio::runtime::Handle::current());
            let web = forge.auth("https://github.com/acme/widgets.git", "acme/widgets");
            let local = forge.auth("/srv/widgets.git", "acme/widgets");
            (web.unwrap(), local.unwrap())
        })
        .await
        .unwrap();
        assert_eq!(
            web.as_deref(),
            Some("Authorization: Basic eC1hY2Nlc3MtdG9rZW46Z2hzX3Rva2Vu"),
            "x-access-token:ghs_token"
        );
        assert_eq!(local, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_waits_for_the_checks_of_its_head_and_goes_back_when_they_fail() {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;

        // The change is published and, with no checks recorded for it yet, waits for them.
        let step = f.settle(&card).await;
        assert_eq!(step.waiting.as_deref(), Some("waiting for the checks"));
        let change = f.server.app.store.open_change(card.id).unwrap().unwrap();
        assert_eq!(change.pull_request, Some(7));
        let head = change.head.clone().unwrap();

        // Checks of an older commit do not count.
        f.fake.set_head("older");
        f.fake.pulls.lock().unwrap().check_runs = vec![check("build", "completed", "success")];
        f.server.poll_github().await;
        let step = f.settle(&card).await;
        assert_eq!(step.waiting.as_deref(), Some("waiting for the checks"));

        // The head moves to the published commit and its checks fail: the card goes back, told which.
        f.fake.set_head(&head);
        f.fake.pulls.lock().unwrap().check_runs = vec![
            check("build", "completed", "success"),
            check("lint", "completed", "failure"),
        ];
        f.server.poll_github().await;
        f.settle(&card).await;
        let card = f.server.app.card(card.id).unwrap();
        assert_eq!(card.column, "In progress");
        let note = card.note.unwrap();
        assert!(
            note.starts_with("The checks failed:\nlint: failure"),
            "{note}"
        );
        assert!(!note.contains("build"), "{note}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_without_checks_goes_on_once_the_grace_period_is_over() {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;
        f.settle(&card).await;
        assert_eq!(f.card(1).unwrap().column, "In review");

        let store = &f.server.app.store;
        let change = store.open_change(card.id).unwrap().unwrap();
        let long_ago = crate::store::now() - 3 * 60;
        store
            .set_change_head(change.id, change.head.as_deref().unwrap(), long_ago)
            .unwrap();
        f.settle(&card).await;
        assert_eq!(f.card(1).unwrap().column, "Ready to merge");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_card_keeps_one_branch_and_one_pull_request_across_rounds() {
        let f = Fixture::new().await;
        let card = f.card_in_review().await;
        let app = f.server.app.clone();
        let publish = || {
            let app = app.clone();
            async move {
                tokio::task::spawn_blocking(move || app.publish_change(card.id).unwrap())
                    .await
                    .unwrap()
            }
        };
        let first = publish().await;
        f.code(&card, "second.txt").await;
        let second = publish().await;

        assert_eq!(f.fake.pulls.lock().unwrap().opened.len(), 1);
        assert_eq!(second.id, first.id);
        assert_eq!(second.pull_request, Some(7));
        assert_eq!(second.branch, "moebius/card-1");
        // The branch on GitHub moved forward from the first round's commit to the second's.
        let (first_head, second_head) = (first.head.unwrap(), second.head.unwrap());
        assert_ne!(first_head, second_head);
        assert_eq!(
            git(&f.remote, &["rev-parse", "moebius/card-1"]).unwrap(),
            second_head
        );
        assert!(
            git(
                &f.remote,
                &["merge-base", "--is-ancestor", &first_head, &second_head]
            )
            .is_ok()
        );

        // Checks recorded for the first commit say nothing about the second.
        let stored = f.change(first.id);
        app.store
            .set_change_status(stored.id, Some("success"), &first_head, None, None)
            .unwrap();
        assert_eq!(app.checks_of(&f.change(first.id)).unwrap(), "pending");
        app.store
            .set_change_status(stored.id, Some("success"), &second_head, None, None)
            .unwrap();
        assert_eq!(app.checks_of(&f.change(first.id)).unwrap(), "success");
    }

    #[tokio::test]
    async fn a_github_repository_cannot_offer_a_change_without_the_github_app() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::parse("", dir.path(), &dir.path().join("moebius.toml")).unwrap();
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let store = &server.app.store;
        let repository = store
            .add_github_repository("widgets", "/srv/widgets", "main", "acme/widgets", &[])
            .unwrap();
        let card = store
            .add_numbered_card(repository.id, 1, "Card", "", "leon", "In progress")
            .unwrap();
        let error = server.app.offer_change(card.id).unwrap_err();
        assert!(error.to_string().contains("forge github"), "{error}");
        assert!(store.changes(repository.id).unwrap().is_empty());
    }
}
