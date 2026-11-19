//! The check of a private project (ADR 0018) on every route that names one thing by its id: a card, an inbox item, a
//! change, a run or chat, a repository, a project, a lesson, a judgement, a grilling, a proposal, an export, an
//! automation, a research, a project's request or a workflow run. A thing of a private project that the person asking is no member of is
//! answered `404`, as if it did not exist, before the route runs, so that no route of these can show or change it; an
//! id that is not a plain number is refused the same way, since the router would decode `%31` to `1`. [`check`] runs in
//! `authenticate` after the role, and a test fails for a route with a parameter that is neither guarded here nor named
//! as belonging to no project. The lists (the snapshot, the event stream, the lessons, judgements, replays, proposals,
//! Retros, exports, automations, research and ports) leave such things out in their own routes; what is seen is
//! decided in `app::projects::sight`.

use axum::http::StatusCode;

use super::ApiError;
use crate::app::{App, Result, Sight};
use crate::decide::Actor;
use crate::improve::export;
use crate::store::InboxItem;

/// The kinds of thing a route names by the id after `/v1/<kind>/`, which this check guards.
pub(super) const GUARDED: [&str; 15] = [
    "cards",
    "inbox",
    "changes",
    "runs",
    "chats",
    "repositories",
    "projects",
    "lessons",
    "judgements",
    "grillings",
    "proposals",
    "exports",
    "automations",
    "research",
    "requests",
];

/// The kind of a workflow run, which a route names after `/v1/workflows/runs/`.
pub(super) const WORKFLOW_RUNS: &str = "workflow_runs";

/// The routes of a project that show only its record, which an owner who is no member may use to name its members.
const RECORD_ROUTES: [&str; 2] = ["/v1/projects/{id}", "/v1/projects/{id}/access"];

/// The kind of thing `route` names by id and where the id is among the parts of the path, if it names one this check
/// guards.
pub(super) fn guarded(route: &str) -> Option<(&str, usize)> {
    let parts: Vec<&str> = route.split('/').collect();
    match parts.as_slice() {
        ["", "v1", "workflows", "runs", param, ..] if param.starts_with('{') => {
            Some((WORKFLOW_RUNS, 4))
        }
        ["", "v1", kind, param, ..] if param.starts_with('{') && GUARDED.contains(kind) => {
            Some((kind, 3))
        }
        _ => None,
    }
}

/// Refuses with `404` the request for `path`, served by `route`, when it names a thing that `by` does not see. An id
/// that is not a plain number (such as `%31`, which the router would decode to `1`) is refused too, so that no spelling
/// of an id gets past.
pub(super) fn check(
    app: &App,
    by: &Actor,
    route: &str,
    path: &str,
) -> std::result::Result<(), ApiError> {
    let Some((kind, at)) = guarded(route) else {
        return Ok(());
    };
    let segment = path.split('/').nth(at).unwrap_or_default();
    let hidden = || {
        ApiError(
            StatusCode::NOT_FOUND,
            format!("no {} {segment}", noun(kind)),
        )
    };
    let plain = !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit() || b == b'-');
    let Some(id) = segment.parse::<i64>().ok().filter(|_| plain) else {
        return Err(hidden());
    };
    let sight = app.sight(by)?;
    if sight.hides_nothing() || sees(app, &sight, kind, route, id)? {
        Ok(())
    } else {
        Err(hidden())
    }
}

/// The word for one thing of `kind`, as a 404 says it.
fn noun(kind: &str) -> &str {
    match kind {
        "inbox" => "inbox item",
        "chats" => "chat",
        "repositories" => "repository",
        WORKFLOW_RUNS => "workflow run",
        other => other.strip_suffix('s').unwrap_or(other),
    }
}

