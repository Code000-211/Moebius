//! The import against a fake Jira on a port of its own: every kind of record, a second import that changes only what
//! changed there, and a Jira user without a person who keeps their name.

use std::sync::Arc;
use std::time::Duration;

use moebius_plugin_api::Host;
use serde_json::json;

use super::*;
use crate::app::CycleFields;
use crate::repos::jira::{Connection, fetch};
use crate::repos::jira_fake::{self, Shared, basic};
use crate::store::{DUPLICATE, RELATED, Store};

struct Fixture {
    _dir: tempfile::TempDir,
    app: App,
    site: Shared,
    address: String,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
    let app = App::new(
        store,
        Arc::new(Host::new(Duration::from_secs(1))),
        &dir.path().join("data"),
    );
    let (address, site) =
        jira_fake::serve(jira_fake::project(basic("ada@example.com", "tok"))).await;
    Fixture {
        _dir: dir,
        app,
        site,
        address,
    }
}

impl Fixture {
    fn project(&self, name: &str) -> i64 {
        // A project without a repository, as ADR 0019 allows: its cards live in its own files.
        self.app.add_project(name, "WEB").unwrap().id
    }

    async fn import(&self, project: i64, users: Users) -> Report {
        self.import_mapped(project, users, BTreeMap::new())
            .await
            .unwrap()
    }

    async fn import_mapped(
        &self,
        project: i64,
        users: Users,
        user_map: BTreeMap<String, String>,
    ) -> Result<Report> {
        let connection = Connection {
            url: self.address.clone(),
            email: Some("ada@example.com".into()),
            token: "tok".into(),
        };
        let fetched = fetch(&connection, "WEB").await.unwrap();
        let options = Options {
            site: self.address.clone(),
            users,
            user_map,
        };
        self.app
            .import_jira(&Actor::first_owner(), project, &options, &fetched)
    }

    fn card(&self, project: i64, title: &str) -> crate::store::Card {
        self.app
            .store
            .all_cards()
            .unwrap()
            .into_iter()
            .find(|c| c.project_id == project && c.title == title)
            .unwrap_or_else(|| panic!("no card {title}"))
    }

    /// Changes the issue `key` of the fake.
    fn change(&self, key: &str, edit: impl FnOnce(&mut serde_json::Value)) {
        let mut site = self.site.lock().unwrap();
        let issue = site.issues.iter_mut().find(|i| i["key"] == key).unwrap();
        edit(&mut issue["fields"]);
    }
}

