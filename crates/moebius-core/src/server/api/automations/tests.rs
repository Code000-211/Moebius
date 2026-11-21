//! The tests of the automations API: an automation is added, listed, changed and deleted; an older row is written to
//! a workflow file, keeps its place in the schedule and cannot fire twice over an upgrade; and a firing starts a chat
//! or a workflow run, or says why it started none.

use serde_json::json;

use super::*;
use crate::app::{App, cron, on_automation_schedule};
use crate::server::api::testing::{get, kinds, post, start, status, until};

async fn add(state: &ApiState, addr: &str, body: serde_json::Value) -> reqwest::Response {
    post(state, addr, "/v1/automations".into(), body).await
}

fn triage(prompt: &str) -> serde_json::Value {
    json!({ "name": "Triage", "schedule": "*/5 * * * *", "agent": "Coder", "prompt": prompt })
}

/// The harness configuration repository of `dir`, on a server that has a project to keep workflows in. Automations
/// added before it was there are rows without a project, which the migration gives one.
fn server_wide_harness(
    app: &App,
    dir: &std::path::Path,
) -> std::sync::Arc<crate::harness::repo::HarnessRepo> {
    if app.store.projects().unwrap().is_empty() {
        app.add_project("Home", "HOM").unwrap();
    }
    crate::harness::repo::testing::use_harness(app, dir)
}

/// What a start does to the rows of an older server: writes them to workflow files and moves the files to projects.
fn migrate(app: &App) {
    app.migrate_automations().unwrap();
    app.migrate_workflows_to_projects();
}

/// The project that holds the workflow file `key`.
fn holder(app: &App, key: &str) -> Option<crate::store::Project> {
    let root = app.harness_repo().unwrap().path().to_path_buf();
    app.store.projects().unwrap().into_iter().find(|project| {
        crate::workflows::definition::project_file_for(&project.name, key)
            .is_ok_and(|file| root.join(file).is_file())
    })
}

/// The workflow file `key`, read from the project that holds it.
fn file(app: &App, key: &str) -> crate::workflows::definition::WorkflowDefinition {
    let project = holder(app, key).unwrap_or_else(|| panic!("no project holds {key}"));
    crate::workflows::definition::read_in_project(
        app.harness_repo().unwrap().path(),
        &project.name,
        key,
    )
    .unwrap()
}

/// Where the file `key` is.
fn file_path(app: &App, key: &str) -> std::path::PathBuf {
    let project = holder(app, key).unwrap_or_else(|| panic!("no project holds {key}"));
    app.harness_repo()
        .unwrap()
        .path()
        .join(crate::workflows::definition::project_file_for(&project.name, key).unwrap())
}

/// Saves the file `key` in the project that holds it, else in the oldest project.
fn save(app: &App, key: &str, definition: &crate::workflows::definition::WorkflowDefinition) {
    let project = holder(app, key).unwrap_or_else(|| {
        let mut projects = app.store.projects().unwrap();
        projects.sort_by_key(|project| (project.created_at, project.id));
        projects.remove(0)
    });
    app.harness_repo()
        .unwrap()
        .save_project_workflow(
            app,
            &crate::decide::Actor::first_owner(),
            &project.name,
            key,
            definition,
        )
        .unwrap();
}

/// The local minute a minute after it is on the server.
fn a_minute_from_now(app: &App) -> time::PrimitiveDateTime {
    cron::parse_minute(&app.store.local_minute().unwrap()).unwrap() + time::Duration::minutes(1)
}

