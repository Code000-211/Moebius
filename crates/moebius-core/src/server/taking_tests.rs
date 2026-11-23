//! Working with people on other servers through the forge (ADR 0027), against the fake GitHub: taking an issue assigns
//! the person's login, an issue someone else took is left alone, the ready label follows the blockers, a repository's
//! agents may take only the labelled issues, and a pull request someone else opened is followed to its merge or its
//! close.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::fake_github::{Fixture, check, issue};
use crate::app::AppError;
use crate::decide::Actor;
use crate::repos::git;
use crate::store::Card;

/// An issue of the fake that says nothing blocks it, so that the server may mark it ready.
fn open_issue(number: u64, body: &str) -> Value {
    let mut issue = issue(number, "open", "leon", body);
    issue["issue_dependencies_summary"] = json!({ "total_blocked_by": 0 });
    issue
}

impl Fixture {
    /// Says the owner's login on GitHub.
    fn owner_logs_in_as(&self, login: &str) {
        let owner = self
            .server
            .app
            .store
            .person_by_handle("owner")
            .unwrap()
            .unwrap();
        let logins = if login.is_empty() {
            BTreeMap::new()
        } else {
            BTreeMap::from([("github".to_string(), login.to_string())])
        };
        self.server
            .app
            .store
            .set_person_logins(owner.id, &logins)
            .unwrap();
    }

    /// Sets how the repository works with others.
    fn works_with_others(&self, mark_ready: bool, agents_take: &str) {
        self.server
            .app
            .store
            .set_repository_working_with_others(
                self.repository.id,
                mark_ready,
                "Ready for agent",
                agents_take,
            )
            .unwrap();
    }

    /// The owner claims the card, on a thread that may block.
    async fn claim(&self, card: &Card) -> Result<Card, AppError> {
        let (app, id) = (self.server.app.clone(), card.id);
        tokio::task::spawn_blocking(move || app.claim_issue(&Actor::first_owner(), id, None))
            .await
            .unwrap()
    }

    /// The card of issue `number`, in `column`, as the board would have moved it there.
    fn put_in(&self, number: i64, column: &str) -> Card {
        let card = self.card(number).unwrap();
        self.server.app.store.move_card(card.id, column).unwrap();
        self.card(number).unwrap()
    }

    fn runs_of(&self, number: i64) -> usize {
        let card = self.card(number).unwrap();
        self.server.app.store.runs_for_card(card.id).unwrap().len()
    }