#[tokio::test]
async fn a_fake_jira_is_imported_with_every_kind_of_record() {
    let f = fixture().await;
    let project = f.project("Web shop");
    let report = f.import(project, Users::People).await;
    let app = &f.app;

    // The report counts what was made; the sprint without dates is the one thing left out.
    assert_eq!(
        report.issues,
        Counts {
            created: 5,
            updated: 0,
            unchanged: 0
        }
    );
    assert_eq!(report.kinds.created, 4, "{report:?}");
    assert_eq!(report.people.created, 3);
    assert_eq!(report.milestones.created, 1);
    assert_eq!(report.cycles.created, 2);
    assert_eq!(report.comments.created, 3);
    assert_eq!(
        report.links,
        LinkCounts {
            added: 3,
            removed: 0,
            unchanged: 0,
            skipped: 1
        }
    );
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.warnings[0].contains("Sprint 3"));

    // An issue lives in Moebius only: in the project's own files, unconfirmed, in the backlog.
    let story = f.card(project, "Pay with card");
    let files = app.project_files(project).unwrap();
    assert_eq!(story.repository_id, files.id);
    assert_eq!(story.column, "Backlog");
    assert!(!story.confirmed);
    assert!(story.body.contains("As a buyer I pay with a card."));
    assert!(story.body.contains(&format!("{}/browse/WEB-2", f.address)));
    assert_eq!(story.author, "person:ada");
    assert_eq!(
        story.created_at,
        super::read::timestamp("2026-10-01T09:30:00.000+0000").unwrap()
    );

    // Kind, fields, labels, priority, due date, estimate and assignee.
    assert_eq!(story.kind.as_deref(), Some("Story"));
    assert_eq!(
        story.fields,
        BTreeMap::from([
            ("Customer".to_string(), json!("ACME")),
            ("Go-live".to_string(), json!("2026-12-15")),
            ("Severity".to_string(), json!("Major")),
        ])
    );
    assert!(
        app.kind_named("Bug")
            .unwrap()
            .fields
            .iter()
            .any(|f| f.name == "Severity")
    );
    assert!(app.kind_named("Epic").is_err());
    for label in ["payments", "priority:high", "due:2026-12-01", "estimate:5"] {
        assert!(
            story.labels.contains(&label.to_string()),
            "{label}: {:?}",
            story.labels
        );
    }
    assert!(!story.labels.contains(&"priority:stale".to_string()));
    assert_eq!(story.assignees, vec!["grace-hopper".to_string()]);
    assert_eq!(
        f.card(project, "Refund fails").priority,
        crate::store::Priority::Urgent
    );
    let kinds: Vec<_> = app.store.project_kinds(project).unwrap();
    assert_eq!(kinds.len(), 4);

    // People are made disabled, with a role that does nothing until an owner gives them a token.
    let ada = app.store.person_by_handle("ada").unwrap().unwrap();
    assert!(ada.disabled);
    assert_eq!(ada.name, "Ada Lovelace");
    assert!(
        app.store
            .person_by_handle("old-colleague")
            .unwrap()
            .is_some()
    );

    // The epic is a milestone with its issues in it, and no card.
    let checkout = app.store.milestone_by_name("Checkout").unwrap().unwrap();
    assert_eq!(checkout.target_date, "2026-12-31");
    let held: Vec<i64> = app
        .store
        .card_milestones(story.id)
        .unwrap()
        .iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(held, vec![checkout.id]);
    assert!(
        app.store
            .all_cards()
            .unwrap()
            .iter()
            .all(|c| c.title != "Checkout")
    );

    // A sprint is a cycle; the closed one is closed, and its unfinished card went out of it.
    let cycles = app.store.cycles(project).unwrap();
    let named = |n: &str| cycles.iter().find(|c| c.name == n).unwrap().clone();
    assert!(named("Sprint 1").closed_at.is_some());
    assert!(named("Sprint 2").closed_at.is_none());
    assert_eq!(
        app.store.card_cycle(story.id).unwrap().unwrap().name,
        "Sprint 2"
    );
    assert!(
        app.store
            .card_cycle(f.card(project, "Refund fails").id)
            .unwrap()
            .is_none()
    );

    // A sub-task has its parent, and a done issue is closed.
    let task = f.card(project, "Validate card number");
    assert_eq!(task.parent, Some(story.number));
    assert!(task.closed);
    assert_eq!(task.column, "Done");

    // Links: a blocker, a duplicate (closed, as Moebius closes one) and a related card.
    let bug = f.card(project, "Refund fails");
    let original = f.card(project, "Refund error");
    assert_eq!(app.store.dependencies(bug.id).unwrap(), vec![task.number]);
    let duplicate = app.store.links_from(bug.id, DUPLICATE).unwrap();
    assert_eq!(duplicate[0].other_id, original.id);
    assert!(app.card(bug.id).unwrap().closed);
    assert_eq!(
        app.store.links_from(bug.id, RELATED).unwrap()[0].other_id,
        story.id
    );

    // Comments keep their time and who wrote them, and the third came from the page of its own.
    let comments = app.comments(story.id).unwrap();
    let who: Vec<&str> = comments.iter().map(|c| c.author.as_str()).collect();
    assert_eq!(
        who,
        ["person:ada", "person:old-colleague", "person:grace-hopper"]
    );
    assert_eq!(comments[2].body, "Shipping Friday.");
    assert_eq!(
        comments[0].at,
        super::read::timestamp("2026-10-02T10:00:00.000+0000").unwrap()
    );
}