#[tokio::test]
async fn an_automation_is_added_listed_changed_and_deleted() {
    let (_dir, state, addr) = start().await;
    let added = add(&state, &addr, triage("Triage the new issues.")).await;
    assert_eq!(added.status(), 200);
    let added: AutomationView = added.json().await.unwrap();
    let a = &added.automation;
    assert_eq!(
        (
            a.name.as_str(),
            a.agent.as_str(),
            a.enabled,
            a.repository_id
        ),
        ("Triage", "Coder", true, None)
    );
    // It fires next within five minutes, on a minute that divides by five.
    let next = added.next_firing.unwrap();
    assert!(next > crate::store::now() - 60 && next <= crate::store::now() + 5 * 60);
    assert_eq!(next % 300, 0);
    assert!(added.last_firing.is_none());

    let listed: Vec<AutomationView> = get(&state, &addr, "/v1/automations").await;
    assert_eq!(listed, std::slice::from_ref(&added));

    let client = reqwest::Client::new();
    let put = |body: serde_json::Value| {
        client
            .put(format!("http://{addr}/v1/automations/{}", a.id))
            .bearer_auth(&*state.token)
            .json(&body)
            .send()
    };
    let mut off = triage("Triage the new issues.");
    off["enabled"] = json!(false);
    off["repository"] = json!(1);
    let changed: AutomationView = put(off).await.unwrap().json().await.unwrap();
    assert_eq!(
        (changed.automation.enabled, changed.automation.repository_id),
        (false, Some(1))
    );
    // While it is off it does not fire.
    assert_eq!(changed.next_firing, None);

    // What is wrong is said, and nothing is stored.
    for (body, says) in [
        (
            json!({ "name": "x", "schedule": "* * *", "agent": "Coder", "prompt": "p" }),
            "five fields",
        ),
        (
            json!({ "name": "x", "schedule": "* * * * *", "agent": "Nobody", "prompt": "p" }),
            "no agent",
        ),
        (
            json!({ "name": " ", "schedule": "* * * * *", "agent": "Coder", "prompt": "p" }),
            "name",
        ),
        (
            json!({ "name": "x", "schedule": "* * * * *", "agent": "Coder", "prompt": "" }),
            "prompt",
        ),
        (
            json!({ "name": "x", "schedule": "* * * * *", "agent": "Coder", "prompt": "p", "repository": 9 }),
            "not found",
        ),
    ] {
        let refused = add(&state, &addr, body).await;
        assert_ne!(refused.status(), 200);
        assert!(refused.text().await.unwrap().contains(says), "{says}");
    }
    assert_eq!(state.app.automations().unwrap().len(), 1);

    let deleted = client
        .delete(format!("http://{addr}/v1/automations/{}", a.id))
        .bearer_auth(&*state.token)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 200);
    let gone = client
        .get(format!("http://{addr}/v1/automations/{}", a.id))
        .bearer_auth(&*state.token)
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404);
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn a_firing_starts_a_chat_with_the_agent_which_ends_once_it_has_answered() {
    let (_dir, state, addr) = start().await;
    let app = &state.app;
    let added: AutomationView = add(&state, &addr, triage("Triage the new issues."))
        .await
        .json()
        .await
        .unwrap();
    let id = added.automation.id;
    let mut events = app.bus.subscribe();

    // Firing it starts its chat.
    let first = app.run_automation(&*state.runs, id).unwrap();
    assert_eq!(first.status, "started");
    let firings: Vec<Firing> = get(&state, &addr, &format!("/v1/automations/{id}/firings")).await;
    assert_eq!(firings.len(), 1);
    let firing = &firings[0];
    assert_eq!((firing.status.as_str(), firing.manual), ("started", true));
    let chat = firing.run_id.unwrap();
    let run = app.run(chat).unwrap();
    assert_eq!(
        (
            run.agent.as_str(),
            run.purpose.as_str(),
            run.origin.as_str()
        ),
        ("Coder", "chat", "automation")
    );
    let fired = loop {
        let event = events.recv().await.unwrap();
        if event.kind == "automation_fired" {
            break event;
        }
    };
    assert_eq!(fired.data["run"], chat);
    // The chat's status may say it waits a moment before its last event is stored: both are waited for.
    until("the agent has answered", || {
        status(app, chat) == "waiting" && kinds(app, chat).contains(&"agent_settled".to_string())
    })
    .await;
    let said = app.store.events(chat).unwrap();
    assert_eq!(said[0].data["message"], "Triage the new issues.");
    // It is listed with the chats.
    assert!(
        app.chats(false)
            .unwrap()
            .iter()
            .any(|c| c.id == chat && c.origin == "automation")
    );

    // The next check ends the chat that has answered: a run never waits for a person.
    on_automation_schedule(app, &*state.runs).unwrap();
    until("the chat has ended", || {
        app.run(chat).unwrap().ended_at.is_some()
    })
    .await;
    assert_eq!(status(app, chat), "succeeded");
    // The same check again fires nothing.
    on_automation_schedule(app, &*state.runs).unwrap();
    assert_eq!(app.firings(id).unwrap().len(), 1);
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn a_firing_due_while_the_chat_before_still_runs_is_skipped() {
    let (_dir, state, addr) = start().await;
    let app = &state.app;
    let added: AutomationView = add(&state, &addr, triage("Take your time [hang]"))
        .await
        .json()
        .await
        .unwrap();
    let id = added.automation.id;
    let path = format!("/v1/automations/{id}/run");
    let run = || post(&state, &addr, path.clone(), json!({}));
    let first: Firing = run().await.json().await.unwrap();
    assert_eq!((first.status.as_str(), first.manual), ("started", true));
    let chat = first.run_id.unwrap();
    until("the agent works", || status(app, chat) == "running").await;

    let second: Firing = run().await.json().await.unwrap();
    assert_eq!(second.status, "skipped");
    assert_eq!(second.run_id, None);
    assert!(second.reason.contains("still runs"), "{}", second.reason);
    let firings = app.firings(id).unwrap();
    assert_eq!(
        firings
            .iter()
            .map(|f| (f.status.as_str(), f.manual))
            .collect::<Vec<_>>(),
        [("skipped", true), ("started", true)]
    );
    assert_eq!(firings[1].chat_status.as_deref(), Some("running"));
    let shown: AutomationView = get(&state, &addr, &format!("/v1/automations/{id}")).await;
    assert_eq!(shown.last_firing.unwrap().status, "skipped");

    // Once the chat has ended, the automation fires again.
    app.end_chat(&*state.runs, chat).unwrap();
    until("the chat has ended", || {
        app.run(chat).unwrap().ended_at.is_some()
    })
    .await;
    let third: Firing = run().await.json().await.unwrap();
    assert_eq!(third.status, "started");
    app.end_chat(&*state.runs, third.run_id.unwrap()).unwrap();
    until("the chat has ended", || {
        app.run(third.run_id.unwrap()).unwrap().ended_at.is_some()
    })
    .await;
}

#[tokio::test]
async fn an_upgraded_automation_is_edited_as_a_workflow_file() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let added: AutomationView = add(&state, &addr, triage("Triage the new issues."))
        .await
        .json()
        .await
        .unwrap();
    let id = added.automation.id;

    let _harness = server_wide_harness(app, dir.path());
    migrate(app);
    let definition = file(app, "Triage");
    assert_eq!(definition.name, "Triage");
    assert_eq!(definition.trigger[0].kind, "schedule");
    assert_eq!(definition.trigger[0].params["cron"], "*/5 * * * *");
    assert_eq!(definition.nodes[0].kind, "chat");
    assert_eq!(
        definition.nodes[0].params["prompt"],
        "Triage the new issues."
    );

    // The old route changes the workflow file behind the page.
    let client = reqwest::Client::new();
    let mut changed = triage("Triage the next issues.");
    changed["enabled"] = json!(false);
    let put = client
        .put(format!("http://{addr}/v1/automations/{id}"))
        .bearer_auth(&*state.token)
        .json(&changed)
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 200);
    let definition = file(app, "Triage");
    assert_eq!(
        definition.nodes[0].params["prompt"],
        "Triage the next issues."
    );
    assert_eq!(definition.trigger[0].params["enabled"], false);
    let shown: AutomationView = get(&state, &addr, &format!("/v1/automations/{id}")).await;
    assert_eq!(shown.automation.prompt, "Triage the next issues.");
    assert!(!shown.automation.enabled);

    // The old delete route removes the workflow too.
    let client = reqwest::Client::new();
    let deleted = client
        .delete(format!("http://{addr}/v1/automations/{id}"))
        .bearer_auth(&*state.token)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 200);
    assert!(holder(app, "Triage").is_none());
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn a_repository_automation_fires_as_a_workflow_run() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let harness = server_wide_harness(app, dir.path());
    let repo = app
        .store
        .add_repository(
            "demo",
            crate::server::api::testing::origin(dir.path())
                .to_str()
                .unwrap(),
            "none",
            "main",
        )
        .unwrap();
    let mut body = triage("Triage the new issues.");
    body["repository"] = json!(repo.id);
    let added: AutomationView = add(&state, &addr, body).await.json().await.unwrap();
    let id = added.automation.id;
    let mut events = app.bus.subscribe();

    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    crate::server::schedule::on_workflow_schedules(app, Some(&harness), Some(&*state.runs), due)
        .unwrap();
    let firing = app.firings(id).unwrap().remove(0);
    assert_eq!(firing.status, "started");
    let workflow_run = firing
        .workflow_run_id
        .expect("the firing names its workflow run");
    // The chat node starts the chat on that run.
    until("the chat starts", || {
        app.firings(id)
            .unwrap()
            .first()
            .and_then(|firing| firing.run_id)
            .is_some()
    })
    .await;
    let chat = app.firings(id).unwrap()[0].run_id.unwrap();
    let fired = loop {
        let event = events.recv().await.unwrap();
        if event.kind == "automation_fired" {
            break event;
        }
    };
    assert_eq!(fired.data["workflow_run"], workflow_run);
    assert_eq!(fired.data["workflow"], "Triage");

    // The same due minute does not fire it a second time.
    crate::server::schedule::on_workflow_schedules(app, Some(&harness), Some(&*state.runs), due)
        .unwrap();
    app.end_chat(&*state.runs, chat).unwrap();
    until("the first chat ends", || {
        app.store
            .run(chat)
            .unwrap()
            .is_some_and(|run| run.ended_at.is_some())
    })
    .await;
    // The workflow file is what fires it, so a change to its chat node reaches the next chat.
    let mut definition = file(app, "Triage");
    definition.nodes[0].params["prompt"] = json!("Triage the next issues.");
    save(app, "Triage", &definition);
    let more_due = due + time::Duration::minutes(5);
    crate::server::schedule::on_workflow_schedules(
        app,
        Some(&harness),
        Some(&*state.runs),
        more_due,
    )
    .unwrap();
    until("the next chat starts", || {
        app.firings(id)
            .unwrap()
            .first()
            .is_some_and(|firing| firing.run_id.is_some())
    })
    .await;
    assert_eq!(app.firings(id).unwrap().len(), 2);
    let second = app.firings(id).unwrap()[0].run_id.unwrap();
    until("the changed prompt reaches the chat", || {
        app.store
            .events(second)
            .unwrap()
            .first()
            .is_some_and(|event| event.data["message"] == "Triage the next issues.")
    })
    .await;
    app.end_chat(&*state.runs, second).unwrap();
}