    fn writes_of(&self, number: u64) -> Vec<Vec<String>> {
        self.fake
            .label_writes
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| *n == number)
            .map(|(_, labels)| labels.clone())
            .collect()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn taking_an_issue_assigns_the_persons_login_and_a_refusal_is_told_on_the_card() {
    let f = Fixture::new().await;
    f.fake.set_issues(
        "\"v1\"",
        vec![
            open_issue(1, "One."),
            open_issue(2, "Two."),
            open_issue(3, "Three."),
        ],
    );
    f.server.poll_github().await;

    // With a login, the claim assigns it on the forge, and the poll reads it back as the person's own.
    f.owner_logs_in_as("leon-gh");
    let claimed = f.claim(&f.card(1).unwrap()).await.unwrap();
    assert_eq!(claimed.claimed_by.as_deref(), Some("person:owner"));
    assert_eq!(
        *f.fake.assigned.lock().unwrap(),
        vec![(1, "leon-gh".to_string())]
    );
    assert_eq!(f.card(1).unwrap().assign_problem, None);
    f.server.poll_github().await;
    assert_eq!(
        f.card(1).unwrap().taken_by,
        None,
        "the person's own login is nobody else's"
    );

    // Without one, nothing is assigned, the claim stands, and the card says why.
    f.owner_logs_in_as("");
    let claimed = f.claim(&f.card(2).unwrap()).await.unwrap();
    assert_eq!(claimed.claimed_by.as_deref(), Some("person:owner"));
    assert_eq!(f.fake.assigned.lock().unwrap().len(), 1);
    let problem = f.card(2).unwrap().assign_problem.unwrap();
    assert!(problem.contains("Settings > You > Profile"), "{problem}");

    // A forge that will not assign the login does not fail the claim either.
    f.owner_logs_in_as("not-a-collaborator");
    f.fake
        .assignable
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let claimed = f.claim(&f.card(3).unwrap()).await.unwrap();
    assert_eq!(claimed.claimed_by.as_deref(), Some("person:owner"));
    let problem = f.card(3).unwrap().assign_problem.unwrap();
    assert!(problem.contains("could not be assigned"), "{problem}");
    assert!(problem.contains("@owner"), "{problem}");
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_a_run_for_a_person_assigns_them_too() {
    let f = Fixture::new().await;
    f.fake.set_issues("\"v1\"", vec![open_issue(1, "One.")]);
    f.server.poll_github().await;
    f.owner_logs_in_as("leon-gh");
    let card = f.put_in(1, "In progress");
    f.settle(&card).await;
    assert_eq!(f.runs_of(1), 1);
    assert_eq!(
        *f.fake.assigned.lock().unwrap(),
        vec![(1, "leon-gh".to_string())]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_issue_someone_else_took_is_never_started_and_cannot_be_claimed() {
    let f = Fixture::new().await;
    let mut taken = open_issue(1, "One.");
    taken["assignees"] = json!([{ "login": "alice" }]);
    f.fake.set_issues("\"v1\"", vec![taken]);
    f.server.poll_github().await;
    f.owner_logs_in_as("leon-gh");
    let card = f.put_in(1, "In progress");
    assert_eq!(card.taken_by.as_deref(), Some("alice"));

    // The board's script waits with the reason and starts nothing.
    let step = f.settle(&card).await;
    assert_eq!(step.waiting.as_deref(), Some("taken by @alice"));
    assert_eq!(f.runs_of(1), 0);

    // A claim is refused with the same reason, and assigns nobody.
    let refused = f.claim(&card).await.unwrap_err();
    assert!(matches!(refused, AppError::Conflict(_)), "{refused}");
    assert!(refused.to_string().contains("taken by @alice"), "{refused}");
    assert!(f.fake.assigned.lock().unwrap().is_empty());

    // Once the assignee is taken off on the forge, the card is this server's again.
    f.fake.edit_issue(1, |issue| issue["assignees"] = json!([]));
    f.server.poll_github().await;
    let card = f.card(1).unwrap();
    assert_eq!(card.taken_by, None);
    f.settle(&card).await;
    assert_eq!(f.runs_of(1), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ready_label_is_added_and_removed_as_blockers_come_and_go() {
    let f = Fixture::new().await;
    f.works_with_others(true, "all");
    let unsure = issue(3, "open", "leon", "The forge did not say what blocks it.");
    f.fake.set_issues(
        "\"v1\"",
        vec![open_issue(1, "One."), open_issue(2, "Two."), unsure],
    );
    f.server.poll_github().await;
    assert_eq!(f.writes_of(1), vec![vec!["Ready for agent".to_string()]]);
    assert_eq!(f.writes_of(2), vec![vec!["Ready for agent".to_string()]]);
    assert!(
        f.writes_of(3).is_empty(),
        "an issue it is unsure about stays without the label"
    );

    // A blocker appears: the label is taken off at the next poll.
    f.fake
        .edit_issue(1, |issue| issue["body"] = json!("One.\n\nBlocked by #2"));
    f.server.poll_github().await;
    assert_eq!(
        f.writes_of(1).last(),
        Some(&Vec::new()),
        "{:?}",
        f.writes_of(1)
    );
    assert!(f.card(1).unwrap().labels.is_empty());

    // The blocker is done: the label is back.
    f.fake
        .edit_issue(2, |issue| issue["state"] = json!("closed"));
    f.server.poll_github().await;
    assert_eq!(
        f.writes_of(1).last(),
        Some(&vec!["Ready for agent".to_string()]),
        "{:?}",
        f.writes_of(1)
    );

    // Off, the server leaves the labels alone.
    f.works_with_others(false, "all");
    let before = f.fake.label_writes.lock().unwrap().len();
    f.fake
        .edit_issue(1, |issue| issue["body"] = json!("One.\n\nBlocked by #3"));
    f.server.poll_github().await;
    assert_eq!(f.fake.label_writes.lock().unwrap().len(), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_that_take_only_labelled_issues_skip_the_others() {
    let f = Fixture::new().await;
    f.works_with_others(false, "labelled");
    f.fake
        .set_issues("\"v1\"", vec![open_issue(1, "One."), open_issue(2, "Two.")]);
    f.server.poll_github().await;
    let card = f.put_in(1, "In progress");
    let other = f.put_in(2, "In progress");

    let step = f.settle(&card).await;
    assert!(
        step.waiting
            .as_deref()
            .is_some_and(|w| w.contains("waiting for the label \"Ready for agent\"")),
        "{step:?}"
    );
    assert_eq!(f.runs_of(1), 0);

    // The owner's server released the issue: it carries the label.
    f.fake.edit_issue(1, |issue| {
        issue["labels"] = json!([{ "name": "Ready for agent" }])
    });
    f.server.poll_github().await;
    f.settle(&f.card(1).unwrap()).await;
    assert_eq!(f.runs_of(1), 1);

    // Labelled but taken by someone else, it is left alone too.
    f.fake.edit_issue(2, |issue| {
        issue["labels"] = json!([{ "name": "Ready for agent" }]);
        issue["assignees"] = json!([{ "login": "alice" }]);
    });
    f.server.poll_github().await;
    let step = f.settle(&other).await;
    assert_eq!(step.waiting.as_deref(), Some("taken by @alice"));
    assert_eq!(f.runs_of(2), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unlabelled_issue_a_person_of_this_server_took_is_started_here() {
    let f = Fixture::new().await;
    f.works_with_others(false, "labelled");
    f.owner_logs_in_as("leon-gh");
    let mut mine = open_issue(1, "One.");
    mine["assignees"] = json!([{ "login": "leon-gh" }]);
    f.fake.set_issues("\"v1\"", vec![mine]);
    f.server.poll_github().await;
    let card = f.put_in(1, "In progress");
    assert_eq!(card.taken_by, None);
    f.settle(&card).await;
    assert_eq!(f.runs_of(1), 1);
}

/// A pull request 7 of `alice` that says it fixes issue 1, listed as open.
fn alices_request() -> Value {
    json!({ "number": 7, "title": "Fix the thing", "body": "Fixes #1\n\nDone by hand.",
            "user": { "login": "alice" }, "head": { "sha": "deadbeef" } })
}

/// Has pull request 7 of the remote name a commit, as GitHub does for the head of every pull request.
fn give_the_remote_a_pull_head(f: &Fixture) {
    let head = git(&f.remote, &["rev-parse", "main"]).unwrap();
    git(&f.remote, &["update-ref", "refs/pull/7/head", &head]).unwrap();
}

/// Polls issue 1, taken by alice, into a card in progress that follows her open pull request 7.
async fn following(f: &Fixture) -> Card {
    give_the_remote_a_pull_head(f);
    let mut taken = open_issue(1, "One.");
    taken["assignees"] = json!([{ "login": "alice" }]);
    f.fake.set_issues("\"v1\"", vec![taken]);
    f.server.poll_github().await;
    f.put_in(1, "In progress");
    *f.fake.listed_pulls.lock().unwrap() = vec![alices_request()];
    f.fake.set_pull("open", false);
    f.server.poll_github().await;
    f.card(1).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_someone_else_opened_moves_the_card_to_review_and_to_done_on_merge() {
    let f = Fixture::new().await;
    let card = following(&f).await;
    assert_eq!(card.column, "In review");
    assert_eq!(card.taken_by.as_deref(), Some("alice"));
    let change = f
        .server
        .app
        .store
        .open_change_in(card.id, f.repository.id)
        .unwrap()
        .unwrap();
    assert_eq!(change.pull_request, Some(7));
    assert_eq!(change.outside_author.as_deref(), Some("alice"));
    assert_eq!(change.taken_from.as_deref(), Some("In progress"));
    // Its head is kept in a branch an agent can read.
    let clone = Path::new(&f.repository.path);
    assert!(git(clone, &["rev-parse", "--verify", "moebius/pr-7"]).is_ok());
    // The owner is told.
    let inbox = f.server.app.store.inbox_for_card(card.id).unwrap();
    let item = inbox.iter().find(|i| i.kind == "outside_change").unwrap();
    assert_eq!(item.data["author"], "alice");

    // No script moves it, whatever the column says.
    assert!(f.settle(&card).await.moved.is_none());
    assert_eq!(f.card(1).unwrap().column, "In review");

    // Its checks are read as an own change's are.
    f.fake.pulls.lock().unwrap().check_runs = vec![check("build", "in_progress", "")];
    f.server.poll_github().await;
    assert_eq!(f.change(change.id).checks.as_deref(), Some("pending"));
    // It is not merged while they have not passed.
    let (app, id) = (f.server.app.clone(), change.id);
    let refused = tokio::task::spawn_blocking(move || {
        app.merge_change(&Actor::first_owner(), id, "too early")
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(refused.to_string().contains("checks"), "{refused}");
    f.fake.pulls.lock().unwrap().check_runs = vec![check("build", "completed", "success")];
    f.server.poll_github().await;
    assert_eq!(f.change(change.id).checks.as_deref(), Some("success"));

    // Merged on the forge: the card is done.
    f.fake.set_pull("closed", true);
    *f.fake.listed_pulls.lock().unwrap() = Vec::new();
    f.server.poll_github().await;
    assert_eq!(f.change(change.id).state, "merged");
    assert_eq!(f.card(1).unwrap().column, "Done");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_owner_merges_a_followed_pull_request_from_the_card_once_its_checks_pass() {
    let f = Fixture::new().await;
    let card = following(&f).await;
    f.fake.pulls.lock().unwrap().check_runs = vec![check("build", "completed", "success")];
    f.server.poll_github().await;
    let change = f
        .server
        .app
        .store
        .open_change_in(card.id, f.repository.id)
        .unwrap()
        .unwrap();
    let (app, id) = (f.server.app.clone(), change.id);
    let commit = tokio::task::spawn_blocking(move || {
        app.merge_change(&Actor::first_owner(), id, "looks right")
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(commit, "c0ffee");
    assert_eq!(f.fake.pulls.lock().unwrap().merged.len(), 1);
    assert_eq!(f.card(1).unwrap().column, "Done");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_followed_pull_request_closed_unmerged_sends_the_card_back_still_taken() {
    let f = Fixture::new().await;
    let card = following(&f).await;
    let change = f
        .server
        .app
        .store
        .open_change_in(card.id, f.repository.id)
        .unwrap()
        .unwrap();
    f.fake.set_pull("closed", false);
    *f.fake.listed_pulls.lock().unwrap() = Vec::new();
    f.server.poll_github().await;
    assert_eq!(f.change(change.id).state, "closed");
    let card = f.card(1).unwrap();
    assert_eq!(card.column, "In progress", "back where it was");
    assert_eq!(
        card.taken_by.as_deref(),
        Some("alice"),
        "taken until the assignee is removed"
    );
    let inbox = f.server.app.store.inbox_for_card(card.id).unwrap();
    assert!(
        inbox
            .iter()
            .all(|i| i.kind != "outside_change" || i.resolved_at.is_some()),
        "{inbox:?}"
    );
    // The closed request is not followed again while it is not listed as open.
    f.server.poll_github().await;
    assert_eq!(f.card(1).unwrap().column, "In progress");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_of_this_servers_own_people_or_changes_is_not_followed() {
    let f = Fixture::new().await;
    f.owner_logs_in_as("alice");
    give_the_remote_a_pull_head(&f);
    f.fake.set_issues("\"v1\"", vec![open_issue(1, "One.")]);
    f.server.poll_github().await;
    f.put_in(1, "In progress");
    *f.fake.listed_pulls.lock().unwrap() = vec![alices_request()];
    f.server.poll_github().await;
    assert_eq!(f.card(1).unwrap().column, "In progress");
    let card = f.card(1).unwrap();
    assert!(
        f.server
            .app
            .store
            .open_change_in(card.id, f.repository.id)
            .unwrap()
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_reviewer_reads_the_followed_pull_request_from_its_branch() {
    let f = Fixture::new().await;
    let card = following(&f).await;
    let change = f
        .server
        .app
        .store
        .open_change_in(card.id, f.repository.id)
        .unwrap()
        .unwrap();
    // The work of the card is that request's branch.
    assert_eq!(
        f.server.app.store.code_branch(card.id).unwrap().as_deref(),
        Some("moebius/pr-7")
    );
    // Asked without a Reviewer, it says so and starts nothing.
    let (app, id) = (f.server.app.clone(), change.id);
    let runs = super::fake_github::IdleRuns;
    let error = tokio::task::spawn_blocking(move || {
        app.review_outside_change(&Actor::first_owner(), &runs, id, "")
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("Reviewer"), "{error}");
}