#[tokio::test]
async fn a_second_import_changes_only_what_changed_in_jira() {
    let f = fixture().await;
    let project = f.project("Web shop");
    f.import(project, Users::People).await;
    let story = f.card(project, "Pay with card");
    let comments = f.app.comments(story.id).unwrap().len();
    let cards = f.app.store.all_cards().unwrap().len();

    // Nothing changed there: nothing is made, nothing is written, nothing is announced.
    let mut events = f.app.bus.subscribe();
    let again = f.import(project, Users::People).await;
    assert_eq!(
        again.issues,
        Counts {
            created: 0,
            updated: 0,
            unchanged: 5
        }
    );
    assert_eq!(
        again.comments,
        Counts {
            created: 0,
            updated: 0,
            unchanged: 3
        }
    );
    assert_eq!(
        again.kinds,
        Counts {
            created: 0,
            updated: 0,
            unchanged: 4
        }
    );
    assert_eq!(
        again.milestones,
        Counts {
            created: 0,
            updated: 0,
            unchanged: 1
        }
    );
    assert_eq!(
        again.cycles,
        Counts {
            created: 0,
            updated: 0,
            unchanged: 2
        }
    );
    assert_eq!(again.people.created, 0);
    assert_eq!(
        again.links,
        LinkCounts {
            added: 0,
            removed: 0,
            unchanged: 3,
            skipped: 1
        }
    );
    assert!(
        events.try_recv().is_err(),
        "an unchanged import announced something"
    );
    assert_eq!(f.app.store.all_cards().unwrap().len(), cards);

    // One issue is retitled and a comment edited, a link is gone, a card is done, and the duplicate is edited too.
    f.change("WEB-2", |fields| {
        fields["summary"] = json!("Pay with a card");
        fields["comment"]["comments"][1]["body"] = json!("We tried this in 2020.");
    });
    f.change("WEB-3", |fields| fields["issuelinks"] = json!([]));
    f.change("WEB-4", |fields| {
        fields["issuelinks"].as_array_mut().unwrap().remove(0);
        fields["summary"] = json!("Refund fails twice");
    });
    f.change("WEB-6", |fields| {
        fields["status"] = json!({"name": "Done", "statusCategory": {"key": "done"}});
    });
    let changed = f.import(project, Users::People).await;
    assert_eq!(
        changed.issues,
        Counts {
            created: 0,
            updated: 3,
            unchanged: 2
        }
    );
    assert_eq!(
        changed.comments,
        Counts {
            created: 0,
            updated: 1,
            unchanged: 2
        }
    );
    assert_eq!(
        changed.links,
        LinkCounts {
            added: 0,
            removed: 1,
            unchanged: 2,
            skipped: 1
        }
    );
    assert_eq!(changed.people.created, 0);
    assert_eq!(f.app.store.all_cards().unwrap().len(), cards);
    assert_eq!(f.app.card(story.id).unwrap().title, "Pay with a card");
    let comments_now = f.app.comments(story.id).unwrap();
    assert_eq!(comments_now.len(), comments);
    assert_eq!(comments_now[1].body, "We tried this in 2020.");
    let bug = f.card(project, "Refund fails twice");
    assert!(f.app.store.dependencies(bug.id).unwrap().is_empty());
    // The duplicate is closed by Moebius and stays closed although Jira has it open.
    assert!(bug.closed);
    assert!(f.card(project, "Plan the next sprint").closed);
}

#[tokio::test]
async fn a_jira_user_without_a_person_keeps_their_name_on_what_they_wrote() {
    let f = fixture().await;
    let project = f.project("Web shop");
    let report = f.import(project, Users::Names).await;
    assert_eq!(report.people.created, 0);
    assert_eq!(
        f.app.store.people().unwrap().len(),
        1,
        "only the first owner"
    );
    let story = f.card(project, "Pay with card");
    assert_eq!(story.author, "Ada Lovelace (Jira)");
    // Their assignee is not a person, so the card is given to no one.
    assert!(story.assignees.is_empty());
    let who: Vec<String> = f
        .app
        .comments(story.id)
        .unwrap()
        .into_iter()
        .map(|c| c.author)
        .collect();
    assert_eq!(
        who,
        [
            "Ada Lovelace (Jira)",
            "Old Colleague (Jira)",
            "Grace Hopper (Jira)"
        ]
    );
    // A name of Jira is no person: it cannot pass for one.
    assert!(!crate::app::is_person(&story.author));
}

#[tokio::test]
async fn a_jira_user_can_be_mapped_to_a_person_that_exists() {
    let f = fixture().await;
    let project = f.project("Web shop");
    let lea = f
        .app
        .store
        .add_person("lea", "Lea", crate::store::Role::Member)
        .unwrap();
    let map = BTreeMap::from([("ADA@example.com".to_string(), "@lea".to_string())]);
    f.import_mapped(project, Users::People, map).await.unwrap();
    let story = f.card(project, "Pay with card");
    assert_eq!(story.author, lea.author());
    assert!(f.app.store.person_by_handle("ada").unwrap().is_none());
    assert!(!f.app.store.person(lea.id).unwrap().unwrap().disabled);

    let nobody = BTreeMap::from([("g1".to_string(), "nobody".to_string())]);
    let refused = f.import_mapped(project, Users::People, nobody).await;
    assert!(matches!(refused, Err(AppError::Invalid(_))), "{refused:?}");
}