#[tokio::test]
async fn a_repository_automation_whose_chat_cannot_start_is_a_failed_firing() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let harness = server_wide_harness(app, dir.path());
    let repo = app
        .store
        .add_repository(
            "demo",
            crate::server::api::testing::origin(dir.path())
                .to_str()
                .unwrap(),
            "none",
            "main",
        )
        .unwrap();
    let mut body = triage("Triage the new issues.");
    body["repository"] = json!(repo.id);
    let added: AutomationView = add(&state, &addr, body).await.json().await.unwrap();
    let id = added.automation.id;
    migrate(app);

    // The chat node now asks for both a repository and the harness configuration repository, so the run
    // reaches it and cannot start the chat.
    let mut definition = file(app, "Triage");
    definition.nodes[0].params["harness"] = json!(true);
    save(app, "Triage", &definition);

    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    crate::server::schedule::on_workflow_schedules(app, Some(&harness), Some(&*state.runs), due)
        .unwrap();
    until("the firing fails", || {
        app.firings(id)
            .unwrap()
            .first()
            .is_some_and(|firing| firing.status == "failed")
    })
    .await;
    let firing = app.firings(id).unwrap().remove(0);
    assert_eq!(firing.run_id, None);
    assert!(
        firing.reason.contains("repository") || firing.reason.contains("harness"),
        "{}",
        firing.reason
    );
}

