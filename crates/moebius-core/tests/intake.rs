//! Intake: a project that turns it on takes a viewer's request, which a grilling (a fake Pi here) drafts into an issue
//! that a member accepts into a card the requester follows, or declines with a reason; a project without intake takes
//! none.

// Unix only: the fake Pi (testdata/fake-pi) is a Python script started by its `#!` line.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use moebius_core::store::Role;
use serde_json::{Value, json};

fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(source.join("README.md"), "A demo.\n").unwrap();
    git(&source, &["add", "-A"]).unwrap();
    git(&source, &["commit", "--quiet", "-m", "first"]).unwrap();
    source
}

/// A server with the fake Pi and a repository `demo`; returns it with the id of the repository's project.
fn server(dir: &Path) -> (Arc<Server>, i64) {
    let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[agents.Coder]\nmodel = \"coder-model\"\n",
        fake.display().to_string()
    );
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    let server = Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap());
    server
        .app
        .add_repository("demo", source(dir).to_str().unwrap())
        .unwrap();
    let project = server.app.store.projects().unwrap()[0].id;
    (server, project)
}

/// A person of `role` with a token of their own.
fn person(server: &Server, handle: &str, role: Role) -> String {
    let person = server.app.store.add_person(handle, handle, role).unwrap();
    let token = format!("moebius-access-{:064}", person.id);
    server
        .app
        .store
        .add_access_token(person.id, "test", &token)
        .unwrap();
    token
}

struct Api {
    base: String,
    owner: String,
    client: reqwest::Client,
}

impl Api {
    async fn start(server: &Server) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            codes: Default::default(),
            app: server.app.clone(),
            runs: server.harness.clone(),
            harness_repo: server.harness_repo.clone(),
            token: server.token.clone(),
            router: server.router.clone(),
            config: server.live.clone(),
        };
        tokio::spawn(api::serve(listener, state));
        Self {
            base,
            owner: server.token.to_string(),
            client: reqwest::Client::new(),
        }
    }

    async fn call(
        &self,
        token: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    async fn get(&self, token: &str, path: &str) -> (u16, Value) {
        self.call(token, reqwest::Method::GET, path, None).await
    }

    async fn post(&self, token: &str, path: &str, body: Value) -> (u16, Value) {
        self.call(token, reqwest::Method::POST, path, Some(body))
            .await
    }
}

