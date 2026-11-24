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
pub(super) fn open_issue(number: u64, body: &str) -> Value {
    let mut issue = issue(number, "open", "leon", body);
    issue["issue_dependencies_summary"] = json!({ "total_blocked_by": 0 });
    issue
}

impl Fixture {
    /// Says the owner's login on GitHub.
    pub(super) fn owner_logs_in_as(&self, login: &str) {
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
    pub(super) fn works_with_others(&self, mark_ready: bool, agents_take: &str) {
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
    pub(super) fn put_in(&self, number: i64, column: &str) -> Card {
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
pub(super) fn alices_request() -> Value {
    json!({ "number": 7, "title": "Fix the thing", "body": "Fixes #1\n\nDone by hand.",
            "user": { "login": "alice" }, "head": { "sha": "deadbeef" }, "base": { "ref": "main" } })
}

/// Has pull request 7 of the remote name a commit, as GitHub does for the head of every pull request.
pub(super) fn give_the_remote_a_pull_head(f: &Fixture) {
    let head = git(&f.remote, &["rev-parse", "main"]).unwrap();
    git(&f.remote, &["update-ref", "refs/pull/7/head", &head]).unwrap();
}

/// Polls issue 1, taken by alice, into a card in progress that follows her open pull request 7.
pub(super) async fn following(f: &Fixture) -> Card {
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

/// The head of pull request 7 on the remote, and the fake saying it is the head.
pub(super) fn the_pull_head(f: &Fixture) -> String {
    let head = git(&f.remote, &["rev-parse", "refs/pull/7/head"]).unwrap();
    f.fake.set_head(&head);
    head
}

/// Gives pull request 7 a new head on the remote and in the fake, as a push to its branch does.
pub(super) fn push_to_the_pull_request(f: &Fixture) -> String {
    let tree = git(&f.remote, &["rev-parse", "main^{tree}"]).unwrap();
    let parent = git(&f.remote, &["rev-parse", "refs/pull/7/head"]).unwrap();
    let head = git(
        &f.remote,
        &[
            "commit-tree",
            &tree,
            "-p",
            &parent,
            "-m",
            "after the review",
        ],
    )
    .unwrap();
    git(&f.remote, &["update-ref", "refs/pull/7/head", &head]).unwrap();
    f.fake.set_head(&head);
    f.fake.listed_pulls.lock().unwrap()[0]["head"]["sha"] = json!(head);
    head
}

/// Runs that start a run of any agent, among them a Reviewer.
pub(super) struct ReviewerRuns;

impl crate::board::CardRuns for ReviewerRuns {
    fn start(
        &self,
        app: &std::sync::Arc<crate::app::App>,
        card: &Card,
        agent: &str,
        purpose: (&str, Option<&str>),
        _: crate::board::Variant,
    ) -> Result<crate::store::Run, crate::board::StartError> {
        app.store
            .add_run(Some(card.id), agent, "idle", purpose.0)
            .map_err(|e| crate::board::StartError::Store(e.to_string()))
    }
    fn has_agent(&self, _: &str) -> bool {
        true
    }
    fn runners(&self) -> Vec<crate::harness::remote::RunnerInfo> {
        Vec::new()
    }
    fn agents(&self) -> Vec<crate::board::AgentInfo> {
        Vec::new()
    }
    fn token_budget(&self, _: &str) -> Option<i64> {
        None
    }
    fn steer(&self, _: i64, _: &str) -> bool {
        false
    }
    fn stop(&self, _: i64) -> bool {
        false
    }
    fn hold(&self, _: bool) {}
}

#[tokio::test(flavor = "multi_thread")]
async fn the_merge_of_a_followed_pull_request_is_pinned_to_the_head_the_owner_saw() {
    let f = Fixture::new().await;
    let card = following(&f).await;
    let head = the_pull_head(&f);
    f.fake.pulls.lock().unwrap().check_runs = vec![check("build", "completed", "success")];
    f.fake.listed_pulls.lock().unwrap()[0]["head"]["sha"] = json!(head);
    f.server.poll_github().await;
    let change = f
        .server
        .app
        .store
        .open_change_in(card.id, f.repository.id)
        .unwrap()
        .unwrap();
    assert_eq!(change.head.as_deref(), Some(head.as_str()));

    // A push the poll has not read yet: its checks are unknown, so the merge waits.
    let app = f.server.app.clone();
    let id = change.id;
    app.store.set_change_head(id, "ffff", 1).unwrap();
    let refused = tokio::task::spawn_blocking({
        let app = app.clone();
        move || app.merge_change(&Actor::first_owner(), id, "now")
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(refused.to_string().contains("checks"), "{refused}");
    app.store.set_change_head(id, &head, 1).unwrap();

    // The forge is told which commit is merged, so that a later push is refused there, not merged unseen.
    tokio::task::spawn_blocking(move || app.merge_change(&Actor::first_owner(), id, "now"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f.fake.pulls.lock().unwrap().merged[0]["sha"], json!(head));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_counts_only_for_the_head_it_read() {
    let f = Fixture::new().await;
    let card = following(&f).await;
    the_pull_head(&f);
    let change = f
        .server
        .app
        .store
        .open_change_in(card.id, f.repository.id)
        .unwrap()
        .unwrap();
    assert!(!change.review_is_current(), "nothing was reviewed yet");

    let (app, id) = (f.server.app.clone(), change.id);
    tokio::task::spawn_blocking(move || {
        app.review_outside_change(&Actor::first_owner(), &ReviewerRuns, id, "")
    })
    .await
    .unwrap()
    .unwrap();
    assert!(f.change(change.id).review_is_current());

    // The author pushes again: the review is of the commit before.
    let pushed = push_to_the_pull_request(&f);
    f.server.poll_github().await;
    let after = f.change(change.id);
    assert_eq!(after.head.as_deref(), Some(pushed.as_str()));
    assert!(!after.review_is_current());
}

/// Polls issue 1 (assigned to `assignees`) with `request` open, in a card in progress.
async fn polled_with(f: &Fixture, request: Value, assignees: &[&str]) {
    give_the_remote_a_pull_head(f);
    let mut issue = open_issue(1, "One.");
    issue["assignees"] = json!(
        assignees
            .iter()
            .map(|a| json!({ "login": a }))
            .collect::<Vec<_>>()
    );
    f.fake.set_issues("\"v1\"", vec![issue]);
    f.server.poll_github().await;
    f.put_in(1, "In progress");
    *f.fake.listed_pulls.lock().unwrap() = vec![request];
    f.fake.set_pull("open", false);
    f.server.poll_github().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_pull_request_of_someone_who_may_write_or_is_assigned_is_followed() {
    // Anyone's fork may say "Fixes #1": a stranger's request does not take the card.
    let f = Fixture::new().await;
    polled_with(&f, alices_request(), &[]).await;
    assert_eq!(f.card(1).unwrap().column, "In progress");

    // A collaborator who may write is believed.
    f.fake.writers.lock().unwrap().push("alice".into());
    f.server.poll_github().await;
    assert_eq!(f.card(1).unwrap().column, "In review");

    // So is the issue's assignee, who needs no permission to be asked about.
    let g = Fixture::new().await;
    polled_with(&g, alices_request(), &["alice"]).await;
    assert_eq!(g.card(1).unwrap().column, "In review");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_against_another_branch_or_for_a_card_at_work_or_taken_by_someone_else_is_not_followed()
 {
    let f = Fixture::new().await;
    f.fake.writers.lock().unwrap().push("alice".into());
    let mut elsewhere = alices_request();
    elsewhere["base"]["ref"] = json!("release");
    polled_with(&f, elsewhere, &[]).await;
    assert_eq!(
        f.card(1).unwrap().column,
        "In progress",
        "not against the default branch"
    );

    // A run of Moebius is going on the card.
    let g = Fixture::new().await;
    g.fake.writers.lock().unwrap().push("alice".into());
    polled_with(
        &g,
        json!({ "number": 7, "title": "t", "body": "Fixes #2", "user": { "login": "alice" },
        "head": { "sha": "deadbeef" }, "base": { "ref": "main" } }),
        &[],
    )
    .await;
    let card = g.card(1).unwrap();
    g.server
        .app
        .store
        .add_run(Some(card.id), "Coder", "idle", "code")
        .unwrap();
    *g.fake.listed_pulls.lock().unwrap() = vec![alices_request()];
    g.server.poll_github().await;
    assert_eq!(g.card(1).unwrap().column, "In progress", "its run is going");
    g.server
        .app
        .store
        .set_run_status(
            g.server.app.store.runs_for_card(card.id).unwrap()[0].id,
            "succeeded",
        )
        .unwrap();
    g.server.poll_github().await;
    assert_eq!(
        g.card(1).unwrap().column,
        "In review",
        "once the run is over it is"
    );

    // Someone else took the issue: alice's request does not take it from them.
    let h = Fixture::new().await;
    h.fake.writers.lock().unwrap().push("alice".into());
    polled_with(&h, alices_request(), &["bob"]).await;
    assert_eq!(h.card(1).unwrap().column, "In progress");
    assert_eq!(h.card(1).unwrap().taken_by.as_deref(), Some("bob"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reopened_pull_request_is_followed_again() {
    let f = Fixture::new().await;
    let card = following(&f).await;
    f.fake.set_pull("closed", false);
    *f.fake.listed_pulls.lock().unwrap() = Vec::new();
    f.server.poll_github().await;
    assert_eq!(f.card(1).unwrap().column, "In progress");

    f.fake.set_pull("open", false);
    *f.fake.listed_pulls.lock().unwrap() = vec![alices_request()];
    f.server.poll_github().await;
    assert_eq!(f.card(1).unwrap().column, "In review");
    let open = f
        .server
        .app
        .store
        .open_change_in(card.id, f.repository.id)
        .unwrap()
        .unwrap();
    assert_eq!(open.outside_author.as_deref(), Some("alice"));
}

#[tokio::test(flavor = "multi_thread")]
async fn when_two_servers_take_an_issue_at_once_the_one_listed_first_keeps_it() {
    // The other server's login was added first: this server's person is taken off again, and nothing starts.
    let f = Fixture::new().await;
    f.fake.set_issues("\"v1\"", vec![open_issue(1, "One.")]);
    f.server.poll_github().await;
    f.owner_logs_in_as("leon-gh");
    let card = f.put_in(1, "In progress");
    f.fake.assigned.lock().unwrap().push((1, "alice".into()));
    let step = f.settle(&card).await;
    assert_eq!(f.runs_of(1), 0, "{step:?}");
    assert_eq!(
        *f.fake.unassigned.lock().unwrap(),
        vec![(1, "leon-gh".to_string())]
    );
    assert_eq!(f.card(1).unwrap().taken_by.as_deref(), Some("alice"));
    let refused = f.claim(&f.card(1).unwrap()).await.unwrap_err();
    assert!(refused.to_string().contains("taken by @alice"), "{refused}");

    // This server's login was added first: the issue is this server's, the other login after it blocks nothing, and
    // later runs start as before.
    let g = Fixture::new().await;
    g.fake.set_issues("\"v1\"", vec![open_issue(1, "One.")]);
    g.server.poll_github().await;
    g.owner_logs_in_as("leon-gh");
    let card = g.put_in(1, "In progress");
    g.settle(&card).await;
    assert_eq!(g.runs_of(1), 1);
    g.fake.edit_issue(1, |issue| {
        let mut assignees = issue["assignees"].as_array().cloned().unwrap_or_default();
        assignees.push(json!({ "login": "alice" }));
        issue["assignees"] = json!(assignees);
    });
    g.server.poll_github().await;
    let card = g.card(1).unwrap();
    assert_eq!(card.taken_by, None);
    assert!(g.server.app.left_alone(&card).unwrap().is_none());
}