#[tokio::test]
async fn an_existing_kind_is_completed_and_a_taken_milestone_name_is_told_apart() {
    let f = fixture().await;
    let project = f.project("Web shop");
    f.app
        .add_kind(
            "Bug",
            &[FieldDef {
                name: "Severity".into(),
                field_type: FieldType::Text,
                options: vec![],
            }],
        )
        .unwrap();
    f.app
        .add_milestone(&MilestoneFields {
            name: "checkout".into(),
            target_date: "2026-11-30".into(),
            description: String::new(),
        })
        .unwrap();
    let report = f.import(project, Users::People).await;
    // The Bug kind has a text field Severity where Jira has a choice: the field stays, and its values are not imported.
    assert!(
        report.warnings.iter().any(|w| w.contains("Severity")),
        "{:?}",
        report.warnings
    );
    assert!(f.card(project, "Refund fails").fields.is_empty());
    assert_eq!(report.kinds.created, 3);
    assert!(
        f.app
            .store
            .milestone_by_name("Checkout (WEB-1)")
            .unwrap()
            .is_some()
    );
    assert_eq!(report.milestones.created, 1);
}

#[tokio::test]
async fn what_a_person_moved_stays_and_a_deleted_card_comes_back_whole() {
    let f = fixture().await;
    let project = f.project("Web shop");
    f.import(project, Users::People).await;
    let story = f.card(project, "Pay with card");
    let cycle = f.app.store.card_cycle(story.id).unwrap().unwrap();
    let other = f
        .app
        .add_cycle(
            &Actor::first_owner(),
            project,
            &CycleFields {
                name: "Mine".into(),
                starts_on: "2026-11-01".into(),
                ends_on: "2026-11-14".into(),
            },
        )
        .unwrap();
    f.app
        .set_card_cycle(&Actor::first_owner(), story.id, Some(other.id))
        .unwrap();
    f.app.store.set_card_milestones(story.id, &[]).unwrap();
    f.import(project, Users::People).await;
    assert_eq!(
        f.app.store.card_cycle(story.id).unwrap().unwrap().id,
        other.id
    );
    assert_ne!(cycle.id, other.id);
    assert!(f.app.store.card_milestones(story.id).unwrap().is_empty());

    // A card a person deleted is made again with its comments, its parent link and its blockers.
    let task = f.card(project, "Validate card number");
    f.app.store.delete_card(task.id, false).unwrap();
    let report = f.import(project, Users::People).await;
    assert_eq!(report.issues.created, 1);
    let back = f.card(project, "Validate card number");
    assert_eq!(back.parent, Some(story.number));
    let bug = f.card(project, "Refund fails");
    assert!(
        f.app
            .store
            .dependencies(bug.id)
            .unwrap()
            .contains(&back.number)
    );
}

#[tokio::test]
async fn a_milestone_without_a_date_in_jira_keeps_the_date_it_has() {
    let f = fixture().await;
    let project = f.project("Web shop");
    f.change("WEB-1", |fields| {
        fields["duedate"] = serde_json::Value::Null
    });
    f.import(project, Users::People).await;
    let checkout = f.app.store.milestone_by_name("Checkout").unwrap().unwrap();
    f.app
        .edit_milestone(
            checkout.id,
            &MilestoneFields {
                name: "Checkout".into(),
                target_date: "2027-03-01".into(),
                description: checkout.description.clone(),
            },
        )
        .unwrap();
    // The note was of the date the import chose; a date in Moebius is not written over by a day's passing.
    let again = f.import(project, Users::People).await;
    assert_eq!(
        f.app
            .store
            .milestone_by_name("Checkout")
            .unwrap()
            .unwrap()
            .target_date,
        "2027-03-01",
        "{again:?}"
    );
}

#[test]
fn removing_the_last_repository_of_a_project_with_an_import_works() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::in_memory(&dir.path().join("blobs")).unwrap();
    let repo = store.add_repository("web", "/web", "none", "main").unwrap();
    store
        .set_imported(repo.home_project_id, SOURCE, "issue", "WEB-1", 1, "")
        .unwrap();
    assert!(store.delete_repository(repo.id).unwrap());
}
