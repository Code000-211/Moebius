//! Replays: a refused tool call is run again in a throwaway copy of the worktree and what happened becomes the
//! judgement's outcome, while the worktree of the run stays as it was. The run is a fake Pi that waits in its worktree;
//! the calls are refused by the guard, which fails closed on a prose rule when no teacher is configured.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use moebius_core::api::{self, ApiState};
use moebius_core::app::PERSON;
use moebius_core::board::Runs;
use moebius_core::bus::AppEvent;
use moebius_core::config::Config;
use moebius_core::decide::Question;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::{replay, store::Run};
use serde_json::{Value, json};

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A repository with three files in `src`.
fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    for name in ["a", "b", "c"] {
        write(&source, &format!("src/{name}.rs"), "fn main() {}\n");
    }
    write(&source, "README.md", "hello\n");
    git(&source, &["add", "-A"]).unwrap();
    git(&source, &["commit", "--quiet", "-m", "first"]).unwrap();
    source
}

/// A harness configuration repository with the Coder; returns the bare repository the server clones.
fn harness_origin(dir: &Path) -> PathBuf {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&work, "agents/Coder.toml", "model = \"coder-model\"\n");
    write(&work, "agents/Coder.md", "Keep changes small.\n");
    git(&work, &["add", "-A"]).unwrap();
    git(&work, &["commit", "--quiet", "-m", "Start"]).unwrap();
    let bare = dir.join("harness.git");
    git(
        dir,
        &["init", "--quiet", "--bare", "-b", "main", "harness.git"],
    )
    .unwrap();
    git(&work, &["push", "--quiet", bare.to_str().unwrap(), "main"]).unwrap();
    bare
}

fn server(dir: &Path) -> Arc<Server> {
    let harness = harness_origin(dir);
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\n",
        fake_pi.display().to_string(),
        harness.display().to_string()
    );
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap())
}

