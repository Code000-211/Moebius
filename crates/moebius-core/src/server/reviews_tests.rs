//! The review of a pull request someone else opened (ADR 0029), against the fake GitHub: the author's `/done` has the
//! Reviewer read it once its checks are green, what it found is written as a review with comments on lines, and an
//! approved pull request is merged pinned to the head that was read, or waits for the owner.

use std::sync::Arc;

use serde_json::{Value, json};

use super::fake_github::{Fixture, check};
use super::taking_tests::{ReviewerRuns, following, push_to_the_pull_request};
use crate::app::{ROUNDS_SPENT, columns};
use crate::decide::check::{CheckRequest, decide_call};
use crate::repos::git;
use crate::router::Grant;
use crate::store::{Card, Run};

/// The text a review run ends with.
fn said(verdict: &str, summary: &str, comments: Value) -> String {
    format!(
        "I read it.\n```json\n{}\n```",
        json!({ "verdict": verdict, "summary": summary, "comments": comments })
    )
}

impl Fixture {
    /// Gives pull request 7 a commit that adds `src/lib.rs` with ten lines, in the remote and in the fake.
    fn pull_adds_a_file(&self) -> String {
        let work = self._dir.path().join("pull-work");
        git(
            self._dir.path(),
            &[
                "clone",
                "--quiet",
                self.remote.to_str().unwrap(),
                "pull-work",
            ],
        )
        .unwrap();
        std::fs::create_dir_all(work.join("src")).unwrap();
        let lines: String = (1..=10).map(|n| format!("line {n}\n")).collect();
        std::fs::write(work.join("src/lib.rs"), lines).unwrap();
        git(&work, &["add", "."]).unwrap();
        git(
            &work,
            &[
                "-c",
                "user.name=alice",
                "-c",
                "user.email=alice@example.com",
                "commit",
                "--quiet",
                "-m",
                "Add the library",
            ],
        )
        .unwrap();
        git(
            &work,
            &["push", "--quiet", "origin", "HEAD:refs/pull/7/head"],
        )
        .unwrap();
        let head = git(&work, &["rev-parse", "HEAD"]).unwrap();
        self.fake.set_head(&head);
        self.fake.listed_pulls.lock().unwrap()[0]["head"]["sha"] = json!(head);
        head
    }

    /// Says what the checks of the pull request's head are.
    fn checks_are(&self, status: &str, conclusion: &str) {
        self.fake.pulls.lock().unwrap().check_runs = vec![check("build", status, conclusion)];
    }

    /// The author writes `/done` on the issue, and the poll reads it.
    async fn author_says_done(&self) {
        self.fake.comment(1, "alice", "/done");
        self.server.poll_github().await;
    }

    /// The runs of the card whose purpose is `review`.
    fn reviews_of(&self, card: &Card) -> Vec<Run> {
        self.server
            .app
            .store
            .runs_for_card(card.id)
            .unwrap()
            .into_iter()
            .filter(|r| r.purpose == "review")
            .collect()
    }

    /// The review run ends with `text`, and the engine ticks the card.
    async fn reviewer_ends(&self, card: &Card, run: &Run, text: &str) {
        self.server.app.store.set_run_summary(run.id, text).unwrap();
        self.server
            .app
            .store
            .set_run_status(run.id, "succeeded")
            .unwrap();
        let (engine, id) = (self.engine.clone(), card.id);
        tokio::task::spawn_blocking(move || engine.tick(id).unwrap())
            .await
            .unwrap();
    }

    fn on_the_pull_request(&self) -> Vec<String> {
        self.fake
            .posted
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| *n == 7)
            .map(|(_, text)| text.clone())
            .collect()
    }

    fn on_the_issue(&self) -> Vec<String> {
        self.fake
            .posted
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| *n == 1)
            .map(|(_, text)| text.clone())
            .collect()
    }

    fn merged_requests(&self) -> Vec<Value> {
        self.fake.pulls.lock().unwrap().merged.clone()
    }
}

