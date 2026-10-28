//! Grilling: a chat with the Grilling partner (a fake Pi here) that ends in issues a person confirms, which become
//! cards, or in a research report.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use moebius_core::api::{self, ApiState};
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use serde_json::{Value, json};

/// A repository with one commit.
fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(source.join("README.md"), "A demo.\n").unwrap();
    git(&source, &["add", "-A"]).unwrap();
    git(&source, &["commit", "--quiet", "-m", "first"]).unwrap();
    source
}

fn fake_pi() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/fake-pi")
        .display()
        .to_string()
}

/// A server with the fake Pi and a repository `demo` without a forge; returns the server and the repository's id.
fn server(dir: &Path) -> (Arc<Server>, i64) {
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[agents.Coder]\nmodel = \"coder-model\"\n",
        fake_pi()
    );
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    let server = Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap());
    let repo = server
        .app
        .add_repository("demo", source(dir).to_str().unwrap())
        .unwrap();
    (server, repo.id)
}

struct Api {
    base: String,
    token: String,
    client: reqwest::Client,
}

impl Api {
    async fn start(server: &Server) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            app: server.app.clone(),
            runs: server.harness.clone(),
            harness_repo: server.harness_repo.clone(),
            token: server.token.clone(),
            config: server.live.clone(),
        };
        tokio::spawn(api::serve(listener, state));
        Self {
            base,
            token: server.token.to_string(),
            client: reqwest::Client::new(),
        }
    }

    async fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token);
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

    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.call(reqwest::Method::POST, path, Some(body)).await
    }

    async fn get(&self, path: &str) -> (u16, Value) {
        self.call(reqwest::Method::GET, path, None).await
    }

    /// Has a grilling chat in `repository` leave `files` in its worktree, ends it and waits until it has ended;
    /// returns the chat's run id.
    async fn grill(&self, server: &Server, repository: i64, files: &[(&str, &str)]) -> i64 {
        let mut message = "Export the reports.".to_string();
        for (name, text) in files {
            message += &format!("\n[put .moebius/grilling/{name}]\n{text}\n[/put]");
        }
        let (status, run) = self
            .post(
                "/v1/chats",
                json!({ "agent": "Grilling-partner", "message": message, "repository": repository }),
            )
            .await;
        assert_eq!(status, 200, "{run}");
        let id = run["id"].as_i64().unwrap();
        // The agent has answered, so it has written its files, when the chat waits for the next message.
        wait_for("the agent answers", || {
            (server.app.store.run(id).unwrap().unwrap().status == "waiting").then_some(())
        })
        .await;
        let (status, ended) = self.post(&format!("/v1/chats/{id}/end"), json!({})).await;
        assert_eq!(status, 200, "{ended}");
        wait_for("the chat ends", || {
            server
                .app
                .store
                .run(id)
                .unwrap()
                .unwrap()
                .ended_at
                .map(|_| ())
        })
        .await;
        id
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

const TWO_ISSUES: &str = r#"[{"title": "Add the export button", "body": "A button on the report page.", "blocked_by": []}, {"title": "Write the CSV", "body": "The columns are the report's.", "blocked_by": [0]}]"#;

fn item_kinds(server: &Server) -> Vec<String> {
    server
        .app
        .store
        .inbox()
        .unwrap()
        .into_iter()
        .map(|item| item.kind)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_grilling_ends_in_issues_that_become_cards_that_move_on_to_ready() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let clone = PathBuf::from(server.app.store.repository(repo).unwrap().unwrap().path);
    let api = Api::start(&server).await;
    let mut events = server.app.bus.subscribe();

    let run = api
        .grill(&server, repo, &[("issues.json", TWO_ISSUES)])
        .await;

    // The chat is the agent's, ended as succeeded; its result was read, not offered as a change.
    let chat = server.app.store.run(run).unwrap().unwrap();
    assert_eq!(
        (chat.agent.as_str(), chat.status.as_str()),
        ("Grilling-partner", "succeeded")
    );
    assert!(server.app.store.changes(repo).unwrap().is_empty());
    assert_eq!(
        git(
            &clone,
            &["rev-list", "--count", &format!("main..{}", chat.branch())]
        )
        .unwrap_or_default(),
        "0",
        "nothing was committed on the chat's branch"
    );
    let (_, grillings) = api.get("/v1/grillings").await;
    assert_eq!(grillings.as_array().unwrap().len(), 1);
    let grilling = &grillings[0];
    assert_eq!(
        (grilling["status"].as_str(), grilling["run_id"].as_i64()),
        (Some("proposed"), Some(run))
    );
    assert_eq!(grilling["repository_id"], repo);
    assert_eq!(grilling["report"], Value::Null);
    assert_eq!(grilling["issues"][1]["blocked_by"], json!([0]));
    let id = grilling["id"].as_i64().unwrap();
    let ended = loop {
        let event = events.recv().await.unwrap();
        if event.kind == "grilling_ended" {
            break event;
        }
    };
    assert_eq!(ended.data, json!({ "grilling": id, "status": "proposed" }));

    // One inbox item, with the issues in it.
    let inbox = server.app.store.inbox().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].kind, "grilling_issues");
    assert_eq!(inbox[0].title, "2 issues from a grilling");
    assert_eq!(inbox[0].data["grilling"], id);
    assert_eq!(inbox[0].data["issues"][0]["title"], "Add the export button");

    // A list that does not hold together confirms nothing.
    let bad = json!({ "issues": [{ "title": "Alone", "blocked_by": [4] }] });
    let (status, why) = api.post(&format!("/v1/grillings/{id}/confirm"), bad).await;
    assert_eq!(status, 422, "{why}");
    let (status, _) = api
        .post(
            &format!("/v1/grillings/{id}/confirm"),
            json!({ "issues": [] }),
        )
        .await;
    assert_eq!(status, 422);
    assert_eq!(
        api.get(&format!("/v1/grillings/{id}")).await.1["status"],
        "proposed"
    );

    // The person reorders the issues and edits one: the final list is what is made.
    let final_list = json!({ "issues": [
        { "title": "Write the CSV", "body": "Edited.", "blocked_by": [1] },
        { "title": "Add the export button", "body": "A button.", "blocked_by": [] },
    ] });
    let (status, confirmed) = api
        .post(&format!("/v1/grillings/{id}/confirm"), final_list.clone())
        .await;
    assert_eq!(status, 200, "{confirmed}");
    assert_eq!(confirmed["status"], "confirmed");
    assert!(server.app.store.inbox().unwrap().is_empty());
    // Confirming again is refused.
    let (status, _) = api
        .post(&format!("/v1/grillings/{id}/confirm"), final_list.clone())
        .await;
    assert_eq!(status, 422);

    // The decision is recorded with the final list.
    let decisions = server.app.store.judgements("confirm_grilling").unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].decider, "person");
    let context = server
        .app
        .store
        .blobs()
        .get(&decisions[0].context[0])
        .unwrap();
    let context: Value = serde_json::from_str(&context).unwrap();
    assert_eq!(context["grilling"], id.to_string());
    let listed: Value = serde_json::from_str(context["issues"].as_str().unwrap()).unwrap();
    assert_eq!(listed, final_list["issues"]);

    // Two cards in the Backlog, the blocker first, each confirmed; the other depends on it.
    let cards = server.app.store.cards(repo).unwrap();
    let titles: Vec<_> = cards.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Add the export button", "Write the CSV"]);
    assert!(cards.iter().all(|c| c.column == "Backlog" && c.confirmed));
    assert_eq!(cards[1].body, "Edited.");
    assert_eq!(
        server.app.store.dependencies(cards[0].id).unwrap(),
        Vec::<i64>::new()
    );
    assert_eq!(
        server.app.store.dependencies(cards[1].id).unwrap(),
        [cards[0].number]
    );

    // backlog.luau moves them on without approval; ready.luau holds the second one until the first is done.
    for card in &cards {
        let step = server.engine.tick(card.id).unwrap();
        assert_eq!(step.moved.as_deref(), Some("Ready"));
    }
    assert!(item_kinds(&server).is_empty(), "no approval was asked for");
    let step = server.engine.tick(cards[1].id).unwrap();
    assert_eq!(step.moved, None);
    assert_eq!(step.waiting.as_deref(), Some("waiting for #1"));
    assert_eq!(server.app.card(cards[1].id).unwrap().column, "Ready");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_report_waits_to_be_read_and_dismissed() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let api = Api::start(&server).await;

    let run = api
        .grill(&server, repo, &[("report.md", "# Findings\nIt depends.")])
        .await;
    let (_, grillings) = api.get("/v1/grillings").await;
    assert_eq!(grillings[0]["status"], "reported");
    assert_eq!(grillings[0]["run_id"], run);
    assert_eq!(grillings[0]["issues"], json!([]));
    let report = grillings[0]["report"].as_str().unwrap();
    assert!(report.starts_with("# Findings"), "{report}");
    let id = grillings[0]["id"].as_i64().unwrap();
    assert_eq!(item_kinds(&server), ["grilling_report"]);
    let item = &server.app.store.inbox().unwrap()[0];
    assert_eq!(item.data["report"], report);

    // A report has no issues to confirm.
    let (status, _) = api
        .post(
            &format!("/v1/grillings/{id}/confirm"),
            json!({ "issues": [{ "title": "Nope" }] }),
        )
        .await;
    assert_eq!(status, 422);
    let (status, dismissed) = api
        .post(&format!("/v1/grillings/{id}/dismiss"), json!({}))
        .await;
    assert_eq!(status, 200, "{dismissed}");
    assert_eq!(dismissed["status"], "dismissed");
    assert!(item_kinds(&server).is_empty());
    assert!(server.app.store.cards(repo).unwrap().is_empty());
    let (status, _) = api
        .post(&format!("/v1/grillings/{id}/dismiss"), json!({}))
        .await;
    assert_eq!(status, 422);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_grilling_with_no_usable_result_is_empty_and_asks_for_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let api = Api::start(&server).await;

    api.grill(&server, repo, &[]).await;
    api.grill(&server, repo, &[("issues.json", "[{\"title\": ")])
        .await;
    let (_, grillings) = api.get("/v1/grillings").await;
    let grillings = grillings.as_array().unwrap();
    assert_eq!(grillings.len(), 2);
    // Newest first.
    assert_eq!(grillings[0]["status"], "empty");
    assert!(
        grillings[0]["reason"]
            .as_str()
            .unwrap()
            .contains("issues.json")
    );
    assert_eq!(grillings[1]["status"], "empty");
    assert!(grillings[1]["reason"].as_str().unwrap().contains("neither"));
    assert!(server.app.store.inbox().unwrap().is_empty());
    let (status, _) = api
        .post(
            &format!("/v1/grillings/{}/dismiss", grillings[0]["id"]),
            json!({}),
        )
        .await;
    assert_eq!(status, 422);
    let (status, _) = api.get("/v1/grillings/99").await;
    assert_eq!(status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_grilling_needs_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _) = server(dir.path());
    let api = Api::start(&server).await;
    let (status, why) = api
        .post(
            "/v1/chats",
            json!({ "agent": "Grilling-partner", "message": "An idea." }),
        )
        .await;
    assert_eq!(status, 422, "{why}");
    assert!(why.as_str().unwrap().contains("repository"), "{why}");
    assert!(server.app.store.chats().unwrap().is_empty());
}