async fn wait_for<T>(what: &str, mut found: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(found) = found() {
            return found;
        }
        assert!(Instant::now() < deadline, "{what} did not happen");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Waits until the chat `run` has answered and waits for the next message.
async fn answered(server: &Server, run: i64) {
    wait_for("the agent answers", || {
        (server.app.store.run(run).unwrap().unwrap().status == "waiting").then_some(())
    })
    .await;
}

/// A request that has the fake Pi leave `issues` as the grilling's issues.json.
fn request_with_issues(words: &str, issues: &str) -> Value {
    json!({ "text": format!("{words}\n[put .moebius/grilling/issues.json]\n{issues}\n[/put]") })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_without_intake_takes_no_request() {
    let dir = tempfile::tempdir().unwrap();
    let (server, project) = server(dir.path());
    let api = Api::start(&server).await;
    let kim = person(&server, "kim", Role::Viewer);

    let (_, view) = api.get(&kim, &format!("/v1/projects/{project}")).await;
    assert_eq!(view["intake"], false);
    let (status, refused) = api
        .post(
            &kim,
            &format!("/v1/projects/{project}/requests"),
            json!({"text": "Dark mode, please"}),
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert!(refused.to_string().contains("no intake"), "{refused}");
    assert!(server.app.store.requests(project, None).unwrap().is_empty());

    // Only an owner turns it on.
    let (status, _) = api
        .call(
            &kim,
            reqwest::Method::PUT,
            &format!("/v1/projects/{project}/intake"),
            Some(json!({"enabled": true})),
        )
        .await;
    assert_eq!(status, 403);
    let (status, view) = api
        .call(
            &api.owner,
            reqwest::Method::PUT,
            &format!("/v1/projects/{project}/intake"),
            Some(json!({"enabled": true})),
        )
        .await;
    assert_eq!((status, &view["intake"]), (200, &json!(true)), "{view}");
    let (status, made) = api
        .post(
            &kim,
            &format!("/v1/projects/{project}/requests"),
            json!({"text": "Dark mode, please"}),
        )
        .await;
    assert_eq!(
        (status, made["status"].as_str()),
        (200, Some("grilling")),
        "{made}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_viewer_s_request_is_grilled_into_a_draft_and_a_member_s_acceptance_makes_a_card_they_follow()
 {
    let dir = tempfile::tempdir().unwrap();
    let (server, project) = server(dir.path());
    let api = Api::start(&server).await;
    let kim = person(&server, "kim", Role::Viewer);
    let lou = person(&server, "lou", Role::Viewer);
    let mia = person(&server, "mia", Role::Member);
    server.app.set_intake(project, true).unwrap();
    let mut events = server.app.bus.subscribe();

    let (status, made) = api
        .post(
            &kim,
            &format!("/v1/projects/{project}/requests"),
            request_with_issues(
                "Please add a dark mode.",
                r#"[{"title": "Add a dark mode", "body": "A theme switch.", "blocked_by": []}, {"title": "Remember the choice"}]"#,
            ),
        )
        .await;
    assert_eq!(status, 200, "{made}");
    assert_eq!(
        (made["status"].as_str(), made["requester"].as_str()),
        (Some("grilling"), Some("kim"))
    );
    let id = made["id"].as_i64().unwrap();
    let run = made["run_id"].as_i64().expect("a grilling was started");
    let chat = server.app.store.run(run).unwrap().unwrap();
    assert_eq!(
        (chat.agent.as_str(), chat.origin.as_str()),
        ("Grilling-partner", "automation")
    );
    answered(&server, run).await;

    // A member publishes what the grilling left: the request gets its draft, and the members an inbox item.
    let (status, published) = api
        .post(&mia, &format!("/v1/chats/{run}/publish"), json!({}))
        .await;
    assert_eq!(status, 200, "{published}");
    let (_, drafted) = api.get(&kim, &format!("/v1/requests/{id}")).await;
    assert_eq!(drafted["status"], "drafted", "{drafted}");
    assert_eq!(drafted["title"], "Add a dark mode");
    let body = drafted["body"].as_str().unwrap();
    assert!(body.starts_with("A theme switch."), "{body}");
    assert!(body.contains("- Remember the choice"), "{body}");
    let kinds: Vec<String> = server
        .app
        .store
        .inbox()
        .unwrap()
        .into_iter()
        .map(|i| i.kind)
        .collect();
    assert_eq!(
        kinds,
        ["intake_request"],
        "the grilling is not a second item"
    );
    let kinds = loop {
        let event = events.recv().await.unwrap();
        if event.kind == "intake_changed" && event.data["status"] == "drafted" {
            break event.data;
        }
    };
    assert_eq!(
        kinds,
        json!({"project": project, "request": id, "status": "drafted"})
    );

    // A viewer reads their own requests and no one else's, and decides nothing.
    let (_, mine) = api
        .get(&kim, &format!("/v1/projects/{project}/requests"))
        .await;
    assert_eq!(mine.as_array().unwrap().len(), 1);
    let (_, others) = api
        .get(&lou, &format!("/v1/projects/{project}/requests"))
        .await;
    assert_eq!(others, json!([]));
    assert_eq!(api.get(&lou, &format!("/v1/requests/{id}")).await.0, 404);
    let (_, all) = api
        .get(&mia, &format!("/v1/projects/{project}/requests"))
        .await;
    assert_eq!(all.as_array().unwrap().len(), 1);
    assert_eq!(
        api.post(&kim, &format!("/v1/requests/{id}/accept"), json!({}))
            .await
            .0,
        403
    );
    assert_eq!(
        api.post(
            &kim,
            &format!("/v1/requests/{id}/decline"),
            json!({"reason": "no"})
        )
        .await
        .0,
        403
    );

    // The member edits the title while accepting: one confirmed card, written by the requester, who follows it.
    let (status, accepted) = api
        .post(
            &mia,
            &format!("/v1/requests/{id}/accept"),
            json!({"title": "Dark theme"}),
        )
        .await;
    assert_eq!(status, 200, "{accepted}");
    assert_eq!(accepted["status"], "accepted");
    let card = server
        .app
        .card(accepted["card_id"].as_i64().unwrap())
        .unwrap();
    assert_eq!(
        (
            card.title.as_str(),
            card.author.as_str(),
            card.column.as_str(),
            card.confirmed
        ),
        ("Dark theme", "person:kim", "Backlog", true)
    );
    assert!(card.body.starts_with("A theme switch."));
    assert_eq!(card.project_id, project);
    let (_, followers) = api
        .get(&kim, &format!("/v1/cards/{}/followers", card.id))
        .await;
    assert_eq!(followers, json!(["kim"]));
    let (_, notices) = api.get(&kim, "/v1/notices").await;
    assert_eq!(notices[0]["card_id"], card.id);
    assert_eq!(notices[0]["body"], "Your request was accepted");
    assert_eq!(notices[0]["by"], "mia");
    assert!(
        server.app.store.inbox().unwrap().is_empty(),
        "the item is resolved"
    );
    wait_for("the grilling chat ends", || {
        server
            .app
            .store
            .run(run)
            .unwrap()
            .unwrap()
            .ended_at
            .map(|_| ())
    })
    .await;
    let grilling = server.app.store.grilling_for_run(run).unwrap().unwrap();
    assert_eq!(grilling.status, "dismissed");
    assert_eq!(
        server.app.store.judgements("accept_request").unwrap().len(),
        1
    );

    // Once only.
    let (status, _) = api
        .post(&mia, &format!("/v1/requests/{id}/accept"), json!({}))
        .await;
    assert_eq!(status, 422);
    assert_eq!(
        server
            .app
            .store
            .cards(1)
            .unwrap()
            .iter()
            .filter(|c| c.title == "Dark theme")
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_declined_request_tells_the_requester_why_and_a_chat_that_left_nothing_drafts_their_words()
 {
    let dir = tempfile::tempdir().unwrap();
    let (server, project) = server(dir.path());
    let api = Api::start(&server).await;
    let kim = person(&server, "kim", Role::Viewer);
    let mia = person(&server, "mia", Role::Member);
    server.app.set_intake(project, true).unwrap();

    // The grilling leaves no file: ending the chat drafts the person's own words, so the request is not stranded.
    let (_, made) = api
        .post(
            &kim,
            &format!("/v1/projects/{project}/requests"),
            json!({"text": "Make the logo bigger\nIt is hard to see."}),
        )
        .await;
    let (id, run) = (
        made["id"].as_i64().unwrap(),
        made["run_id"].as_i64().unwrap(),
    );
    answered(&server, run).await;
    assert_eq!(
        api.post(&mia, &format!("/v1/chats/{run}/end"), json!({}))
            .await
            .0,
        200
    );
    wait_for("the request is drafted", || {
        (server.app.store.request(id).unwrap().unwrap().status == "drafted").then_some(())
    })
    .await;
    let drafted = server.app.store.request(id).unwrap().unwrap();
    assert_eq!(drafted.title, "Make the logo bigger");
    assert!(drafted.body.contains("It is hard to see."));

    // A decline needs a reason.
    let (status, _) = api
        .post(
            &mia,
            &format!("/v1/requests/{id}/decline"),
            json!({"reason": "  "}),
        )
        .await;
    assert_eq!(status, 422);
    let (status, declined) = api
        .post(
            &mia,
            &format!("/v1/requests/{id}/decline"),
            json!({"reason": "The logo is a brand rule."}),
        )
        .await;
    assert_eq!(status, 200, "{declined}");
    assert_eq!(
        (declined["status"].as_str(), declined["card_id"].clone()),
        (Some("declined"), Value::Null)
    );

    // The requester reads why, and was told.
    let (_, read) = api.get(&kim, &format!("/v1/requests/{id}")).await;
    assert_eq!(read["reason"], "The logo is a brand rule.");
    let (_, notices) = api.get(&kim, "/v1/notices").await;
    assert_eq!(
        notices[0]["body"],
        "Your request was declined: The logo is a brand rule."
    );
    assert_eq!(notices[0]["title"], "Make the logo bigger");
    assert!(server.app.store.inbox().unwrap().is_empty());
    // Decided once; it cannot be accepted afterwards, and no card was made.
    assert_eq!(
        api.post(&mia, &format!("/v1/requests/{id}/accept"), json!({}))
            .await
            .0,
        422
    );
    assert_eq!(
        api.post(
            &mia,
            &format!("/v1/requests/{id}/decline"),
            json!({"reason": "again"})
        )
        .await
        .0,
        422
    );
    assert_eq!(server.app.store.cards(1).unwrap().len(), 0);
}