/// A fixture whose card follows alice's pull request 7, which adds `src/lib.rs`, with green checks for its head, a
/// Reviewer that starts runs, and commands on. The pull request is not reviewed yet.
async fn ready() -> (Fixture, Card, String) {
    let f = Fixture::new().await;
    f.fake.threads.lock().unwrap().insert(1, Vec::new());
    f.fake.writers.lock().unwrap().push("alice".into());
    f.server
        .app
        .store
        .set_repository_commands(f.repository.id, true, 3, true)
        .unwrap();
    f.server.app.set_review_runs(Arc::new(ReviewerRuns));
    let card = following(&f).await;
    let head = f.pull_adds_a_file();
    f.checks_are("completed", "success");
    f.server.poll_github().await;
    (f, card, head)
}

#[tokio::test(flavor = "multi_thread")]
async fn done_with_green_checks_has_the_pull_request_reviewed_once() {
    let (f, card, head) = ready().await;
    assert_eq!(f.reviews_of(&card).len(), 0, "nothing starts by itself");
    f.author_says_done().await;
    let runs = f.reviews_of(&card);
    assert_eq!(runs.len(), 1);
    let change = f.server.app.followed_change(&card).unwrap().unwrap();
    let rounds = f.server.app.store.external_reviews(change.id).unwrap();
    assert_eq!((rounds[0].state.as_str(), rounds[0].round), ("running", 1));
    assert_eq!(rounds[0].head, head);
    assert_eq!(
        f.change(change.id).reviewed_head.as_deref(),
        Some(head.as_str())
    );

    // The same head is not read twice, however often the author says so.
    f.author_says_done().await;
    f.author_says_done().await;
    assert_eq!(f.reviews_of(&card).len(), 1);
    assert!(
        f.on_the_issue()
            .iter()
            .any(|t| t.contains("reviewed already")),
        "{:?}",
        f.on_the_issue()
    );
    // Nobody else's `/done` has it reviewed.
    f.fake.comment(1, "mallory", "/done");
    f.server.poll_github().await;
    assert_eq!(f.reviews_of(&card).len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn pending_checks_wait_and_red_checks_are_answered_on_the_pull_request() {
    let (f, card, _) = ready().await;
    f.checks_are("in_progress", "");
    f.server.poll_github().await;
    f.author_says_done().await;
    assert_eq!(f.reviews_of(&card).len(), 0, "it waits for the checks");
    f.server.poll_github().await;
    assert_eq!(f.reviews_of(&card).len(), 0, "still pending");

    f.checks_are("completed", "success");
    f.server.poll_github().await;
    assert_eq!(f.reviews_of(&card).len(), 1, "green: it starts");

    // Red checks: nothing is reviewed, and the pull request is told.
    let (g, card, _) = ready().await;
    g.checks_are("completed", "failure");
    g.server.poll_github().await;
    g.author_says_done().await;
    g.server.poll_github().await;
    assert_eq!(g.reviews_of(&card).len(), 0);
    let told = g.on_the_pull_request();
    assert!(told.iter().any(|t| t.contains("checks")), "{told:?}");
    // It does not repeat itself at every poll.
    g.server.poll_github().await;
    assert_eq!(g.on_the_pull_request().len(), told.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn changes_requested_are_a_review_with_comments_on_lines_and_the_rest_in_the_body() {
    let (f, card, head) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    let text = said(
        "changes",
        "Two problems.",
        json!([
            { "path": "src/lib.rs", "line": 4, "body": "Off by one." },
            { "path": "src/lib.rs", "line": 99, "body": "Not in the diff." },
            { "path": "elsewhere.rs", "line": 1, "body": "Not a file of it." },
        ]),
    );
    f.reviewer_ends(&card, &run, &text).await;

    let posts = f.fake.review_posts.lock().unwrap().clone();
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert_eq!(posts[0]["event"], "REQUEST_CHANGES");
    assert_eq!(posts[0]["commit_id"], json!(head));
    let comments = posts[0]["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["path"], "src/lib.rs");
    assert_eq!(comments[0]["line"], 4);
    assert_eq!(comments[0]["side"], "RIGHT");
    let body = posts[0]["body"].as_str().unwrap();
    assert!(body.starts_with("Two problems."), "{body}");
    assert!(
        body.contains("`src/lib.rs` line 99: Not in the diff."),
        "{body}"
    );
    assert!(body.contains("`elsewhere.rs` line 1"), "{body}");
    assert!(f.merged_requests().is_empty());
    assert_eq!(f.card(1).unwrap().column, columns::IN_REVIEW);

    let change = f.server.app.followed_change(&card).unwrap().unwrap();
    let round = &f.server.app.store.external_reviews(change.id).unwrap()[0];
    assert_eq!(round.state, "posted");
    assert!(
        round
            .url
            .as_deref()
            .unwrap()
            .contains("pullrequestreview-91")
    );
    assert!(
        f.on_the_issue()
            .iter()
            .any(|t| t.contains("changes are requested"))
    );

    // A forge that refuses the lines still gets the review, with them in its body.
    let (g, card, _) = ready().await;
    g.fake
        .refuse_line_comments
        .store(true, std::sync::atomic::Ordering::SeqCst);
    g.author_says_done().await;
    let run = g.reviews_of(&card).remove(0);
    g.reviewer_ends(
        &card,
        &run,
        &said(
            "changes",
            "One.",
            json!([{ "path": "src/lib.rs", "line": 2, "body": "Hm." }]),
        ),
    )
    .await;
    let posts = g.fake.review_posts.lock().unwrap().clone();
    assert_eq!(posts.len(), 1);
    assert!(posts[0]["comments"].as_array().unwrap().is_empty());
    assert!(posts[0]["body"].as_str().unwrap().contains("line 2: Hm."));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_merges_the_head_that_was_read_and_thanks_the_author() {
    let (f, card, head) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    f.reviewer_ends(&card, &run, &said("approve", "Looks right.", json!([])))
        .await;
    let posts = f.fake.review_posts.lock().unwrap().clone();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["event"], "APPROVE");
    assert_eq!(posts[0]["commit_id"], json!(head));
    let merged = f.merged_requests();
    assert_eq!(merged.len(), 1);
    assert_eq!(
        merged[0]["sha"],
        json!(head),
        "pinned to the head that was read"
    );
    assert_eq!(f.card(1).unwrap().column, columns::DONE);
    assert!(
        f.on_the_issue()
            .iter()
            .any(|t| t == "Merged, thank you @alice!"),
        "{:?}",
        f.on_the_issue()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn without_the_setting_an_approval_waits_for_the_owner_in_ready_to_merge() {
    let (f, card, _) = ready().await;
    f.server
        .app
        .store
        .set_repository_merge_reviewed(f.repository.id, false)
        .unwrap();
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    f.reviewer_ends(&card, &run, &said("approve", "Fine.", json!([])))
        .await;
    assert_eq!(f.fake.review_posts.lock().unwrap()[0]["event"], "APPROVE");
    assert!(f.merged_requests().is_empty());
    assert_eq!(f.card(1).unwrap().column, columns::READY_TO_MERGE);
    let inbox = f.server.app.store.inbox_for_card(card.id).unwrap();
    assert!(
        inbox
            .iter()
            .any(|i| i.kind == "ready_to_merge" && i.resolved_at.is_none()),
        "{inbox:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_push_after_the_review_never_merges_the_old_verdict() {
    // The poll has read the push: the verdict is dropped and nothing is written.
    let (f, card, _) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    push_to_the_pull_request(&f);
    f.server.poll_github().await;
    f.reviewer_ends(&card, &run, &said("approve", "Fine.", json!([])))
        .await;
    assert!(f.fake.review_posts.lock().unwrap().is_empty());
    assert!(f.merged_requests().is_empty());
    let change = f.server.app.followed_change(&card).unwrap().unwrap();
    assert_eq!(
        f.server.app.store.external_reviews(change.id).unwrap()[0].state,
        "stale"
    );
    assert_eq!(f.card(1).unwrap().column, columns::IN_REVIEW);
    // The new head can be reviewed: a dropped round does not count.
    f.checks_are("completed", "success");
    f.server.poll_github().await;
    f.author_says_done().await;
    assert_eq!(f.reviews_of(&card).len(), 2);

    // The push is not read yet: the forge refuses the merge pinned to the old head.
    let (g, card, _) = ready().await;
    g.author_says_done().await;
    let run = g.reviews_of(&card).remove(0);
    g.fake.set_head("0123456789abcdef0123456789abcdef01234567");
    g.reviewer_ends(&card, &run, &said("approve", "Fine.", json!([])))
        .await;
    assert!(g.merged_requests().is_empty());
    // Nothing was merged, and the owner is asked: the approval is on the forge.
    assert_eq!(g.card(1).unwrap().column, columns::READY_TO_MERGE);
    let told = g.on_the_pull_request();
    assert!(
        told.iter().any(|t| t.contains("could not be merged")),
        "{told:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fourth_round_asks_the_owner_instead() {
    let (f, card, _) = ready().await;
    f.author_says_done().await;
    for round in 1..=3 {
        let runs = f.reviews_of(&card);
        assert_eq!(runs.len(), round, "round {round}");
        f.reviewer_ends(
            &card,
            &runs[round - 1],
            &said("changes", "Again.", json!([])),
        )
        .await;
        // Each round needs a push of its own; the next `/done` starts the next round.
        if round < 3 {
            push_to_the_pull_request(&f);
            f.checks_are("completed", "success");
            f.server.poll_github().await;
            f.author_says_done().await;
        }
    }
    push_to_the_pull_request(&f);
    f.server.poll_github().await;
    f.author_says_done().await;
    assert_eq!(f.reviews_of(&card).len(), 3, "no fourth round");
    assert!(
        f.on_the_issue()
            .iter()
            .any(|t| t.contains("a person will look")),
        "{:?}",
        f.on_the_issue()
    );
    let inbox = f.server.app.store.inbox_for_card(card.id).unwrap();
    assert!(
        inbox
            .iter()
            .any(|i| i.kind == ROUNDS_SPENT && i.resolved_at.is_none()),
        "{inbox:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_that_gives_no_verdict_writes_nothing_and_may_be_asked_again() {
    let (f, card, _) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    f.reviewer_ends(&card, &run, "It looks fine to me.\nVERDICT: approve")
        .await;
    assert!(f.fake.review_posts.lock().unwrap().is_empty());
    assert!(f.merged_requests().is_empty());
    f.author_says_done().await;
    assert_eq!(
        f.reviews_of(&card).len(),
        2,
        "a failed round does not count"
    );
}

/// Asks whether the run may call `tool`.
fn decision(f: &Fixture, run: &Run, card: i64, tool: &str, input: Value) -> (String, String) {
    let reply = decide_call(
        &f.server.app,
        None,
        &Grant {
            run_id: run.id,
            card_id: Some(card),
            budget: None,
        },
        &CheckRequest {
            tool: tool.into(),
            input,
            tool_call_id: "call-1".into(),
        },
    )
    .unwrap();
    (reply.decision, reply.reason)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_run_started_restricted_is_refused_every_call_even_after_its_pull_request_is_closed()
 {
    let (f, card, _) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    // The run is started restricted, once (a real start does it, see the tests of the harness).
    f.server.app.store.set_run_restricted(run.id).unwrap();
    let run = f.server.app.run(run.id).unwrap();
    assert!(f.server.app.is_restricted_review(&run));

    let calls = [
        ("bash", json!({ "command": "ls" })),
        ("bash", json!({ "command": "cargo test" })),
        ("bash", json!({ "command": "curl https://example.com/x" })),
        ("Bash", json!({ "command": "git push origin HEAD" })),
        ("WebFetch", json!({ "url": "https://example.com" })),
        ("write", json!({ "path": "a.txt", "content": "x" })),
        ("search__query", json!({ "q": "x" })),
        ("lessons", json!({ "query": "x" })),
        // Not even what a restricted run that has files may read: it has none.
        ("read", json!({ "path": "src/lib.rs" })),
        ("read", json!({ "path": "@/etc/passwd" })),
        ("ls", json!({})),
        ("grep", json!({ "pattern": "line" })),
        ("Glob", json!({ "pattern": "/etc/*" })),
        // Arguments no check can read.
        ("read", json!("{\"path\": \"/etc/passwd\"")),
    ];
    for (tool, input) in &calls {
        let (decision, reason) = decision(&f, &run, card.id, tool, input.clone());
        assert_eq!(decision, "refuse", "{tool}");
        assert!(reason.contains("restricted-review"), "{tool}: {reason}");
    }

    // The attacker closes the pull request while the review runs: the run is as restricted as it was.
    f.fake.set_pull("closed", false);
    *f.fake.listed_pulls.lock().unwrap() = Vec::new();
    f.server.poll_github().await;
    assert!(f.server.app.followed_change(&card).unwrap().is_none());
    assert!(f.server.app.is_restricted_review(&run));
    for (tool, input) in &calls {
        let (decision, reason) = decision(&f, &run, card.id, tool, input.clone());
        assert_eq!(decision, "refuse", "after the close: {tool}");
        assert!(reason.contains("restricted-review"), "{tool}: {reason}");
    }

    // A run that was not started restricted is not made so by what the card follows.
    let ordinary = f
        .server
        .app
        .store
        .add_run(Some(card.id), "Coder", "idle", "code")
        .unwrap();
    assert!(!f.server.app.is_restricted_review(&ordinary));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_round_asked_for_at_one_head_and_started_at_another_reads_the_new_one() {
    let (f, card, _) = ready().await;
    // The checks of the head the author wrote `/done` at are still running.
    f.checks_are("in_progress", "");
    f.server.poll_github().await;
    f.author_says_done().await;
    assert_eq!(f.reviews_of(&card).len(), 0);
    // The author pushes, and the checks of the new head pass.
    let pushed = push_to_the_pull_request(&f);
    f.checks_are("completed", "success");
    f.server.poll_github().await;
    let runs = f.reviews_of(&card);
    assert_eq!(runs.len(), 1);
    let change = f.server.app.followed_change(&card).unwrap().unwrap();
    assert_eq!(
        f.server.app.store.external_reviews(change.id).unwrap()[0].head,
        pushed,
        "the round is of the head its run reads"
    );
    // Its verdict is not dropped as stale: the head it read is the head now.
    f.reviewer_ends(&card, &runs[0], &said("approve", "Fine.", json!([])))
        .await;
    assert_eq!(f.merged_requests().len(), 1);
    assert_eq!(f.merged_requests()[0]["sha"], json!(pushed));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_without_any_checks_is_reviewed_and_merged_and_says_so() {
    let (f, card, _) = ready().await;
    f.fake.pulls.lock().unwrap().check_runs = Vec::new();
    f.server.poll_github().await;
    f.author_says_done().await;
    let runs = f.reviews_of(&card);
    assert_eq!(runs.len(), 1, "no checks is nothing to wait for");
    assert!(
        f.on_the_pull_request()
            .iter()
            .any(|t| t.contains("No checks ran")),
        "{:?}",
        f.on_the_pull_request()
    );
    f.reviewer_ends(&card, &runs[0], &said("approve", "Fine.", json!([])))
        .await;
    assert_eq!(f.merged_requests().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_that_cannot_be_merged_waits_for_the_owner_and_is_merged_by_the_next_done() {
    let (f, card, head) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    f.fake
        .fail_merges
        .store(true, std::sync::atomic::Ordering::SeqCst);
    f.reviewer_ends(&card, &run, &said("approve", "Fine.", json!([])))
        .await;
    // The approval is on the forge, the pull request says what happened, and the owner is asked with the reason.
    assert_eq!(f.fake.review_posts.lock().unwrap().len(), 1);
    assert!(f.merged_requests().is_empty());
    assert_eq!(f.card(1).unwrap().column, columns::READY_TO_MERGE);
    assert!(
        f.on_the_pull_request()
            .iter()
            .any(|t| t.contains("could not be merged"))
    );
    let inbox = f.server.app.store.inbox_for_card(card.id).unwrap();
    let item = inbox
        .iter()
        .find(|i| i.kind == "ready_to_merge" && i.resolved_at.is_none())
        .unwrap_or_else(|| panic!("{inbox:?}"));
    assert!(item.data["reason"].as_str().is_some_and(|r| !r.is_empty()));

    // The forge is well again: the author's `/done` for the same head merges it, and writes no second review.
    f.fake
        .fail_merges
        .store(false, std::sync::atomic::Ordering::SeqCst);
    f.author_says_done().await;
    assert_eq!(f.merged_requests().len(), 1);
    assert_eq!(f.merged_requests()[0]["sha"], json!(head));
    assert_eq!(f.fake.review_posts.lock().unwrap().len(), 1);
    assert_eq!(f.reviews_of(&card).len(), 1, "it was not read again");
    assert_eq!(f.card(1).unwrap().column, columns::DONE);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_the_forge_failed_to_take_is_not_written_again_by_itself() {
    let (f, card, _) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    f.fake
        .fail_reviews
        .store(true, std::sync::atomic::Ordering::SeqCst);
    f.reviewer_ends(&card, &run, &said("approve", "Fine.", json!([])))
        .await;
    let attempts = || {
        f.fake
            .review_attempts
            .load(std::sync::atomic::Ordering::SeqCst)
    };
    // One attempt with the lines and one without are what a forge that refuses the lines gets; no more at later ticks.
    let first = attempts();
    for _ in 0..3 {
        let (engine, id) = (f.engine.clone(), card.id);
        tokio::task::spawn_blocking(move || engine.tick(id).unwrap())
            .await
            .unwrap();
        f.server.poll_github().await;
    }
    assert_eq!(attempts(), first);
    assert!(f.merged_requests().is_empty());
    let change = f.server.app.followed_change(&card).unwrap().unwrap();
    assert_eq!(
        f.server.app.store.external_reviews(change.id).unwrap()[0].state,
        "failed"
    );
    assert!(
        f.on_the_pull_request()
            .iter()
            .any(|t| t.contains("could not be written"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn two_threads_advancing_the_same_round_write_and_merge_it_once() {
    let (f, card, _) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    let text = said("approve", "Fine.", json!([]));
    f.server.app.store.set_run_summary(run.id, &text).unwrap();
    f.server
        .app
        .store
        .set_run_status(run.id, "succeeded")
        .unwrap();
    let mut threads = Vec::new();
    for _ in 0..6 {
        let (app, id) = (f.server.app.clone(), card.id);
        threads.push(tokio::task::spawn_blocking(move || {
            app.advance_external_reviews(id)
        }));
    }
    for thread in threads {
        thread.await.unwrap();
    }
    assert_eq!(f.fake.review_posts.lock().unwrap().len(), 1);
    assert_eq!(f.merged_requests().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn whether_a_run_is_restricted_is_kept_on_the_run_and_fails_closed() {
    let (f, card, _) = ready().await;
    let run = f
        .server
        .app
        .store
        .add_run(Some(card.id), "Reviewer", "idle", "review")
        .unwrap();
    assert!(!f.server.app.is_restricted_review(&run));
    f.server.app.store.set_run_restricted(run.id).unwrap();
    assert!(f.server.app.is_restricted_review(&run));
    // A card that is gone does not lift it, and neither does a run that cannot be found.
    f.server.app.store.delete_card(card.id, false).ok();
    assert!(f.server.app.is_restricted_review(&run));
}

/// Has the pull request's checks pending, the Reviewer approve it, and the round wait for them: its run ended, its answer
/// is read, and the round is `approved`, not merged.
async fn approved_and_waiting(f: &Fixture, card: &Card) {
    f.checks_are("in_progress", "");
    f.server.poll_github().await;
    f.author_says_done().await;
    assert_eq!(f.reviews_of(card).len(), 0, "it waits for the checks");
    f.checks_are("completed", "success");
    f.server.poll_github().await;
    let run = f.reviews_of(card).remove(0);
    f.checks_are("in_progress", "");
    f.server.poll_github().await;
    f.reviewer_ends(card, &run, &said("approve", "Fine.", json!([])))
        .await;
    let change = f.server.app.followed_change(card).unwrap().unwrap();
    assert_eq!(
        f.server.app.store.external_reviews(change.id).unwrap()[0].state,
        "approved"
    );
    assert!(f.merged_requests().is_empty());
}

async fn tick(f: &Fixture, card: &Card) {
    let (engine, id) = (f.engine.clone(), card.id);
    tokio::task::spawn_blocking(move || engine.tick(id).unwrap())
        .await
        .unwrap();
    f.server.poll_github().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_card_or_paused_agents_stop_a_round_from_posting_and_merging() {
    // An approval that waits for its checks does not merge while the card is held, and does when it is released.
    let (f, card, _) = ready().await;
    approved_and_waiting(&f, &card).await;
    f.server
        .app
        .store
        .hold_card(card.id, "person", Some("wait"))
        .unwrap();
    f.checks_are("completed", "success");
    f.server.poll_github().await;
    tick(&f, &card).await;
    assert!(f.merged_requests().is_empty(), "held");
    assert!(f.fake.review_posts.lock().unwrap().is_empty());
    f.server.app.store.release_card(card.id).unwrap();
    tick(&f, &card).await;
    assert_eq!(f.merged_requests().len(), 1, "released");

    // The same for pause all, which the poll and the author's `/done` meet as well as the engine.
    let (g, card, _) = ready().await;
    approved_and_waiting(&g, &card).await;
    g.server.app.store.pause_runs("person").unwrap();
    g.checks_are("completed", "success");
    g.server.poll_github().await;
    tick(&g, &card).await;
    g.author_says_done().await;
    assert!(g.merged_requests().is_empty(), "paused");
    assert!(g.fake.review_posts.lock().unwrap().is_empty());
    g.server.app.store.resume_runs().unwrap();
    tick(&g, &card).await;
    assert_eq!(g.merged_requests().len(), 1, "resumed");

    // A run that ended while the card was held is read once it is released, and writes nothing before.
    let (h, card, _) = ready().await;
    h.author_says_done().await;
    let run = h.reviews_of(&card).remove(0);
    h.server
        .app
        .store
        .hold_card(card.id, "person", None)
        .unwrap();
    h.reviewer_ends(&card, &run, &said("changes", "No.", json!([])))
        .await;
    assert!(h.fake.review_posts.lock().unwrap().is_empty(), "held");
    h.server.app.store.release_card(card.id).unwrap();
    tick(&h, &card).await;
    assert_eq!(h.fake.review_posts.lock().unwrap().len(), 1, "released");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_retargeted_to_another_branch_is_never_merged() {
    let (f, card, _) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    // The author retargets the approved pull request to a release branch; the poll reads it.
    f.fake.set_base("release");
    f.server.poll_github().await;
    f.reviewer_ends(&card, &run, &said("approve", "Fine.", json!([])))
        .await;
    assert!(f.merged_requests().is_empty());
    assert!(
        f.fake.review_posts.lock().unwrap().is_empty(),
        "no approval is written for it either"
    );
    let change = f.server.app.followed_change(&card).unwrap().unwrap();
    assert_eq!(
        f.server.app.store.external_reviews(change.id).unwrap()[0].state,
        "stale"
    );
    assert!(
        f.on_the_pull_request()
            .iter()
            .any(|t| t.contains("another branch")),
        "{:?}",
        f.on_the_pull_request()
    );
    // A pull request against another branch is not started on either.
    let (g, card, _) = ready().await;
    g.fake.set_base("release");
    g.server.poll_github().await;
    g.author_says_done().await;
    assert_eq!(g.reviews_of(&card).len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_stranger_made_the_reviewer_say_is_redacted_before_it_is_posted() {
    let (f, card, _) = ready().await;
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    let secret = "sk-ant-abcdefghijklmnopqrstuvwxyz0123456789";
    let text = said(
        "changes",
        &format!("A secret in the summary: {secret}."),
        json!([
            { "path": "src/lib.rs", "line": 3, "body": format!("And on a line: {secret}") },
            { "path": "src/lib.rs", "line": 99, "body": format!("Off the diff: {secret}") },
        ]),
    );
    f.reviewer_ends(&card, &run, &text).await;
    let posts = f.fake.review_posts.lock().unwrap().clone();
    assert_eq!(posts.len(), 1);
    let all = posts[0].to_string();
    assert!(!all.contains(secret), "{all}");
    assert!(all.contains("[redacted:anthropic-key]"), "{all}");
}