/// What the fake GitHub was asked to do.
#[derive(Default)]
struct FakeGitHub {
    /// The issues opened, as GitHub lists them.
    issues: Mutex<Vec<Value>>,
    /// The requests that opened them.
    opened: Mutex<Vec<Value>>,
}

/// The number of the first issue the fake GitHub opens.
const FIRST_NUMBER: i64 = 41;

async fn fake_github() -> (String, Arc<FakeGitHub>) {
    let fake = Arc::new(FakeGitHub::default());
    let app = axum::Router::new()
        .route(
            "/repos/acme/widgets/installation",
            get(|| async { axum::Json(json!({ "id": 77 })) }),
        )
        .route(
            "/app/installations/77/access_tokens",
            post(|| async {
                let expires = time::OffsetDateTime::now_utc() + Duration::from_secs(3600);
                (
                    StatusCode::CREATED,
                    axum::Json(json!({
                        "token": "ghs_token",
                        "expires_at": expires
                            .format(&time::format_description::well_known::Rfc3339)
                            .unwrap(),
                    })),
                )
            }),
        )
        .route(
            "/repos/acme/widgets/issues",
            get(|State(fake): State<Arc<FakeGitHub>>| async move {
                axum::Json(fake.issues.lock().unwrap().clone())
            })
            .post(
                |State(fake): State<Arc<FakeGitHub>>, axum::Json(body): axum::Json<Value>| async move {
                    let mut issues = fake.issues.lock().unwrap();
                    let number = FIRST_NUMBER + issues.len() as i64;
                    issues.push(json!({
                        "number": number, "title": body["title"], "body": body["body"],
                        "state": "open", "user": { "login": "moebius[bot]" },
                    }));
                    fake.opened.lock().unwrap().push(body);
                    (StatusCode::CREATED, axum::Json(json!({ "number": number })))
                },
            ),
        )
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (url, fake)
}

