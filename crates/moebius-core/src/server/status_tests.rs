//! Status labels on the forge (ADR 0033), against the fake GitHub: one label that starts with the prefix on every open
//! issue that has a card, following the column and the finer states.

use serde_json::{Value, json};

use super::fake_github::Fixture;
use super::reviews_tests::{ready, said};
use super::taking_tests::open_issue;
use crate::app::FlowEdit;
use crate::decide::Actor;

fn owner() -> Actor {
    Actor::first_owner()
}

impl Fixture {
    fn status_labels(&self, on: bool) {
        self.server
            .app
            .edit_flow(
                &owner(),
                self.repository.id,
                &FlowEdit {
                    status_labels: Some(on),
                    ..Default::default()
                },
                "test",
            )
            .unwrap();
    }

    /// The labels the fake holds on issue `number` that start with `status:`.
    fn status_on(&self, number: u64) -> Vec<String> {
        self.labels_on(number)
            .into_iter()
            .filter(|l| l.starts_with("status:"))
            .collect()
    }

    fn labels_on(&self, number: u64) -> Vec<String> {
        let issues = self.fake.issues.lock().unwrap();
        issues
            .1
            .iter()
            .find(|i| i["number"] == number)
            .map_or_else(Vec::new, |i| {
                i["labels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|l| l["name"].as_str().unwrap().to_string())
                    .collect()
            })
    }
}

async fn with_one_issue(labels: &[&str]) -> Fixture {
    let f = Fixture::new().await;
    let mut issue = open_issue(1, "One.");
    issue["labels"] = Value::Array(labels.iter().map(|l| json!({ "name": l })).collect());
    f.fake.set_issues("\"v1\"", vec![issue]);
    f.server.poll_github().await;
    f
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_written_while_the_setting_is_off() {
    let f = with_one_issue(&[]).await;
    f.server.poll_github().await;
    assert!(f.fake.label_writes.lock().unwrap().is_empty());
    assert!(f.fake.labels_made.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn labels_follow_the_column_and_only_one_status_label_stays() {
    let f = with_one_issue(&["bug", "status:old"]).await;
    f.status_labels(true);
    f.server.poll_github().await;
    // The card is in the column it was made in; the old status label is swapped, a label without the prefix stays.
    let column = f.card(1).unwrap().column;
    let wanted = format!("status:{}", column.to_lowercase().replace(' ', "-"));
    let labels = f.labels_on(1);
    assert_eq!(f.status_on(1), vec![wanted.clone()], "{labels:?}");
    assert!(labels.contains(&"bug".to_string()), "{labels:?}");
    // It was made with a colour.
    assert!(
        f.fake
            .labels_made
            .lock()
            .unwrap()
            .iter()
            .any(|(n, c)| *n == wanted && !c.is_empty())
    );

    f.put_in(1, "In progress");
    f.server.poll_github().await;
    f.server.poll_github().await;
    assert_eq!(f.status_on(1), vec!["status:in-progress".to_string()]);
    assert!(f.labels_on(1).contains(&"bug".to_string()));
    // A person takes it off by hand: it is put back at the next poll.
    f.fake
        .edit_issue(1, |i| i["labels"] = json!([{ "name": "bug" }]));
    f.server.poll_github().await;
    assert_eq!(f.status_on(1), vec!["status:in-progress".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_custom_column_gets_its_own_label() {
    let f = with_one_issue(&[]).await;
    crate::harness::repo::testing::use_harness(&f.server.app, f._dir.path());
    crate::harness::repo::testing::put(
        &f.server.app,
        "widgets",
        "kanban.toml",
        "[[column]]\nname = \"Ready for testing\"\nafter = \"In review\"\n",
    );
    f.status_labels(true);
    f.put_in(1, "Ready for testing");
    f.server.poll_github().await;
    f.server.poll_github().await;
    assert_eq!(f.status_on(1), vec!["status:ready-for-testing".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_finer_states_take_precedence_in_their_order() {
    let (f, card, _) = ready().await;
    f.status_labels(true);
    // Followed, in review.
    f.server.poll_github().await;
    assert_eq!(f.status_on(1), vec!["status:in-review".to_string()]);
    // Changes requested on the current head.
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    f.reviewer_ends(&card, &run, &said("changes", "Wrong.", json!([])))
        .await;
    f.server.poll_github().await;
    f.server.poll_github().await;
    assert_eq!(f.status_on(1), vec!["status:changes-requested".to_string()]);
    // The card waits for its author in In progress, and the label stays.
    assert_eq!(f.card(1).unwrap().column, crate::app::columns::IN_PROGRESS);
    // A frozen repository beats every other state.
    f.server
        .app
        .freeze_repository(&owner(), f.repository.id, "stop")
        .unwrap();
    f.server.poll_github().await;
    f.server.poll_github().await;
    assert_eq!(f.status_on(1), vec!["status:frozen".to_string()]);
    f.server
        .app
        .unfreeze_repository(&owner(), f.repository.id, "")
        .unwrap();
    f.server.poll_github().await;
    f.server.poll_github().await;
    assert_eq!(f.status_on(1), vec!["status:changes-requested".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn awaiting_a_measurement_and_taken_have_their_labels() {
    let (f, card, _) = ready().await;
    f.status_labels(true);
    f.author_says_done().await;
    let run = f.reviews_of(&card).remove(0);
    f.reviewer_ends(&card, &run, &said("approve_code", "Right.", json!([])))
        .await;
    f.server.poll_github().await;
    f.server.poll_github().await;
    assert_eq!(
        f.status_on(1),
        vec!["status:awaiting-measurement".to_string()]
    );

    // Taken by somebody, with no pull request followed: taken.
    let g = Fixture::new().await;
    let mut issue = open_issue(1, "One.");
    issue["assignees"] = json!([{ "login": "alice" }]);
    g.fake.set_issues("\"v1\"", vec![issue]);
    g.server.poll_github().await;
    g.status_labels(true);
    g.server.poll_github().await;
    g.server.poll_github().await;
    assert_eq!(g.status_on(1), vec!["status:taken".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_issue_keeps_the_label_it_had_and_the_prefix_can_be_changed() {
    let f = with_one_issue(&[]).await;
    f.status_labels(true);
    f.server.poll_github().await;
    let before = f.status_on(1);
    assert_eq!(before.len(), 1);
    f.fake.edit_issue(1, |i| i["state"] = json!("closed"));
    let writes = f.fake.label_writes.lock().unwrap().len();
    f.server.poll_github().await;
    f.server.poll_github().await;
    assert_eq!(f.fake.label_writes.lock().unwrap().len(), writes);
    assert_eq!(f.status_on(1), before);
    // Nothing but a short prefix without a comma or a new line is a prefix.
    for bad in ["", "a,b", "x\ny"] {
        assert!(
            f.server
                .app
                .edit_flow(
                    &owner(),
                    f.repository.id,
                    &FlowEdit {
                        status_prefix: Some(bad.into()),
                        ..Default::default()
                    },
                    ""
                )
                .is_err()
        );
    }
}