/// A run of the fake Pi that waits in its worktree, once the worktree is there.
fn waiting_run(server: &Server, dir: &Path) -> Run {
    let repo = server
        .app
        .add_repository("demo", source(dir).to_str().unwrap())
        .unwrap();
    let card = server
        .app
        .create_issue(repo.id, "Tidy up [hang]", "", PERSON)
        .unwrap();
    let run = server
        .harness
        .start(&server.app, &card, "Coder", "code", None)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let run = server.app.store.run(run.id).unwrap().unwrap();
        if run.status == "running" && run.worktree.is_some() {
            return run;
        }
        assert!(Instant::now() < deadline, "the run did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_until_ended(server: &Server, run: i64) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while server
        .app
        .store
        .run(run)
        .unwrap()
        .unwrap()
        .ended_at
        .is_none()
    {
        assert!(Instant::now() < deadline, "the run did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A tool call of `run` that the guard refuses: a prose rule applies and there is no teacher to judge it, so it
/// fails closed. Returns the judgement's id.
fn refused(server: &Server, run: &Run, tool: &str, input: Value) -> i64 {
    let context = BTreeMap::from([
        ("tool".to_string(), tool.to_string()),
        ("input".to_string(), input.to_string()),
        ("tool_call_id".to_string(), "call-1".to_string()),
        ("run_id".to_string(), run.id.to_string()),
        ("card_id".to_string(), run.card_id.unwrap().to_string()),
        ("worktree".to_string(), run.worktree.clone().unwrap()),
        (
            "rules".to_string(),
            "## keep-src\nNever touch src.".to_string(),
        ),
    ]);
    let decision = server
        .app
        .decisions
        .decide(&Question {
            kind: "tool_call".into(),
            context,
            options: vec!["allow".into(), "refuse".into()],
            fail_closed: "refuse".into(),
            run_id: Some(run.id),
            card_id: run.card_id,
        })
        .unwrap();
    assert_eq!(decision.choice, "refuse");
    decision.judgement_id
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
            round_limit: 5,
            day_budget: None,
            config: std::sync::Arc::new(
                moebius_core::config::Config::parse(
                    "",
                    std::path::Path::new("."),
                    std::path::Path::new("moebius.toml"),
                )
                .unwrap(),
            ),
        };
        tokio::spawn(api::serve(listener, state));
        Self {
            base,
            token: server.token.to_string(),
            client: reqwest::Client::new(),
        }
    }

    async fn call(&self, method: reqwest::Method, path: &str) -> (u16, Value) {
        let response = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
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

    async fn replay(&self, judgement: i64) -> (u16, Value) {
        let path = format!("/v1/judgements/{judgement}/replay");
        self.call(reqwest::Method::POST, &path).await
    }
}

/// The files of `dir`, with `.git` left out.
fn files(dir: &Path) -> Vec<String> {
    replay::listing(dir)
        .unwrap()
        .into_keys()
        .filter(|name| !name.starts_with(".git"))
        .collect()
}

fn leftovers(server: &Server) -> Vec<String> {
    match std::fs::read_dir(server.app.data_dir.join("replays")) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => vec![],
    }
}

#[tokio::test]
async fn a_replayed_refusal_gets_an_outcome_and_the_worktree_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let run = waiting_run(&server, dir.path());
    let worktree = PathBuf::from(run.worktree.clone().unwrap());
    let before = files(&worktree);
    assert!(before.contains(&"src/a.rs".to_string()));
    let api = Api::start(&server).await;
    let mut events = server.app.bus.subscribe();
    let store = &server.app.store;

    let rm = refused(&server, &run, "bash", json!({ "command": "rm -rf src" }));
    let (status, replayed) = api.replay(rm).await;
    assert_eq!(status, 200, "{replayed}");
    assert_eq!(replayed["judgement"], rm);
    assert_eq!(replayed["run"], run.id);
    assert_eq!(replayed["exit"], 0);
    assert_eq!(replayed["timed_out"], false);
    assert_eq!(
        replayed["deleted"],
        json!(["src/a.rs", "src/b.rs", "src/c.rs"])
    );
    assert_eq!(replayed["created"], json!([]));

    // The judgement has the outcome, and the event says so.
    let outcome = "replayed: exit 0; deleted 3 files";
    assert_eq!(
        store.judgement(rm).unwrap().unwrap().outcome.as_deref(),
        Some(outcome)
    );
    let event = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let event = events.recv().await.unwrap();
            if event.kind == "replayed" {
                return event;
            }
        }
    })
    .await
    .expect("no replayed event");
    assert_eq!(
        event,
        AppEvent::new(
            "replayed",
            json!({"judgement": rm, "run": run.id, "outcome": outcome})
        )
    );

    // The original worktree has everything it had, and git sees no change; the copy is gone.
    assert_eq!(files(&worktree), before);
    assert_eq!(git(&worktree, &["status", "--porcelain"]).unwrap(), "");
    assert_eq!(leftovers(&server), Vec::<String>::new());

    // A call is replayed once.
    let (status, again) = api.replay(rm).await;
    assert_eq!(status, 409, "{again}");
    assert_eq!(store.replays(None, None, 10).unwrap().len(), 1);

    // The replays are listed by judgement and by run.
    for filter in [format!("judgement={rm}"), format!("run={}", run.id)] {
        let (status, listed) = api
            .call(reqwest::Method::GET, &format!("/v1/replays?{filter}"))
            .await;
        assert_eq!(status, 200);
        assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
        assert_eq!(listed[0]["id"], replayed["id"]);
    }
    let (_, none) = api
        .call(reqwest::Method::GET, "/v1/replays?judgement=9999")
        .await;
    assert_eq!(none, json!([]));

    // Once the run is over its worktree is gone, and the copy is a checkout of the repository.
    server
        .app
        .stop_run(&*server.harness, run.id, "done")
        .unwrap();
    wait_until_ended(&server, run.id);
    assert!(!worktree.exists());
    let later = refused(
        &server,
        &run,
        "bash",
        json!({ "command": "rm -rf src; ls | wc -l" }),
    );
    let (status, replayed) = api.replay(later).await;
    assert_eq!(status, 200, "{replayed}");
    assert_eq!(replayed["deleted"].as_array().unwrap().len(), 3);
    assert_eq!(replayed["output"], "1");
    assert_eq!(
        store.judgement(later).unwrap().unwrap().outcome.as_deref(),
        Some(outcome)
    );
    assert_eq!(leftovers(&server), Vec::<String>::new());
}