/// Whether `sight` sees the thing of `kind` with `id`. A thing that does not exist is seen, so that its route says so.
pub(super) fn sees(app: &App, sight: &Sight, kind: &str, route: &str, id: i64) -> Result<bool> {
    let store = &app.store;
    match kind {
        "cards" => app.sees_card(sight, id),
        "inbox" => match store.inbox()?.into_iter().find(|item| item.id == id) {
            Some(item) => sees_inbox_item(app, sight, &item),
            None => Ok(true),
        },
        "changes" => sees_change(app, sight, id),
        "runs" | "chats" => app.sees_run_id(sight, id),
        "repositories" => Ok(sight.repository(id)),
        "projects" if RECORD_ROUTES.contains(&route) => Ok(sight.project_record(id)),
        "projects" => Ok(sight.project(id)),
        "lessons" => match store.lesson(id)? {
            Some(lesson) => app.sees_lesson(sight, &lesson),
            None => Ok(true),
        },
        "judgements" => match store.judgement(id)? {
            Some(j) => app.sees_judgement(sight, j.card_id, j.run_id),
            None => Ok(true),
        },
        "grillings" => match store.grilling(id)? {
            Some(g) => Ok(sight.repository(g.repository_id) && app.sees_run_id(sight, g.run_id)?),
            None => Ok(true),
        },
        "proposals" => match store.proposal(id)? {
            Some(p) => Ok(p.repository.is_none_or(|r| sight.repository(r))),
            None => Ok(true),
        },
        "exports" => Ok(export::list(&app.data_dir)
            .into_iter()
            .find(|e| e.id == id)
            .is_none_or(|e| export::seen(&e, sight))),
        "automations" => match store.automation(id)? {
            Some(a) => Ok(a.repository_id.is_none_or(|r| sight.repository(r))),
            None => Ok(true),
        },
        "research" => match store.research(id)? {
            Some(r) => Ok(sight.repository(r.repository_id)),
            None => Ok(true),
        },
        "requests" => match store.request(id)? {
            Some(request) => Ok(sight.project(request.project_id)),
            None => Ok(true),
        },
        WORKFLOW_RUNS => match store.workflow_run(id)? {
            Some(run) => Ok(sight.repository(run.repository_id)),
            None => Ok(true),
        },
        _ => Ok(false),
    }
}

/// Whether `sight` sees the change `id`: by its card, else by its repository.
fn sees_change(app: &App, sight: &Sight, id: i64) -> Result<bool> {
    let Some(change) = app.store.change(id)? else {
        return Ok(true);
    };
    if let Some(card) = change.card_id {
        return app.sees_card(sight, card);
    }
    Ok(change.repository_id.is_none_or(|r| sight.repository(r)))
}

/// Whether `sight` sees an inbox item: by its card, or by the change, grilling, proposal or run it is about.
pub(super) fn sees_inbox_item(app: &App, sight: &Sight, item: &InboxItem) -> Result<bool> {
    if let Some(card) = item.card_id {
        return app.sees_card(sight, card);
    }
    let about = |field: &str| item.data[field].as_i64();
    // A workflow's item names a workflow run as `run`, not a run of an agent.
    if item.kind.starts_with("workflow")
        && let Some(run) = about("run")
    {
        return sees(app, sight, WORKFLOW_RUNS, "", run);
    }
    if let Some(change) = about("change") {
        return sees_change(app, sight, change);
    }
    if let Some(grilling) = about("grilling") {
        return sees(app, sight, "grillings", "", grilling);
    }
    if let Some(proposal) = about("proposal") {
        return sees(app, sight, "proposals", "", proposal);
    }
    if let Some(run) = about("run") {
        return app.sees_run_id(sight, run);
    }
    if let Some(repository) = about("repository") {
        return Ok(sight.repository(repository));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
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
        assert_eq!(seen["inbox"].as_array().unwrap().len(), 3, "{seen}");
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
    /// a workflow, by name, a kind of issues or a template of the organisation, a person's own notice, a wiki page (a private project's pages are hidden by the wiki's
    /// own routes, issue #374); or that checks what it names in its own handler (a card by its key, a milestone of the
    /// organisation, whose cards are counted and listed with `App::may_see_project`).
    const UNGUARDED: [&str; 26] = [
        "/v1/cards/by-key/",
        "/v1/templates/",
        "/v1/kinds/",
        "/v1/wiki/",
        "/v1/notices/",
        "/v1/milestones/",
        "/v1/people/",
        "/v1/tokens/",
        "/v1/favourites/",
        "/v1/recent/",
        "/v1/agents/",
        "/v1/skills/",
        "/v1/mcp-servers/",
        "/v1/workflows/{name}",
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
}
