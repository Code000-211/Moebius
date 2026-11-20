//! The tests of the check of a private project, and the fixture they share: a private project with one thing of every
//! guarded kind, and the people who see it or not.

use serde_json::{Value, json};

use super::*;
use crate::bus::{AppEvent, Event};
use crate::server::api::roles::{ROUTES, access_of};
use crate::server::api::testing::start;
use crate::server::api::{ApiState, TOKEN_PREFIX};
use crate::store::{
    Budget, BudgetKind, NewAutomation, NewJudgement, NewLesson, OwnEdits, Replay, Role,
};

/// The ids of one thing of every guarded kind, all of a private project, and the tokens of the people.
struct Private {
    ids: Vec<(&'static str, i64)>,
    card: i64,
    run: i64,
    lesson: i64,
    project: i64,
    repository: i64,
    workflow_run: i64,
    /// A member of the organisation and of the project.
    lea: String,
    /// A member of the organisation who is not in the project.
    ben: String,
    /// An owner who is not in the project.
    olga: String,
}

fn token_of(state: &ApiState, handle: &str, role: Role, n: usize) -> String {
    let person = state.app.store.add_person(handle, handle, role).unwrap();
    let token = format!("{TOKEN_PREFIX}{n:064}");
    state
        .app
        .store
        .add_access_token(person.id, "test", &token)
        .unwrap();
    token
}

/// Makes the project of the test server's repository private, with one thing of every kind in it.
fn private(state: &ApiState) -> Private {
    let store = &state.app.store;
    let repository = store.repositories().unwrap()[0].clone();
    let card = store.cards(repository.id).unwrap()[0].clone();
    let run = store
        .add_run(Some(card.id), "Coder", "test", "code")
        .unwrap();
    let chat = store.add_run(None, "Moebius", "fake", "chat").unwrap();
    store.set_run_repository(chat.id, repository.id).unwrap();
    let change = store
        .add_change(repository.id, Some(card.id), "moebius/secret", None, "abc")
        .unwrap();
    let item = store
        .add_inbox_item("question", Some(card.id), "Secret question", &json!({}))
        .unwrap();
    let lesson = store
        .add_lesson(&NewLesson {
            repository_id: repository.id,
            kind: "rejection".into(),
            situation: "Secret work".into(),
            lesson: "The reviewer wanted a test".into(),
            advice: "Add a test".into(),
            cards: vec![card.id],
            retro_date: None,
        })
        .unwrap()
        .unwrap();
    let judgement = store
        .add_judgement(&NewJudgement {
            kind: "merge".into(),
            choice: "merge".into(),
            decider: "none".into(),
            card_id: Some(card.id),
            ..Default::default()
        })
        .unwrap();
    let grilling = store
        .add_grilling(chat.id, repository.id, &[], Some("report"), "reported", "")
        .unwrap();
    store
        .add_retro(
            "2026-03-10",
            &json!({"date": "2026-03-10", "errors": [
                {"kind": "run_failed", "run": run.id, "card": card.id, "agent": "Coder", "text": "secret"},
                {"kind": "run_failed", "run": null, "card": null, "agent": null, "text": "public"}
            ], "merged": []}),
            "proposed",
        )
        .unwrap();
    let proposal = store
        .add_own_proposal(
            "2026-03-10",
            "retro/secret",
            "Secret settings",
            "",
            &[card.id],
            &OwnEdits {
                kind: "repository".into(),
                diff: String::new(),
                edits: json!({}),
                repository: Some(repository.id),
            },
        )
        .unwrap();
    let automation = store
        .add_automation(
            &NewAutomation {
                name: "Secret nightly".into(),
                schedule: "0 3 * * *".into(),
                agent: "Moebius".into(),
                prompt: "Read the secret files".into(),
                repository: Some(repository.id),
                harness: false,
                enabled: false,
            },
            "",
        )
        .unwrap();
    let research = store
        .add_research(
            repository.id,
            "Faster secret builds",
            "make bench",
            "lower",
            Budget {
                kind: BudgetKind::Tokens,
                amount: 1000,
            },
        )
        .unwrap();
    let workflow_run = store
        .add_workflow_run("release", repository.id, "manual", &json!({"secret": true}))
        .unwrap();
    store
        .add_inbox_item(
            "workflow_approval",
            None,
            "Approve the secret release",
            &json!({"run": workflow_run.id, "prompt": "secret"}),
        )
        .unwrap();
    store
        .add_replay(Replay {
            id: 0,
            judgement: judgement.id,
            run: run.id,
            exit: Some(0),
            timed_out: false,
            output: "secret output".into(),
            created: vec![],
            changed: vec![],
            deleted: vec![],
            at: 0,
        })
        .unwrap();
    let project = repository.home_project_id;
    let key = store.project(project).unwrap().unwrap().key;
    let wiki_proposal = store
        .add_inbox_item(
            crate::wiki::WIKI_PROPOSAL,
            None,
            "Wiki: a secret page",
            &json!({"page": format!("projects/{key}/Plans")}),
        )
        .unwrap();
    let request = store.add_request(project, 1, "A secret request").unwrap();
    let export = state.app.data_dir.join("exports/7");
    std::fs::create_dir_all(&export).unwrap();
    std::fs::write(
        export.join("export.json"),
        json!({"id": 7, "at": 0, "kinds": [], "private": [project]}).to_string(),
    )
    .unwrap();
    // An export from before projects could be private may hold anything.
    let old = state.app.data_dir.join("exports/8");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(
        old.join("export.json"),
        json!({"id": 8, "at": 0, "kinds": []}).to_string(),
    )
    .unwrap();
    std::fs::write(export.join("merge.jsonl"), "{}\n").unwrap();

    let lea = token_of(state, "lea", Role::Member, 1);
    let ben = token_of(state, "ben", Role::Member, 2);
    let olga = token_of(state, "olga", Role::Owner, 3);
    let lea_id = store.person_by_handle("lea").unwrap().unwrap().id;
    store.set_project_access(project, true, &[lea_id]).unwrap();
    Private {
        ids: vec![
            ("cards", card.id),
            ("inbox", item.id),
            ("changes", change.id),
            ("runs", run.id),
            ("chats", chat.id),
            ("repositories", repository.id),
            ("projects", project),
            ("lessons", lesson.id),
            ("judgements", judgement.id),
            ("grillings", grilling.id),
            ("proposals", proposal),
            ("exports", 7),
            ("automations", automation.id),
            ("research", research.id),
            (WORKFLOW_RUNS, workflow_run.id),
            (WIKI_PROPOSALS, wiki_proposal.id),
            ("requests", request.id),
        ],
        card: card.id,
        run: run.id,
        lesson: lesson.id,
        project,
        repository: repository.id,
        workflow_run: workflow_run.id,
        lea,
        ben,
        olga,
    }
}

fn store_name(state: &ApiState, repository: i64) -> String {
    state
        .app
        .store
        .repository(repository)
        .unwrap()
        .unwrap()
        .name
}

async fn call(addr: &str, token: &str, method: &str, path: &str) -> (u16, Value) {
    let mut request = reqwest::Client::new()
        .request(method.parse().unwrap(), format!("http://{addr}{path}"))
        .bearer_auth(token);
    if matches!(method, "POST" | "PUT" | "PATCH") {
        request = request.json(&json!({}));
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

/// `route` with its thing's id put in, and every other parameter filled with something plain.
fn filled(route: &str, id: i64) -> String {
    let mut first = true;
    route
        .split('/')
        .map(|part| {
            if !part.starts_with('{') {
                part.to_string()
            } else if std::mem::take(&mut first) {
                id.to_string()
            } else if part == "{*path}" {
                "README.md".into()
            } else if part == "{file}" {
                "merge.jsonl".into()
            } else {
                "1".into()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Every route that names a card, inbox item, change, run, chat, repository, project, lesson, judgement, grilling,
/// proposal or export by its id answers `404` for one of a private project to someone who is not its member: to an
/// owner, and to a member wherever their role lets them use the route at all. Only a project's own record routes
/// answer an owner, who names its members.
#[tokio::test]
async fn every_route_of_a_private_project_s_things_is_404_to_a_non_member() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let mut guarded = 0;
    let mut wrong = Vec::new();
    for (method, route, _) in ROUTES {
        let Some((kind, _)) = super::guarded(route) else {
            continue;
        };
        let (_, id) = private
            .ids
            .iter()
            .find(|(k, _)| *k == kind)
            .unwrap_or_else(|| panic!("the test makes no {kind}"));
        guarded += 1;
        let path = filled(route, *id);
        let record = RECORD_ROUTES.contains(route);
        let (status, body) = call(&addr, &private.olga, method, &path).await;
        if (status == 404) == record {
            wrong.push(format!("owner {method} {route}: {status} {body}"));
        }
        let member_may = access_of(&method.parse().unwrap(), route)
            .and_then(|a| a.role)
            .is_some_and(|role| role != Role::Owner);
        let (status, body) = call(&addr, &private.ben, method, &path).await;
        let expected = if member_may { 404 } else { 403 };
        if status != expected {
            wrong.push(format!("member {method} {route}: {status} {body}"));
        }
    }
    assert!(guarded > 70, "only {guarded} routes were called");
    assert!(
        wrong.is_empty(),
        "answered a non-member:\n{}",
        wrong.join("\n")
    );

    // No other spelling of an id gets past: `%31` is decoded to `1` by the router.
    let spelled: String = private
        .card
        .to_string()
        .bytes()
        .map(|b| format!("%{b:02X}"))
        .collect();
    for path in [
        format!("/v1/cards/{spelled}"),
        format!("/v1/cards/{spelled}/comments"),
        format!("/v1/cards/+{}", private.card),
        format!("/v1/cards/0{}", private.card),
    ] {
        let (status, body) = call(&addr, &private.ben, "GET", &path).await;
        assert!(status == 404 || status == 400, "{path}: {status} {body}");
        assert_ne!(body["id"], json!(private.card), "{path}");
    }
    // A card named by its key is checked by its route.
    let card = state.app.store.card(private.card).unwrap().unwrap();
    let project = state.app.store.project(card.project_id).unwrap().unwrap();
    let by_key = format!("/v1/cards/by-key/{}-{}", project.key, card.project_number);
    assert_eq!(call(&addr, &private.ben, "GET", &by_key).await.0, 404);
    assert_eq!(call(&addr, &private.olga, "GET", &by_key).await.0, 404);
    assert_eq!(call(&addr, &private.lea, "GET", &by_key).await.0, 200);
    // Nothing new is started in what a non-member does not see.
    let response = reqwest::Client::new()
        .post(format!("http://{addr}/v1/chats"))
        .bearer_auth(&private.ben)
        .json(&json!({"repository": private.repository, "message": "cat everything"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let name = store_name(&state, private.repository);
    let response = reqwest::Client::new()
        .post(format!("http://{addr}/v1/workflows/release/runs"))
        .bearer_auth(&private.ben)
        .json(&json!({"repository": name}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);

    // Its member reads them.
    for path in [
        format!("/v1/cards/{}", private.card),
        format!("/v1/cards/{}/runs", private.card),
        format!("/v1/runs/{}", private.run),
        format!("/v1/lessons/{}", private.lesson),
        format!("/v1/projects/{}", private.project),
        format!("/v1/repositories/{}/issues", private.repository),
    ] {
        let (status, body) = call(&addr, &private.lea, "GET", &path).await;
        assert_eq!(status, 200, "{path}: {body}");
    }
}

/// The lists leave out what a non-member does not see, and show it to a member.
#[tokio::test]
async fn the_lists_leave_out_a_private_project_for_a_non_member() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let store = &state.app.store;
    store
        .add_proposal(
            "2026-03-10",
            "retro/shared",
            "Tests for radiance",
            "The card Front page of RADIANCE failed",
            &[],
            &[private.card],
        )
        .unwrap();
    let title = store.card(private.card).unwrap().unwrap().title;
    let shared = store
        .proposals(None)
        .unwrap()
        .into_iter()
        .find(|p| p.branch == "retro/shared")
        .unwrap();
    store
        .add_inbox_item(
            "proposal",
            None,
            "Tests for radiance",
            &json!({"proposal": shared.id, "reason": format!("{title} of RADIANCE failed")}),
        )
        .unwrap();
    let snapshot = "/v1/snapshot";
    let (_, seen) = call(&addr, &private.lea, "GET", snapshot).await;
    assert_eq!(seen["boards"].as_array().unwrap().len(), 1);
    assert_eq!(seen["inbox"].as_array().unwrap().len(), 4, "{seen}");
    let (_, hidden) = call(&addr, &private.ben, "GET", snapshot).await;
    assert!(hidden["boards"].as_array().unwrap().is_empty(), "{hidden}");
    // Only the item of the shared proposal is left, without quoting the project.
    let inbox = hidden["inbox"].as_array().unwrap();
    assert_eq!(inbox.len(), 1, "{hidden}");
    assert_eq!(inbox[0]["title"], "Tests for a private project");
    let reason = inbox[0]["data"]["reason"].as_str().unwrap();
    assert!(
        !reason.contains(&title) && !reason.contains("RADIANCE"),
        "{reason}"
    );

    let count = |v: &Value| v.as_array().map_or(0, Vec::len);
    for (path, for_lea, for_ben) in [
        ("/v1/projects", 1, 0),
        ("/v1/lessons", 1, 0),
        ("/v1/judgements", 1, 0),
        ("/v1/exports", 2, 0),
        ("/v1/proposals", 2, 1),
        ("/v1/replays", 1, 0),
        ("/v1/research", 1, 0),
        ("/v1/automations", 1, 0),
    ] {
        assert_eq!(
            count(&call(&addr, &private.lea, "GET", path).await.1),
            for_lea,
            "{path}"
        );
        assert_eq!(
            count(&call(&addr, &private.ben, "GET", path).await.1),
            for_ben,
            "{path}"
        );
    }
    // The shared proposal is shown without quoting the project.
    let (_, proposals) = call(&addr, &private.ben, "GET", "/v1/proposals").await;
    let shared = &proposals[0];
    assert_eq!(shared["cards"], json!([]));
    assert_eq!(shared["title"], "Tests for a private project");
    let body = shared["body"].as_str().unwrap();
    assert!(
        !body.contains(&title) && !body.contains("RADIANCE"),
        "{body}"
    );
    // An owner who is no member sees the record, not the board.
    let (_, projects) = call(&addr, &private.olga, "GET", "/v1/projects").await;
    assert_eq!(projects[0]["member"], false);
    assert_eq!(projects[0]["open_cards"], 0);
    assert_eq!(projects[0]["members"][0]["handle"], "lea");

    let (_, retro) = call(&addr, &private.ben, "GET", "/v1/retros/2026-03-10").await;
    assert_eq!(count(&retro["report"]["errors"]), 1, "{retro}");
    assert_eq!(retro["report"]["errors"][0]["text"], "public");
    let (_, retro) = call(&addr, &private.lea, "GET", "/v1/retros/2026-03-10").await;
    assert_eq!(count(&retro["report"]["errors"]), 2);

    // A milestone counts and lists the private card for its members only.
    let milestone = state
        .app
        .add_milestone(&crate::app::MilestoneFields {
            name: "Launch".into(),
            target_date: "2027-01-31".into(),
            description: String::new(),
        })
        .unwrap();
    store
        .set_card_milestones(private.card, &[milestone.id])
        .unwrap();
    let path = format!("/v1/milestones/{}", milestone.id);
    let (_, seen) = call(&addr, &private.lea, "GET", &path).await;
    assert_eq!(count(&seen["cards"]), 1, "{seen}");
    let (_, hidden) = call(&addr, &private.ben, "GET", &path).await;
    assert_eq!(count(&hidden["cards"]), 0, "{hidden}");

    // A cycle of the private project is not found for a non-member, and its list is not either.
    let cycle = store
        .add_cycle(
            store.card(private.card).unwrap().unwrap().project_id,
            "Cycle 1",
            "2026-11-02",
            "2026-11-15",
        )
        .unwrap();
    let path = format!("/v1/cycles/{}", cycle.id);
    assert_eq!(call(&addr, &private.lea, "GET", &path).await.0, 200);
    assert_eq!(call(&addr, &private.ben, "GET", &path).await.0, 404);

    // The issues across projects leave its cards out.
    assert!(count(&call(&addr, &private.lea, "GET", "/v1/issues").await.1) > 0);
    assert_eq!(
        count(&call(&addr, &private.ben, "GET", "/v1/issues").await.1),
        0
    );
    // A mention in a private card's comment reaches a non-member's notices not at all.
    let ben_id = store.person_by_handle("ben").unwrap().unwrap().id;
    store
        .add_notice(
            ben_id,
            &crate::store::NewNotice {
                kind: crate::store::MENTIONED,
                by: Some("lea"),
                card_id: Some(private.card),
                inbox_item_id: None,
                title: "Secret card",
                body: "@ben look at the secret",
            },
        )
        .unwrap();
    assert_eq!(
        count(&call(&addr, &private.ben, "GET", "/v1/notices").await.1),
        0
    );

    let usage = format!("/v1/usage?repository={}", private.repository);
    assert_eq!(call(&addr, &private.ben, "GET", &usage).await.0, 404);
    assert_eq!(call(&addr, &private.lea, "GET", &usage).await.0, 200);
}

/// An owner makes a project private through the API; its board leaves the snapshot of a member who is not named.
#[tokio::test]
async fn the_runs_of_a_workflow_of_a_private_project_are_left_out_for_a_non_member() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let list = "/v1/workflow-runs?workflow=release";
    let detail = format!("/v1/workflow-runs/{}", private.workflow_run);

    let (status, seen) = call(&addr, &private.lea, "GET", list).await;
    assert_eq!(status, 200);
    assert_eq!(seen.as_array().unwrap().len(), 1, "{seen}");
    assert_eq!(call(&addr, &private.lea, "GET", &detail).await.0, 200);

    for token in [&private.ben, &private.olga] {
        let (status, hidden) = call(&addr, token, "GET", list).await;
        assert_eq!(status, 200);
        assert!(hidden.as_array().unwrap().is_empty(), "{hidden}");
        let (status, body) = call(&addr, token, "GET", &detail).await;
        assert_eq!(status, 404, "{body}");
        assert!(!body.to_string().contains("secret"), "{body}");
    }
}

#[tokio::test]
async fn an_owner_makes_a_project_private_through_the_api() {
    let (_dir, state, addr) = start().await;
    let ben = token_of(&state, "ben", Role::Member, 2);
    token_of(&state, "lea", Role::Member, 1);
    let project = state.app.store.projects().unwrap()[0].id;
    let response = reqwest::Client::new()
        .put(format!("http://{addr}/v1/projects/{project}/access"))
        .bearer_auth(state.token.to_string())
        .json(&json!({"private": true, "members": ["lea", "owner"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let view: Value = response.json().await.unwrap();
    assert_eq!(
        (view["private"].as_bool(), view["member"].as_bool()),
        (Some(true), Some(true))
    );
    let (_, snapshot) = call(&addr, &ben, "GET", "/v1/snapshot").await;
    assert!(snapshot["boards"].as_array().unwrap().is_empty());
    // Only an owner, and never an agent, names who sees a project.
    let path = format!("/v1/projects/{project}/access");
    assert_eq!(call(&addr, &ben, "PUT", &path).await.0, 403);
}

/// A repository that is also in a public project is seen, with the public project's cards only: its board, its
/// issues and the similar issues of a public card leave the private project's cards out.
#[tokio::test]
async fn a_shared_repository_shows_only_the_public_project_s_cards() {
    let (_dir, state, addr) = start().await;
    let private = private(&state);
    let store = &state.app.store;
    let open = store.add_project("Open", "OPEN").unwrap();
    store.link_repository(open.id, private.repository).unwrap();
    let secret = store.card(private.card).unwrap().unwrap();
    let public = store
        .add_card(
            private.repository,
            &secret.title,
            &secret.body,
            "lea",
            "Backlog",
        )
        .unwrap();
    store.set_card_project(public.id, open.id).unwrap();

    let (_, snapshot) = call(&addr, &private.ben, "GET", "/v1/snapshot").await;
    let cards: Vec<i64> = snapshot["boards"][0]["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["card"]["id"].as_i64().or(c["id"].as_i64()).unwrap())
        .collect();
    assert_eq!(cards, [public.id], "{snapshot}");
    let issues = format!("/v1/repositories/{}/issues", private.repository);
    let (status, listed) = call(&addr, &private.ben, "GET", &issues).await;
    assert_eq!(status, 200, "{listed}");
    let ids: Vec<i64> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [public.id]);
    let similar = format!("/v1/cards/{}/similar", public.id);
    let (status, found) = call(&addr, &private.ben, "GET", &similar).await;
    assert_eq!(status, 200, "{found}");
    assert_eq!(found, json!([]), "the private card is not offered");
    // Its files are the public project's too.
    let tree = format!("/v1/repositories/{}/tree", private.repository);
    let (status, body) = call(&addr, &private.ben, "GET", &tree).await;
    // The test's repository has no clone, so its route answers that the folder is missing: it got past the check.
    assert!(
        status == 200 || body.as_str().is_some_and(|b| b.contains("not found")),
        "{status} {body}"
    );
}

/// The event stream sends nothing about a private project to someone who is not its member.
#[tokio::test]
async fn events_of_a_private_project_reach_its_members_only() {
    let (_dir, state, _addr) = start().await;
    let private = private(&state);
    let app = &state.app;
    let person = |handle: &str| {
        let id = app.store.person_by_handle(handle).unwrap().unwrap().id;
        app.sight(&Actor::person(id, handle)).unwrap()
    };
    let (lea, ben, olga) = (person("lea"), person("ben"), person("olga"));
    let card = app.store.card(private.card).unwrap().unwrap();
    let project = app.store.project(private.project).unwrap().unwrap();
    let of = AppEvent::from;
    let events = [
        of(Event::CardMoved {
            card: private.card,
            from: "Backlog".into(),
            to: "Ready".into(),
        }),
        of(Event::CardUpdated(card)),
        of(Event::RunEvent {
            run: private.run,
            seq: 1,
            kind: "said".into(),
            data: json!({"message": "secret"}),
        }),
        of(Event::FileChanged {
            repository: private.repository,
            path: "README.md".into(),
        }),
        of(Event::ProjectChanged(project.clone())),
        of(Event::WorkflowRunStarted {
            run: private.workflow_run,
        }),
        of(Event::ForgeError {
            repository: store_name(&state, private.repository),
            message: "secret".into(),
        }),
    ];
    for event in &events {
        assert!(super::super::events::seen(app, &lea, event), "{event:?}");
        assert!(!super::super::events::seen(app, &ben, event), "{event:?}");
    }
    // An owner who is no member hears of the project's record, nothing else.
    assert!(super::super::events::seen(
        app,
        &olga,
        &of(Event::ProjectChanged(project))
    ));
    assert!(!super::super::events::seen(app, &olga, &events[0]));
    let elsewhere = of(Event::CardMoved {
        card: 999,
        from: "Backlog".into(),
        to: "Ready".into(),
    });
    assert!(super::super::events::seen(app, &ben, &elsewhere));
}

/// The routes with a parameter that the check does not guard, each because what it names belongs to no project:
/// a person, a token, a pin, the harness configuration, a package, a plugin, a setting, a port (filtered by its
/// run), a Retro's day (whose view is filtered), a takeover of the decision model, a replay (listed by its run) or
/// a workflow, by name, a kind of issues or a template of the organisation, a person's own notice, a wiki page, its links or the pages that link to a card (a private project's pages and cards are
/// left out by the wiki's own handlers); or that checks what it names in its own handler (a card by its key, a milestone of the
/// organisation, whose cards are counted and listed with `App::may_see_project`).
const UNGUARDED: [&str; 32] = [
    "/v1/cards/by-key/",
    "/v1/templates/",
    "/v1/kinds/",
    "/v1/wiki/pages/",
    "/v1/wiki/history/",
    "/v1/wiki/links/",
    "/v1/wiki/cards/",
    "/v1/notices/",
    "/v1/milestones/",
    // A view is a person's own or shared; its cards are filtered by what the asker may see.
    "/v1/views/",
    // A cycle belongs to a project; `App::cycle` answers not found for one the asker may not see.
    "/v1/cycles/",
    "/v1/people/",
    "/v1/tokens/",
    "/v1/favourites/",
    "/v1/recent/",
    "/v1/agents/",
    "/v1/skills/",
    "/v1/mcp-servers/",
    "/v1/workflows/{name}",
    "/v1/workflow-runs/",
    "/v1/extensions/",
    "/v1/packages/",
    "/v1/models/",
    "/v1/plugins/",
    "/v1/secrets/",
    "/v1/ports/",
    "/v1/retros/{date}",
    "/v1/takeovers/",
    "/v1/replays/",
    "/v1/webhooks/",
    "/v1/outgoing-webhooks/",
    "/api/hooks/",
];

/// A new route that names something by a parameter is either guarded or named here as belonging to no project, so
/// that a route of a project's things cannot be added without the check.
#[test]
fn every_route_with_a_parameter_is_guarded_or_named_as_of_no_project() {
    let unnamed: Vec<String> = ROUTES
        .iter()
        .filter(|(_, route, _)| route.contains('{'))
        .filter(|(_, route, _)| super::guarded(route).is_none())
        .filter(|(_, route, _)| !UNGUARDED.iter().any(|p| route.starts_with(p)))
        .map(|(method, route, _)| format!("{method} {route}"))
        .collect();
    assert!(
        unnamed.is_empty(),
        "routes neither guarded nor named: {unnamed:?}"
    );
}

mod counts;
mod links;
mod stream;

mod body_ids;