#[tokio::test]
async fn a_call_that_is_not_reversible_is_refused_by_the_api_and_never_picked() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let run = waiting_run(&server, dir.path());
    let api = Api::start(&server).await;
    let store = &server.app.store;

    let push = refused(
        &server,
        &run,
        "bash",
        json!({ "command": "git push origin main" }),
    );
    let outside = refused(
        &server,
        &run,
        "write",
        json!({ "path": "/etc/x", "content": "x" }),
    );
    let (status, body) = api.replay(push).await;
    assert_eq!(status, 422, "{body}");
    assert!(body.as_str().unwrap().contains("git push"), "{body}");
    assert_eq!(api.replay(outside).await.0, 422);
    assert_eq!(api.replay(9999).await.0, 404);

    // A call that was allowed, and a decision that is not about a tool call, are not replayed either.
    let allowed = server
        .app
        .decisions
        .decide(&Question {
            kind: "tool_call".into(),
            context: BTreeMap::from([
                ("tool".to_string(), "bash".to_string()),
                ("input".to_string(), json!({ "command": "ls" }).to_string()),
                ("worktree".to_string(), run.worktree.clone().unwrap()),
            ]),
            options: vec!["allow".into(), "refuse".into()],
            fail_closed: "refuse".into(),
            run_id: Some(run.id),
            card_id: run.card_id,
        })
        .unwrap();
    assert_eq!(allowed.choice, "allow");
    assert_eq!(api.replay(allowed.judgement_id).await.0, 422);
    let person = server
        .app
        .decisions
        .record_person(
            &Question {
                kind: "stop".into(),
                run_id: Some(run.id),
                ..Default::default()
            },
            "stop",
            "enough",
        )
        .unwrap();
    assert_eq!(api.replay(person.judgement_id).await.0, 422);

    // The schedule picks the calls that can be replayed, the oldest first, and skips the others.
    let touch = refused(
        &server,
        &run,
        "bash",
        json!({ "command": "echo hi > new.txt && cat new.txt" }),
    );
    let write = refused(
        &server,
        &run,
        "write",
        json!({ "path": "docs/x.md", "content": "text" }),
    );
    // A card that is done gives its judgements its outcome; the replay replaces it.
    store.set_outcome(write, "merged").unwrap();
    let picked: Vec<i64> = replay::due(&server.app, 10)
        .unwrap()
        .iter()
        .map(|j| j.id)
        .collect();
    assert_eq!(picked, [touch, write]);
    assert_eq!(replay::due(&server.app, 1).unwrap().len(), 1);

    assert_eq!(
        replay::run_due(&server.app, 10, replay::TIME_LIMIT).unwrap(),
        2
    );
    let outcome = |id| store.judgement(id).unwrap().unwrap().outcome;
    assert_eq!(
        outcome(touch).as_deref(),
        Some("replayed: exit 0; created 1 file")
    );
    assert_eq!(
        outcome(write).as_deref(),
        Some("replayed: exit 0; created 1 file")
    );
    assert_eq!(outcome(push), None);
    assert_eq!(store.replays(None, None, 10).unwrap().len(), 2);
    let replayed = store.replays(Some(touch), None, 1).unwrap().pop().unwrap();
    assert_eq!(
        (replayed.output.as_str(), replayed.created),
        ("hi", vec!["new.txt".to_string()])
    );

    // Nothing is left to do, and nothing of it reached the worktree.
    assert_eq!(
        replay::run_due(&server.app, 10, replay::TIME_LIMIT).unwrap(),
        0
    );
    let worktree = PathBuf::from(run.worktree.clone().unwrap());
    assert!(!worktree.join("new.txt").exists() && !worktree.join("docs").exists());
    assert_eq!(leftovers(&server), Vec::<String>::new());
    server
        .app
        .stop_run(&*server.harness, run.id, "done")
        .unwrap();
}

#[tokio::test]
async fn a_command_that_hangs_is_killed_at_the_time_limit() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let run = waiting_run(&server, dir.path());
    let hang = refused(
        &server,
        &run,
        "bash",
        json!({ "command": "echo started; touch left.txt; sleep 300" }),
    );
    let started = Instant::now();
    let replayed = replay::replay(&server.app, hang, Duration::from_millis(700)).unwrap();
    assert!(started.elapsed() < Duration::from_secs(120));
    assert!(replayed.timed_out);
    assert_eq!(replayed.exit, None);
    assert_eq!(replayed.output, "started");
    assert_eq!(replayed.created, ["left.txt"]);
    assert_eq!(
        server
            .app
            .store
            .judgement(hang)
            .unwrap()
            .unwrap()
            .outcome
            .as_deref(),
        Some("replayed: timed out; created 1 file")
    );
    assert_eq!(leftovers(&server), Vec::<String>::new());
    server
        .app
        .stop_run(&*server.harness, run.id, "done")
        .unwrap();
}

#[tokio::test]
async fn secrets_in_what_a_call_printed_are_not_stored() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let run = waiting_run(&server, dir.path());
    let token = format!("ghp_{}", "a".repeat(36));
    write(
        &PathBuf::from(run.worktree.clone().unwrap()),
        "notes.txt",
        &format!("token {token}\n"),
    );
    let read = refused(&server, &run, "read", json!({ "path": "notes.txt" }));
    let replayed = replay::replay(&server.app, read, replay::TIME_LIMIT).unwrap();
    assert_eq!(replayed.output, "token [redacted:github-token]");
    assert_eq!(replayed.exit, Some(0));
    server
        .app
        .stop_run(&*server.harness, run.id, "done")
        .unwrap();
}