#[tokio::test]
async fn a_repository_automation_whose_repository_is_gone_is_a_failed_firing() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let harness = server_wide_harness(app, dir.path());
    let repo = app
        .store
        .add_repository(
            "demo",
            crate::server::api::testing::origin(dir.path())
                .to_str()
                .unwrap(),
            "none",
            "main",
        )
        .unwrap();
    let mut body = triage("Triage the new issues.");
    body["repository"] = json!(repo.id);
    add(&state, &addr, body).await;
    migrate(app);

    // The file names a repository that the server does not have: the firing fails rather than starting a chat in a
    // directory that belongs to no repository.
    let mut definition = file(app, "Triage");
    definition.trigger[0].params["repository"] = json!("gone");
    save(app, "Triage", &definition);
    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    crate::server::schedule::on_workflow_schedules(app, Some(&harness), Some(&*state.runs), due)
        .unwrap();
    let automation = app.store.automations().unwrap().remove(0);
    let firing = app.firings(automation.id).unwrap().remove(0);
    assert_eq!(firing.status, "failed");
    assert_eq!(firing.run_id, None);
    assert_eq!(firing.workflow_run_id, None);
    assert!(
        firing.reason.contains("gone") && firing.reason.contains("repository"),
        "{}",
        firing.reason
    );
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn an_upgraded_automation_still_fires_once() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let added: AutomationView = add(&state, &addr, triage("Triage the new issues."))
        .await
        .json()
        .await
        .unwrap();
    let id = added.automation.id;
    server_wide_harness(app, dir.path());
    migrate(app);

    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    // The row's own clock says a firing came due long ago, and the workflow file holds the automation: only the
    // workflow fires it, so it fires once rather than once from each clock.
    app.store
        .set_automations_checked("2000-01-01 00:00")
        .unwrap();
    on_automation_schedule(app, &*state.runs).unwrap();
    assert!(app.firings(id).unwrap().is_empty());
    let harness = app.harness_repo().unwrap();
    crate::server::schedule::on_workflow_schedules(app, Some(harness), Some(&*state.runs), due)
        .unwrap();
    crate::server::schedule::on_workflow_schedules(app, Some(harness), Some(&*state.runs), due)
        .unwrap();
    on_automation_schedule(app, &*state.runs).unwrap();
    let firings = app.firings(id).unwrap();
    assert_eq!(firings.len(), 1);
    assert_eq!(firings[0].status, "started");
    app.end_chat(&*state.runs, firings[0].run_id.unwrap())
        .unwrap();
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn an_automation_edited_into_an_ordinary_workflow_does_not_also_fire_from_its_row() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let mut body = triage("Triage the new issues.");
    body["schedule"] = json!("* * * * *");
    let added: AutomationView = add(&state, &addr, body).await.json().await.unwrap();
    let id = added.automation.id;
    let _harness = server_wide_harness(app, dir.path());
    migrate(app);

    // A person edits the file into an ordinary workflow by adding a node to it.
    let path = file_path(app, "Triage");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str(
        "\n[[nodes]]\nid = \"after\"\nkind = \"chat\"\nparams = { prompt = \"Ask again.\" }\n\
         \n[[edges]]\nfrom = \"chat\"\nto = \"after\"\nport = \"out\"\n",
    );
    std::fs::write(&path, text).unwrap();
    assert!(!crate::app::is_automation(&file(app, "Triage")));

    app.store
        .set_automations_checked("2000-01-01 00:00")
        .unwrap();
    on_automation_schedule(app, &*state.runs).unwrap();
    assert!(app.firings(id).unwrap().is_empty());
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn an_upgraded_automation_with_a_name_that_is_not_a_file_name_keeps_firing() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let mut body = triage("Triage the new issues.");
    body["name"] = json!("Nightly triage");
    let added: AutomationView = add(&state, &addr, body).await.json().await.unwrap();
    let id = added.automation.id;

    let harness = server_wide_harness(app, dir.path());
    migrate(app);
    let key = format!("automation-{id}");
    let definition = file(app, &key);
    assert_eq!(definition.name, "Nightly triage");
    assert_eq!(definition.trigger[0].kind, "schedule");
    assert_eq!(definition.nodes[0].kind, "chat");

    // The old route edits that file, even though the name cannot be one.
    let client = reqwest::Client::new();
    let mut changed = triage("Triage the next issues.");
    changed["name"] = json!("Nightly triage");
    let put = client
        .put(format!("http://{addr}/v1/automations/{id}"))
        .bearer_auth(&*state.token)
        .json(&changed)
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 200);
    let definition = file(app, &key);
    assert_eq!(
        definition.nodes[0].params["prompt"],
        "Triage the next issues."
    );

    // Its workflow file fires it, and the same minute is not fired twice.
    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    crate::server::schedule::on_workflow_schedules(app, Some(&harness), Some(&*state.runs), due)
        .unwrap();
    crate::server::schedule::on_workflow_schedules(app, Some(&harness), Some(&*state.runs), due)
        .unwrap();
    let firings = app.firings(id).unwrap();
    assert_eq!(firings.len(), 1);
    assert_eq!(firings[0].status, "started");
    app.end_chat(&*state.runs, firings[0].run_id.unwrap())
        .unwrap();
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn an_automation_whose_workflow_file_is_deleted_stops_firing() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let mut body = triage("Triage the new issues.");
    body["schedule"] = json!("* * * * *");
    let added: AutomationView = add(&state, &addr, body).await.json().await.unwrap();
    let id = added.automation.id;
    let harness = server_wide_harness(app, dir.path());
    migrate(app);

    // A person deletes the workflow's file: the automation no longer fires, from its row or anywhere else.
    std::fs::remove_file(file_path(app, "Triage")).unwrap();
    app.store
        .set_automations_checked("2000-01-01 00:00")
        .unwrap();
    on_automation_schedule(app, &*state.runs).unwrap();
    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    crate::server::schedule::on_workflow_schedules(
        app,
        Some(harness.as_ref()),
        Some(&*state.runs),
        due,
    )
    .unwrap();
    assert!(app.firings(id).unwrap().is_empty());
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn an_automation_without_a_harness_fires_from_its_row() {
    let (_dir, state, addr) = start().await;
    let app = &state.app;
    let mut body = triage("Triage the new issues.");
    body["schedule"] = json!("* * * * *");
    let added: AutomationView = add(&state, &addr, body).await.json().await.unwrap();
    let id = added.automation.id;
    app.store
        .set_automations_checked("2000-01-01 00:00")
        .unwrap();

    on_automation_schedule(app, &*state.runs).unwrap();
    let firings = app.firings(id).unwrap();
    assert_eq!(firings.len(), 1);
    assert_eq!(firings[0].status, "started");
    let chat = firings[0].run_id.expect("the chat it started");
    app.end_chat(&*state.runs, chat).unwrap();
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn an_upgrade_does_not_fire_the_minute_it_happens_in_again() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let mut body = triage("Triage the new issues.");
    body["schedule"] = json!("30 3 * * *");
    let added: AutomationView = add(&state, &addr, body).await.json().await.unwrap();
    let id = added.automation.id;
    app.store
        .set_automations_checked("2026-11-09 03:30")
        .unwrap();

    let harness = server_wide_harness(app, dir.path());
    migrate(app);
    assert_eq!(
        app.store
            .project_schedule_firing(holder(app, "Triage").unwrap().id, "Triage")
            .unwrap()
            .as_deref(),
        Some("2026-11-09 03:30")
    );
    // The upgrade happens at 03:31, a minute after that firing; it is not fired again.
    crate::server::schedule::on_workflow_schedules(
        app,
        Some(&harness),
        Some(&*state.runs),
        cron::parse_minute("2026-11-09 03:31").unwrap(),
    )
    .unwrap();
    assert!(app.firings(id).unwrap().is_empty());
    // The next day's firing does happen.
    crate::server::schedule::on_workflow_schedules(
        app,
        Some(&harness),
        Some(&*state.runs),
        cron::parse_minute("2026-11-10 03:31").unwrap(),
    )
    .unwrap();
    let firings = app.firings(id).unwrap();
    assert_eq!(firings.len(), 1);
    assert_eq!(firings[0].status, "started");
    app.end_chat(&*state.runs, firings[0].run_id.unwrap())
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn an_automation_whose_name_a_workflow_holds_gets_a_file_of_its_own_and_keeps_firing() {
    let (dir, state, _addr) = start().await;
    let app = &state.app;
    let harness = server_wide_harness(app, dir.path());
    // A person's manual workflow already has the file the row of the same name would want.
    let ordinary = crate::workflows::definition::parse(
        "Nightly",
        r#"
name = "Nightly"
description = "Started by a person."

[[trigger]]
kind = "manual"

[[nodes]]
id = "build"
kind = "test"

[[edges]]
from = "trigger"
to = "build"
port = "out"
"#,
    )
    .unwrap();
    // As an older server kept it: a file of the server-wide folder, which the migration moves with the automation's.
    std::fs::create_dir_all(harness.path().join("workflows")).unwrap();
    std::fs::write(
        harness.path().join("workflows/Nightly.toml"),
        crate::workflows::definition::render("Nightly", &ordinary).unwrap(),
    )
    .unwrap();

    // A row saved before the upgrade, with no workflow file of its own.
    let row = app
        .store
        .add_automation(
            &crate::store::NewAutomation {
                name: "Nightly".into(),
                schedule: "*/5 * * * *".into(),
                agent: "Coder".into(),
                prompt: "Nightly triage.".into(),
                repository: None,
                harness: false,
                enabled: true,
            },
            &app.store.local_minute().unwrap(),
            None,
        )
        .unwrap();
    migrate(app);

    // The person's workflow keeps its file, and the automation has one of its own beside it.
    let kept = file(app, "Nightly");
    assert!(!crate::app::is_automation(&kept));
    assert_eq!(kept.trigger[0].kind, "manual");
    let key = format!("automation-{}", row.id);
    let own = file(app, &key);
    assert_eq!(own.name, "Nightly");
    assert!(crate::app::is_automation(&own));

    // The page still shows the automation once, as its row.
    let listed = app.automations().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].automation.id, row.id);

    // It fires from its file, and its old row clock does not fire it a second time.
    app.store
        .set_automations_checked("2000-01-01 00:00")
        .unwrap();
    on_automation_schedule(app, &*state.runs).unwrap();
    assert!(app.firings(row.id).unwrap().is_empty());
    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    crate::server::schedule::on_workflow_schedules(
        app,
        Some(harness.as_ref()),
        Some(&*state.runs),
        due,
    )
    .unwrap();
    let firings = app.firings(row.id).unwrap();
    assert_eq!(firings.len(), 1);
    assert_eq!(firings[0].status, "started");
    app.end_chat(&*state.runs, firings[0].run_id.unwrap())
        .unwrap();
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn two_automations_of_the_same_name_keep_a_file_each_and_both_fire_once() {
    let (dir, state, _addr) = start().await;
    let app = &state.app;
    let first = app
        .store
        .add_automation(
            &crate::store::NewAutomation {
                name: "Triage".into(),
                schedule: "*/5 * * * *".into(),
                agent: "Coder".into(),
                prompt: "Triage the first issues.".into(),
                repository: None,
                harness: false,
                enabled: true,
            },
            &app.store.local_minute().unwrap(),
            None,
        )
        .unwrap();
    let second = app
        .store
        .add_automation(
            &crate::store::NewAutomation {
                name: "Triage".into(),
                schedule: "*/5 * * * *".into(),
                agent: "Coder".into(),
                prompt: "Triage the second issues.".into(),
                repository: None,
                harness: false,
                enabled: true,
            },
            &app.store.local_minute().unwrap(),
            None,
        )
        .unwrap();
    let harness = server_wide_harness(app, dir.path());
    migrate(app);

    // Each row keeps its own file, with its own prompt.
    let first_file = file(app, "Triage");
    assert_eq!(
        first_file.nodes[0].params["prompt"],
        "Triage the first issues."
    );
    let second_key = format!("automation-{}", second.id);
    let second_file = file(app, &second_key);
    assert_eq!(
        second_file.nodes[0].params["prompt"],
        "Triage the second issues."
    );

    // The page shows both, each against its own row.
    let listed = app.automations().unwrap();
    assert_eq!(listed.len(), 2);
    let mut ids = listed
        .iter()
        .map(|view| view.automation.id)
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(ids, vec![first.id, second.id]);

    // Only the workflow files fire them, once each, even when the old row clock also runs.
    app.store
        .set_automations_checked("2000-01-01 00:00")
        .unwrap();
    on_automation_schedule(app, &*state.runs).unwrap();
    assert!(app.firings(first.id).unwrap().is_empty());
    assert!(app.firings(second.id).unwrap().is_empty());
    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    for _ in 0..2 {
        crate::server::schedule::on_workflow_schedules(
            app,
            Some(&harness),
            Some(&*state.runs),
            due,
        )
        .unwrap();
        on_automation_schedule(app, &*state.runs).unwrap();
    }
    for id in [first.id, second.id] {
        let firings = app.firings(id).unwrap();
        assert_eq!(firings.len(), 1, "automation {id}");
        assert_eq!(firings[0].status, "started");
        app.end_chat(&*state.runs, firings[0].run_id.unwrap())
            .unwrap();
    }
}