#[tokio::test(flavor = "multi_thread")]
async fn confirming_on_a_github_repository_opens_the_issues_and_the_cards_carry_their_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let (url, fake) = fake_github().await;
    let key = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/github-app-test-key.pem");
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[agents.Coder]\nmodel = \"coder-model\"\n\
         [github]\napp_id = 4242\nprivate_key_file = {:?}\napi_url = {url:?}\n",
        fake_pi(),
        key.display().to_string()
    );
    let config = Config::parse(&text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
    let server = Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap());
    let remote = dir.path().join("remote.git");
    git(
        dir.path(),
        &[
            "clone",
            "--quiet",
            "--bare",
            source(dir.path()).to_str().unwrap(),
            "remote.git",
        ],
    )
    .unwrap();
    let repo = server
        .app
        .add_github_repository("widgets", remote.to_str().unwrap(), "acme/widgets", &[])
        .unwrap();
    let api = Api::start(&server).await;

    api.grill(&server, repo.id, &[("issues.json", TWO_ISSUES)])
        .await;
    let (_, grillings) = api.get("/v1/grillings").await;
    let id = grillings[0]["id"].as_i64().unwrap();
    assert_eq!(grillings[0]["status"], "proposed");
    let (status, confirmed) = api
        .post(
            &format!("/v1/grillings/{id}/confirm"),
            json!({ "issues": grillings[0]["issues"] }),
        )
        .await;
    assert_eq!(status, 200, "{confirmed}");

    // Two issues were opened, the blocker first, the second saying what blocks it the way the poll reads it.
    let opened = fake.opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 2);
    assert_eq!(opened[0]["title"], "Add the export button");
    assert_eq!(opened[0]["body"], "A button on the report page.");
    assert_eq!(
        opened[1]["body"],
        "The columns are the report's.\n\nBlocked by #41"
    );

    // The cards have the numbers of the issues and wait for nobody's approval.
    let cards = server.app.store.cards(repo.id).unwrap();
    let numbers: Vec<_> = cards.iter().map(|c| c.number).collect();
    assert_eq!(numbers, [41, 42]);
    assert!(cards.iter().all(|c| c.column == "Backlog" && c.confirmed));
    assert_eq!(server.app.store.dependencies(cards[1].id).unwrap(), [41]);

    // The poll finds the cards existing: it makes no others and keeps them confirmed and blocked.
    server.poll_github().await;
    let polled = server.app.store.cards(repo.id).unwrap();
    assert_eq!(polled.len(), 2);
    assert!(polled.iter().all(|c| c.confirmed));
    assert_eq!(server.app.store.dependencies(polled[1].id).unwrap(), [41]);
    for card in &polled {
        let step = server.engine.tick(card.id).unwrap();
        assert_eq!(step.moved.as_deref(), Some("Ready"));
    }
    assert!(item_kinds(&server).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn creating_an_issue_on_a_github_repository_opens_it_and_the_card_moves_on() {
    let dir = tempfile::tempdir().unwrap();
    let (url, fake) = fake_github().await;
    let key = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/github-app-test-key.pem");
    let text = format!(
        "[github]\napp_id = 4242\nprivate_key_file = {:?}\napi_url = {url:?}\n",
        key.display().to_string()
    );
    let config = Config::parse(&text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
    let server = Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap());
    let remote = dir.path().join("remote.git");
    git(
        dir.path(),
        &[
            "clone",
            "--quiet",
            "--bare",
            source(dir.path()).to_str().unwrap(),
            "remote.git",
        ],
    )
    .unwrap();
    let repo = server
        .app
        .add_github_repository("widgets", remote.to_str().unwrap(), "acme/widgets", &[])
        .unwrap();
    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let plain = server
        .app
        .add_repository("plain", source(&other).to_str().unwrap())
        .unwrap();
    let api = Api::start(&server).await;

    let (status, card) = api
        .post(
            &format!("/v1/repositories/{}/issues", repo.id),
            json!({ "title": "Add a button", "body": "Somewhere." }),
        )
        .await;
    assert_eq!(status, 200, "{card}");
    assert_eq!(card["number"], FIRST_NUMBER);
    assert_eq!(card["column"], "Backlog");
    assert_eq!(card["author"], "person");
    assert_eq!(card["confirmed"], true);
    let opened = fake.opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0]["title"], "Add a button");
    assert_eq!(opened[0]["body"], "Somewhere.");

    let (status, why) = api
        .post(
            &format!("/v1/repositories/{}/issues", repo.id),
            json!({ "title": "  " }),
        )
        .await;
    assert_eq!(status, 422, "{why}");
    assert_eq!(fake.opened.lock().unwrap().len(), 1);

    // The poll finds the card existing, and it moves on without a person's approval.
    server.poll_github().await;
    let cards = server.app.store.cards(repo.id).unwrap();
    assert_eq!(cards.len(), 1);
    let step = server.engine.tick(cards[0].id).unwrap();
    assert_eq!(step.moved.as_deref(), Some("Ready"));

    // Without a forge the issue stays local.
    let (status, local) = api
        .post(
            &format!("/v1/repositories/{}/issues", plain.id),
            json!({ "title": "Local" }),
        )
        .await;
    assert_eq!(status, 200, "{local}");
    assert_eq!(local["number"], 1);
    assert_eq!(fake.opened.lock().unwrap().len(), 1);
}
