//! Commands in issue comments (ADR 0028), against the fake GitHub: `/take`, `/release` and `/done` are read from the
//! comments the poll reads, each once, answered on the issue, and limited.

use serde_json::json;

use super::fake_github::Fixture;
use super::taking_tests::{following, open_issue};
use crate::app::DONE_ASKED;
use crate::decide::Actor;
use crate::store::{Card, InboxItem};

impl Fixture {
    /// Turns the commands on, with the limit and whether the server answers on the issue.
    fn commands_on(&self, limit: i64, answer: bool) {
        self.server
            .app
            .store
            .set_repository_commands(self.repository.id, true, limit, answer)
            .unwrap();
    }

    /// Polls the issues once `login` wrote `body` on issue `number`.
    async fn says(&self, number: u64, login: &str, body: &str) {
        self.fake.comment(number, login, body);
        self.server.poll_github().await;
    }

    /// What the server wrote on the issues, in order.
    fn answers(&self) -> Vec<(u64, String)> {
        self.fake.posted.lock().unwrap().clone()
    }

    fn last_answer(&self) -> String {
        self.answers().last().cloned().unwrap_or_default().1
    }

    fn assigned_now(&self) -> Vec<(u64, String)> {
        self.fake.assigned.lock().unwrap().clone()
    }

    fn asked_about(&self, number: i64) -> Option<InboxItem> {
        let card = self.card(number)?;
        self.server
            .app
            .store
            .inbox_for_card(card.id)
            .unwrap()
            .into_iter()
            .find(|i| i.kind == DONE_ASKED && i.resolved_at.is_none())
    }

    async fn owner_answers(&self, number: i64, answer: &str, reason: &str) -> Card {
        let (app, id) = (self.server.app.clone(), self.card(number).unwrap().id);
        let (answer, reason) = (answer.to_string(), reason.to_string());
        tokio::task::spawn_blocking(move || {
            app.answer_done(&Actor::first_owner(), id, &answer, &reason)
        })
        .await
        .unwrap()
        .unwrap()
    }
}

/// A fixture with two open issues whose threads exist, polled once, with commands on.
async fn ready(limit: i64) -> Fixture {
    let f = Fixture::new().await;
    f.fake
        .set_issues("\"v1\"", vec![open_issue(1, "One."), open_issue(2, "Two.")]);
    f.fake.threads.lock().unwrap().insert(1, Vec::new());
    f.fake.threads.lock().unwrap().insert(2, Vec::new());
    f.server.poll_github().await;
    f.commands_on(limit, true);
    f
}