#[tokio::test]
async fn an_automation_added_with_a_name_another_keeps_does_not_replace_its_file() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let _harness = server_wide_harness(app, dir.path());

    let first: AutomationView = add(&state, &addr, triage("Triage the first issues."))
        .await
        .json()
        .await
        .unwrap();
    let mut other = triage("Triage the second issues.");
    other["schedule"] = json!("*/10 * * * *");
    let second: AutomationView = add(&state, &addr, other).await.json().await.unwrap();
    assert_ne!(first.automation.id, second.automation.id);

    let first_file = file(app, "Triage");
    assert_eq!(
        first_file.nodes[0].params["prompt"],
        "Triage the first issues."
    );
    assert_eq!(first_file.trigger[0].params["cron"], "*/5 * * * *");
    let second_key = format!("automation-{}", second.automation.id);
    let second_file = file(app, &second_key);
    assert_eq!(
        second_file.nodes[0].params["prompt"],
        "Triage the second issues."
    );
    assert_eq!(second_file.trigger[0].params["cron"], "*/10 * * * *");

    // The page lists both, and changing one through the old route leaves the other's file alone.
    let listed: Vec<AutomationView> = get(&state, &addr, "/v1/automations").await;
    assert_eq!(listed.len(), 2);
    let client = reqwest::Client::new();
    let changed = client
        .put(format!(
            "http://{addr}/v1/automations/{}",
            first.automation.id
        ))
        .bearer_auth(&*state.token)
        .json(&triage("Triage the changed issues."))
        .send()
        .await
        .unwrap();
    assert_eq!(changed.status(), 200);
    let first_file = file(app, "Triage");
    assert_eq!(
        first_file.nodes[0].params["prompt"],
        "Triage the changed issues."
    );
    let second_file = file(app, &second_key);
    assert_eq!(
        second_file.nodes[0].params["prompt"],
        "Triage the second issues."
    );

    // The old delete route takes only the workflow of the automation it names.
    let deleted = client
        .delete(format!(
            "http://{addr}/v1/automations/{}",
            second.automation.id
        ))
        .bearer_auth(&*state.token)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 200);
    assert!(holder(app, &second_key).is_none());
    assert!(file_path(app, "Triage").exists());
}

