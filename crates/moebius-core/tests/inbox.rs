//! The inbox over HTTP, as the apps use it: an outside issue approved, a change merged, and a request the server
//! refuses coming back with its status. (The decisions behind them, and the board's own questions, are tested with the
//! engine and the application; this is the route.)

use std::path::{Path, PathBuf};
use std::sync::Arc;

use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
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

/// A server with a repository `demo` without a forge, and the API on a free port.
async fn start(dir: &Path) -> (Arc<Server>, moebius_core::store::Repository, String) {
    let text = "[router]\nlisten = \"127.0.0.1:0\"\n";
    let config = Config::parse(text, dir, &dir.join("moebius.toml")).unwrap();
    let server = Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap());
    let repo = server
        .app
        .add_repository("demo", source(dir).to_str().unwrap())
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let state = ApiState {
        sessions: Default::default(),
        app: server.app.clone(),
        runs: server.harness.clone(),
        harness_repo: server.harness_repo.clone(),
        token: server.token.clone(),
        router: server.router.clone(),
        config: server.live.clone(),
    };
    tokio::spawn(api::serve(listener, state));
    (server, repo, base)
}

async fn post(server: &Server, base: &str, path: &str, body: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .bearer_auth(&*server.token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn an_outside_issue_is_approved_and_leaves_the_inbox_and_a_merged_change_is_not_merged_again()
{
    let dir = tempfile::tempdir().unwrap();
    let (server, repo, base) = start(dir.path()).await;
    let repository = repo.id;
    let app = &server.app;

    // An issue by a stranger waits in the Backlog for a person, who approves it.
    let outside = app
        .store
        .add_card(repository, "An outside issue", "", "stranger", "Backlog")
        .unwrap();
    app.store
        .add_inbox_item(
            "approve_issue",
            Some(outside.id),
            "An outside issue",
            &json!({}),
        )
        .unwrap();
    let (status, _) = post(
        &server,
        &base,
        "/v1/cards/999/approve",
        json!({"reason": "x"}),
    )
    .await;
    assert_eq!(status, 404);
    let (status, card) = post(
        &server,
        &base,
        &format!("/v1/cards/{}/approve", outside.id),
        json!({"reason": "Known contributor."}),
    )
    .await;
    assert_eq!(status, 200, "{card}");
    assert_ne!(card["column"], "Backlog");
    assert!(app.store.inbox().unwrap().is_empty());
    let judgements = app.store.judgements("approve_issue").unwrap();
    assert_eq!(
        (
            judgements[0].decider.as_str(),
            judgements[0].reason.as_str()
        ),
        ("person", "Known contributor.")
    );

    // A coded change is merged by a person; asking again is refused with a status the apps can show.
    let card = app
        .create_issue(repository, "Add a feature", "", "person")
        .unwrap();
    let clone = PathBuf::from(&repo.path);
    let run = app
        .store
        .add_run(Some(card.id), "Coder", "test", "code")
        .unwrap();
    app.store.set_run_status(run.id, "succeeded").unwrap();
    let worktree = app
        .repos
        .add_worktree("demo", &clone, run.id, "main")
        .unwrap();
    std::fs::write(worktree.path.join("feature.txt"), "done\n").unwrap();
    git(&worktree.path, &["add", "."]).unwrap();
    git(
        &worktree.path,
        &["commit", "--quiet", "-m", "Add the feature"],
    )
    .unwrap();
    app.repos.remove_worktree(&clone, &worktree.path).unwrap();
    let change = app.offer_change(card.id).unwrap();
    let merge = format!("/v1/changes/{}/merge", change.id);
    let (status, merged) = post(&server, &base, &merge, json!({"reason": "ship it"})).await;
    assert_eq!(status, 200, "{merged}");
    assert_eq!(app.card(card.id).unwrap().column, "Done");
    assert!(clone.join("feature.txt").is_file());
    let (status, again) = post(&server, &base, &merge, json!({"reason": "again"})).await;
    assert_eq!(status, 422, "{again}");
}