#[tokio::test(flavor = "multi_thread")]
async fn take_assigns_the_commenter_answers_and_greys_the_card() {
    let f = ready(3).await;
    f.says(1, "alice", "/take").await;
    assert_eq!(f.assigned_now(), vec![(1, "alice".to_string())]);
    assert_eq!(f.answers(), vec![(1, "Assigned to @alice.".to_string())]);
    let card = f.card(1).unwrap();
    assert_eq!(card.taken_by.as_deref(), Some("alice"));
    assert!(card.taken_by_command);
    // The card is left alone by this server's scripts, like any issue someone else took.
    assert_eq!(
        f.server.app.left_alone(&card).unwrap().as_deref(),
        Some("taken by @alice")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_take_by_someone_else_is_refused_with_an_answer() {
    let f = ready(3).await;
    f.says(1, "alice", "/take").await;
    f.says(1, "bob", "/take").await;
    assert_eq!(f.assigned_now(), vec![(1, "alice".to_string())]);
    assert_eq!(f.last_answer(), "Already taken by @alice.");
    f.says(1, "alice", "/Take").await;
    assert_eq!(f.last_answer(), "Already assigned to you.");
    assert_eq!(f.assigned_now().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_login_holds_only_as_many_issues_as_the_repository_allows() {
    let f = ready(1).await;
    f.says(1, "alice", "/take").await;
    f.says(2, "alice", "/take").await;
    assert_eq!(f.assigned_now(), vec![(1, "alice".to_string())]);
    assert_eq!(
        f.last_answer(),
        "You already have 1 issue; release one first."
    );
    // Someone else is not held back by it, and releasing makes room.
    f.says(2, "bob", "/take").await;
    assert_eq!(f.last_answer(), "Assigned to @bob.");
    f.says(1, "alice", "/release").await;
    f.says(1, "alice", "/take").await;
    assert_eq!(f.last_answer(), "Assigned to @alice.");
}

#[tokio::test(flavor = "multi_thread")]
async fn with_the_ready_label_on_only_labelled_issues_can_be_taken() {
    let f = Fixture::new().await;
    f.works_with_others(true, "all");
    // Issue 1 waits for issue 2, so the server does not label it; issue 2 is labelled at the poll.
    f.fake.set_issues(
        "\"v1\"",
        vec![
            open_issue(1, "One.\n\nBlocked by #2"),
            open_issue(2, "Two."),
        ],
    );
    f.fake.threads.lock().unwrap().insert(1, Vec::new());
    f.fake.threads.lock().unwrap().insert(2, Vec::new());
    f.server.poll_github().await;
    f.commands_on(3, true);
    f.says(1, "alice", "/take").await;
    assert_eq!(
        f.last_answer(),
        "Only issues labelled Ready for agent can be taken."
    );
    assert!(f.assigned_now().is_empty());
    f.says(2, "alice", "/take").await;
    assert_eq!(f.last_answer(), "Assigned to @alice.");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_issue_cannot_be_taken() {
    let f = ready(3).await;
    f.fake
        .edit_issue(1, |issue| issue["state"] = json!("closed"));
    f.says(1, "alice", "/take").await;
    assert_eq!(f.last_answer(), "This issue is closed.");
    assert!(f.assigned_now().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn release_is_for_the_assignee_and_the_card_goes_back_to_normal() {
    let f = ready(3).await;
    f.says(1, "alice", "/take").await;
    f.says(1, "bob", "/release").await;
    assert_eq!(f.last_answer(), "Only @alice can do that.");
    assert!(f.fake.unassigned.lock().unwrap().is_empty());
    f.says(1, "alice", "/release").await;
    assert_eq!(f.last_answer(), "Released.");
    assert_eq!(
        *f.fake.unassigned.lock().unwrap(),
        vec![(1, "alice".to_string())]
    );
    let card = f.card(1).unwrap();
    assert_eq!((card.taken_by, card.taken_by_command), (None, false));
    f.says(1, "alice", "/release").await;
    assert_eq!(f.last_answer(), "Nobody has this issue.");
}

#[tokio::test(flavor = "multi_thread")]
async fn done_without_a_pull_request_asks_the_owner_who_answers_in_three_ways() {
    let f = ready(3).await;
    f.says(1, "alice", "/take").await;
    f.says(1, "bob", "/done").await;
    assert_eq!(f.last_answer(), "Only @alice can do that.");
    assert!(f.asked_about(1).is_none());

    f.says(1, "alice", "/done it works now\nsee the notes")
        .await;
    assert_eq!(f.last_answer(), "Thanks, the owner will take a look.");
    let item = f.asked_about(1).expect("the owner is asked");
    assert_eq!(item.data["login"], "alice");
    assert_eq!(item.data["reason"], "it works now\nsee the notes");
    assert!(
        item.title.contains("@alice says #1 is done"),
        "{}",
        item.title
    );
    f.says(1, "alice", "/done again").await;
    assert_eq!(f.last_answer(), "The owner has been asked already.");

    // Give it back: the assignee stays and the question is gone.
    f.owner_answers(1, "give_back", "Not yet.").await;
    assert!(f.asked_about(1).is_none());
    assert_eq!(
        f.last_answer(),
        "Back to @alice: it is not done yet. Not yet."
    );
    assert_eq!(f.card(1).unwrap().taken_by.as_deref(), Some("alice"));

    // Release it: the assignee goes.
    f.says(1, "alice", "/done").await;
    f.owner_answers(1, "release", "").await;
    assert!(f.asked_about(1).is_none());
    assert_eq!(f.last_answer(), "Released by the owner.");
    assert_eq!(f.card(1).unwrap().taken_by, None);
    assert_eq!(f.fake.unassigned.lock().unwrap().len(), 1);

    // Close it: the issue is closed on the forge.
    f.says(1, "alice", "/take").await;
    f.says(1, "alice", "/done").await;
    let closed = f.owner_answers(1, "close", "").await;
    assert!(closed.closed);
    assert_eq!(
        *f.fake.state_writes.lock().unwrap(),
        vec![(1, "closed".to_string())]
    );
    assert_eq!(f.last_answer(), "Closed, thanks @alice.");
    assert!(f.asked_about(1).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_person_answers_and_only_a_question_can_be_answered() {
    let f = ready(3).await;
    f.says(1, "alice", "/take").await;
    let id = f.card(1).unwrap().id;
    let app = f.server.app.clone();
    let unasked = tokio::task::spawn_blocking(move || {
        app.answer_done(&Actor::first_owner(), id, "close", "")
    })
    .await
    .unwrap();
    assert!(unasked.is_err(), "nobody said it is done");
    f.says(1, "alice", "/done").await;
    let app = f.server.app.clone();
    let agent = Actor::Agent {
        name: "Moebius".into(),
        run: 1,
    };
    let refused = tokio::task::spawn_blocking(move || app.answer_done(&agent, id, "close", ""))
        .await
        .unwrap();
    assert!(refused.is_err());
    assert!(f.fake.state_writes.lock().unwrap().is_empty());
    assert!(f.asked_about(1).is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn done_with_a_followed_pull_request_only_says_thanks() {
    let f = Fixture::new().await;
    f.fake.threads.lock().unwrap().insert(1, Vec::new());
    let card = following(&f).await;
    assert_eq!(card.column, "In review");
    f.commands_on(3, true);
    f.says(1, "alice", "/done").await;
    assert_eq!(
        f.answers(),
        vec![(1, "Thanks, the pull request is followed.".to_string())]
    );
    assert!(f.asked_about(1).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn each_comment_is_handled_once_across_polls_and_rereads() {
    let f = ready(3).await;
    f.says(1, "alice", "/take").await;
    for _ in 0..3 {
        f.server.poll_github().await;
    }
    // What a restart with nothing remembered of the comments does: they are all read again.
    f.server
        .app
        .store
        .set_forge_seen(f.repository.id, 1, "comments", "0")
        .unwrap();
    f.fake.edit_issue(1, |_| {});
    f.server.poll_github().await;
    assert_eq!(f.assigned_now().len(), 1);
    assert_eq!(f.answers().len(), 1);
    assert!(f.server.app.store.command_seen(f.repository.id, 1).unwrap() >= 1000);
    // A later comment is read, and only it.
    f.says(1, "alice", "/release").await;
    assert_eq!(f.answers().len(), 2);
    assert_eq!(f.fake.unassigned.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn quoted_and_coded_commands_are_ignored() {
    let f = ready(3).await;
    for body in [
        "> /take",
        "```\n/take\n```",
        "    /take",
        "please /take this",
    ] {
        f.says(1, "alice", body).await;
    }
    assert!(f.assigned_now().is_empty());
    assert!(f.answers().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_servers_own_comments_are_never_commands() {
    let f = ready(3).await;
    let (app, id) = (f.server.app.clone(), f.card(1).unwrap().id);
    tokio::task::spawn_blocking(move || app.comment_issue(&Actor::first_owner(), id, "/take"))
        .await
        .unwrap()
        .unwrap();
    f.server.poll_github().await;
    assert!(f.assigned_now().is_empty());
    // Its replies are comments too, and no reply is answered with a reply.
    f.says(1, "alice", "/take").await;
    f.server.poll_github().await;
    f.server.poll_github().await;
    assert_eq!(f.answers().len(), 2, "its own comment and one reply");
}

#[tokio::test(flavor = "multi_thread")]
async fn comments_from_before_the_commands_were_turned_on_are_not_commands() {
    let f = Fixture::new().await;
    f.fake.set_issues("\"v1\"", vec![open_issue(1, "One.")]);
    f.fake.threads.lock().unwrap().insert(1, Vec::new());
    f.server.poll_github().await;
    // Written while the commands were off, and so long ago that nothing could say they were meant.
    f.fake
        .comment_at(1, "alice", "/take", "2020-01-01T00:00:00Z");
    f.server.poll_github().await;
    f.commands_on(3, true);
    f.fake.comment_at(1, "bob", "/take", "2020-01-01T00:00:00Z");
    f.server.poll_github().await;
    assert!(f.assigned_now().is_empty());
    f.says(1, "carol", "/take").await;
    assert_eq!(f.assigned_now(), vec![(1, "carol".to_string())]);
}

#[tokio::test(flavor = "multi_thread")]
async fn without_answering_on_the_forge_the_command_is_done_and_nothing_is_written() {
    let f = ready(3).await;
    f.commands_on(3, false);
    f.says(1, "alice", "/take").await;
    assert_eq!(f.assigned_now(), vec![(1, "alice".to_string())]);
    assert!(f.answers().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_owner_can_always_release_what_was_taken_with_a_command() {
    let f = ready(3).await;
    f.says(1, "alice", "/take").await;
    let (app, id) = (f.server.app.clone(), f.card(1).unwrap().id);
    let released =
        tokio::task::spawn_blocking(move || app.release_taken(&Actor::first_owner(), id, ""))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(released.taken_by, None);
    assert_eq!(f.last_answer(), "Released by the owner.");
    assert_eq!(
        *f.fake.unassigned.lock().unwrap(),
        vec![(1, "alice".to_string())]
    );
    // An issue nobody took with a command has nothing to release.
    let (app, id) = (f.server.app.clone(), f.card(2).unwrap().id);
    let again =
        tokio::task::spawn_blocking(move || app.release_taken(&Actor::first_owner(), id, ""))
            .await
            .unwrap();
    assert!(again.is_err());
}