/// An automation of the repository `demo` as an older server stored it, migrated to the project that owns the
/// repository: its row has the project, and its file is in the project's folder.
async fn migrated(
    dir: &std::path::Path,
    state: &ApiState,
    addr: &str,
    prompt: &str,
) -> (i64, std::sync::Arc<crate::harness::repo::HarnessRepo>) {
    let app = &state.app;
    let repo = app
        .store
        .add_repository(
            "demo",
            crate::server::api::testing::origin(dir).to_str().unwrap(),
            "none",
            "main",
        )
        .unwrap();
    let mut body = triage(prompt);
    body["repository"] = json!(repo.id);
    let added: AutomationView = add(state, addr, body).await.json().await.unwrap();
    let id = added.automation.id;
    let harness = server_wide_harness(app, dir);
    migrate(app);
    let project = holder(app, "Triage").expect("the migration moved the file to a project");
    assert_eq!(app.store.automation_project(id).unwrap(), Some(project.id));
    (id, harness)
}

/// The project copy of a migrated automation is the only one: saving it writes no server-wide file, no second
/// schedule row, and the next start has nothing left to migrate, so each occurrence fires once with the saved prompt.
#[cfg(unix)]
#[tokio::test]
async fn saving_a_migrated_automation_changes_its_project_file_and_nothing_server_wide() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let (id, harness) = migrated(dir.path(), &state, &addr, "Triage the old issues.").await;
    let project = holder(app, "Triage").unwrap();

    let mut changed = triage("Triage the new issues.");
    changed["repository"] = json!(app.automation(id).unwrap().automation.repository_id);
    let put = reqwest::Client::new()
        .put(format!("http://{addr}/v1/automations/{id}"))
        .bearer_auth(&*state.token)
        .json(&changed)
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 200, "{}", put.text().await.unwrap());

    assert!(
        !harness
            .path()
            .join("workflows")
            .join("Triage.toml")
            .exists()
    );
    assert!(app.store.server_wide_schedule_names().unwrap().is_empty());
    assert_eq!(
        file(app, "Triage").nodes[0].params["prompt"],
        "Triage the new issues."
    );
    assert!(app.workflows_awaiting_a_project().is_empty());
    // A start again has nothing to defer, and the file stays where it is.
    migrate(app);
    assert!(app.workflows_awaiting_a_project().is_empty());
    assert!(
        !harness.path().join("workflows").exists()
            || !harness.path().join("workflows/Triage.toml").exists()
    );

    // One occurrence fires once, with the saved prompt, as a run of the project.
    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    for _ in 0..2 {
        crate::server::schedule::on_workflow_schedules(
            app,
            Some(&harness),
            Some(&*state.runs),
            due,
        )
        .unwrap();
    }
    let firings = app.firings(id).unwrap();
    assert_eq!(firings.len(), 1);
    let run = app
        .store
        .workflow_run(firings[0].workflow_run_id.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(run.project_id, Some(project.id));
    assert_eq!(app.store.workflow_runs(None, None, 10).unwrap().len(), 1);
    until("the chat starts", || {
        app.firings(id).unwrap()[0].run_id.is_some()
    })
    .await;
    app.end_chat(&*state.runs, app.firings(id).unwrap()[0].run_id.unwrap())
        .unwrap();
}

/// Deleting a migrated automation removes its project file, so that nothing keeps firing without a row.
#[tokio::test]
async fn deleting_a_migrated_automation_removes_its_project_file() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let (id, harness) = migrated(dir.path(), &state, &addr, "Triage the old issues.").await;

    let deleted = reqwest::Client::new()
        .delete(format!("http://{addr}/v1/automations/{id}"))
        .bearer_auth(&*state.token)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 200);
    assert!(holder(app, "Triage").is_none(), "the project file is gone");

    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    let started = crate::server::schedule::on_workflow_schedules(
        app,
        Some(&harness),
        Some(&*state.runs),
        due,
    )
    .unwrap();
    assert!(started.is_empty());
    assert!(app.store.workflow_runs(None, None, 10).unwrap().is_empty());
}

/// The project file is what the list shows, what a scheduled firing and "Run now" run, and a firing records its
/// row, its event and its project; a firing due while the chat before still runs is skipped.
#[cfg(unix)]
#[tokio::test]
async fn a_migrated_automation_is_read_and_fired_from_its_project_file() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    let (id, harness) = migrated(dir.path(), &state, &addr, "Take your time [hang]").await;
    let project = holder(app, "Triage").unwrap();

    // A person edits the file in the project: the page, the list and the next firing all follow it.
    let mut definition = file(app, "Triage");
    definition.nodes[0].params["prompt"] = json!("Triage the edited issues. [hang]");
    save(app, "Triage", &definition);
    let shown: AutomationView = get(&state, &addr, &format!("/v1/automations/{id}")).await;
    assert_eq!(shown.automation.prompt, "Triage the edited issues. [hang]");
    assert_eq!(shown.automation.project_id, Some(project.id));
    let listed: Vec<AutomationView> = get(&state, &addr, "/v1/automations").await;
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].automation.prompt,
        "Triage the edited issues. [hang]"
    );
    let in_project: Vec<AutomationView> = get(
        &state,
        &addr,
        &format!("/v1/automations?project={}", project.id),
    )
    .await;
    assert_eq!(in_project.len(), 1);
    let elsewhere: Vec<AutomationView> = get(
        &state,
        &addr,
        &format!("/v1/automations?project={}", project.id + 99),
    )
    .await;
    assert!(elsewhere.is_empty());

    // A scheduled firing records a firing row, the event and the project of its run.
    let mut events = app.bus.subscribe();
    let due = a_minute_from_now(app) + time::Duration::minutes(5);
    crate::server::schedule::on_workflow_schedules(app, Some(&harness), Some(&*state.runs), due)
        .unwrap();
    let firing = app.firings(id).unwrap().remove(0);
    assert_eq!(firing.status, "started");
    let run = app
        .store
        .workflow_run(firing.workflow_run_id.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(run.project_id, Some(project.id));
    let fired = loop {
        let event = events.recv().await.unwrap();
        if event.kind == "automation_fired" {
            break event;
        }
    };
    assert_eq!(fired.data["workflow_run"], run.id);
    until("the chat starts", || {
        app.firings(id).unwrap()[0].run_id.is_some()
    })
    .await;
    let chat = app.firings(id).unwrap()[0].run_id.unwrap();
    until("the agent works", || status(app, chat) == "running").await;
    assert_eq!(
        app.store.events(chat).unwrap()[0].data["message"],
        "Triage the edited issues. [hang]"
    );

    // The next occurrence is due while that chat still runs: skipped, not started.
    let later = due + time::Duration::minutes(5);
    crate::server::schedule::on_workflow_schedules(app, Some(&harness), Some(&*state.runs), later)
        .unwrap();
    let skipped = app.firings(id).unwrap().remove(0);
    assert_eq!(skipped.status, "skipped");
    assert!(skipped.reason.contains("still runs"), "{}", skipped.reason);

    // "Run now" reads the project file too, and is skipped for the same reason.
    let now = app.run_automation(&*state.runs, id).unwrap();
    assert_eq!(now.status, "skipped");
    app.end_chat(&*state.runs, chat).unwrap();
    until("the chat has ended", || {
        app.run(chat).unwrap().ended_at.is_some()
    })
    .await;
    let manual = app.run_automation(&*state.runs, id).unwrap();
    assert_eq!(manual.status, "started");
    let manual_run = app
        .store
        .workflow_run(manual.workflow_run_id.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(manual_run.project_id, Some(project.id));
    until("the chat starts", || {
        app.firings(id).unwrap()[0].run_id.is_some()
    })
    .await;
    app.end_chat(&*state.runs, app.firings(id).unwrap()[0].run_id.unwrap())
        .unwrap();
}

/// An automation is added to the project that is named, which the person must see, and a different project is
/// refused when it is changed.
#[tokio::test]
async fn an_automation_belongs_to_the_project_it_is_added_to() {
    let (dir, state, addr) = start().await;
    let app = &state.app;
    server_wide_harness(app, dir.path());
    let other = app.add_project("Other", "OTH").unwrap();
    let mut body = triage("Triage the new issues.");
    body["project"] = json!(other.id);
    let added: AutomationView = add(&state, &addr, body).await.json().await.unwrap();
    assert_eq!(added.automation.project_id, Some(other.id));
    assert!(holder(app, "Triage").is_some_and(|project| project.id == other.id));

    let mut moved = triage("Triage the new issues.");
    moved["project"] = json!(other.id + 1);
    let refused = reqwest::Client::new()
        .put(format!(
            "http://{addr}/v1/automations/{}",
            added.automation.id
        ))
        .bearer_auth(&*state.token)
        .json(&moved)
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 422);
    assert!(refused.text().await.unwrap().contains("another project"));

    // A project that does not exist is not found.
    let mut missing = triage("Another.");
    missing["project"] = json!(other.id + 50);
    assert_eq!(add(&state, &addr, missing).await.status(), 404);
}
