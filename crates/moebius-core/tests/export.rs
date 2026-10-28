//! The training-set export: judgements of several kinds made through real paths (a tool check through the guard, a
//! person's decision through the API, a replay, a card's outcome) are exported through the API and every stored
//! judgement is in the files exactly once, with its context and outcome. The run is a fake Pi that waits in its
//! worktree.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use moebius_core::api::{self, ApiState};
use moebius_core::app::PERSON;
use moebius_core::board::Runs;
use moebius_core::config::Config;
use moebius_core::decide::Question;
use moebius_core::export::held_out;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::store::Run;
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

    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let response = self
            .client
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
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

    /// The status, the content type and the body of a download.
    async fn download(&self, path: &str) -> (u16, String, String) {
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let kind = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        (status, kind, response.text().await.unwrap())
    }
}

/// The lines of a file as JSON.
fn lines(text: &str) -> Vec<Value> {
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[tokio::test]
async fn every_stored_judgement_is_in_the_export_once_with_its_context_and_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path());
    let run = waiting_run(&server, dir.path());
    let api = Api::start(&server).await;
    let store = &server.app.store;
    let card = run.card_id.unwrap();

    // Two refused tool calls through the guard; one is replayed, the other gets the card's outcome.
    let secret = format!("sk-ant-api03-{}", "k".repeat(40));
    let rm = refused(&server, &run, "bash", json!({ "command": "rm -rf src" }));
    let ls = refused(
        &server,
        &run,
        "bash",
        json!({ "command": format!("echo {secret}") }),
    );
    let (status, replayed) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/judgements/{rm}/replay"),
        )
        .await;
    assert_eq!(status, 200, "{replayed}");
    assert_eq!(store.set_missing_outcomes(card, "merged").unwrap(), 1);

    // A person's decision through the API.
    let (status, stopped) = api
        .post(
            &format!("/v1/runs/{}/stop", run.id),
            json!({"reason": "enough"}),
        )
        .await;
    assert_eq!(status, 200, "{stopped}");

    let mut stored = Vec::new();
    for kind in ["tool_call", "stop"] {
        stored.extend(store.judgements(kind).unwrap());
    }
    assert!(stored.len() >= 3);
    let outcome = |id| store.judgement(id).unwrap().unwrap().outcome;
    assert!(outcome(rm).unwrap().starts_with("replayed: exit 0"));
    assert_eq!(outcome(ls).as_deref(), Some("merged"));

    // The export through the API.
    let (status, made) = api.post("/v1/exports", json!({})).await;
    assert_eq!(status, 200, "{made}");
    assert_eq!(made["id"], 1);
    let kinds: Vec<&str> = made["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["stop", "tool_call"]);
    let tool_calls = &made["kinds"][1];
    assert_eq!(tool_calls["count"], 2);
    assert_eq!(tool_calls["with_outcome"], 2);
    assert_eq!(tool_calls["by_person"], 0);
    assert_eq!(made["kinds"][0]["by_person"], 1);

    let folder = server.app.data_dir.join("exports/1");
    let mut exported = Vec::new();
    for kind in ["stop", "tool_call"] {
        let text = std::fs::read_to_string(folder.join(format!("{kind}.jsonl"))).unwrap();
        exported.extend(lines(&text));
        // The download is the same file.
        let (status, content_type, body) =
            api.download(&format!("/v1/exports/1/{kind}.jsonl")).await;
        assert_eq!(
            (status, content_type.as_str()),
            (200, "application/x-ndjson")
        );
        assert_eq!(body, text);
    }
    let mut ids: Vec<i64> = exported.iter().map(|l| l["id"].as_i64().unwrap()).collect();
    ids.sort();
    let mut expected: Vec<i64> = stored.iter().map(|j| j.id).collect();
    expected.sort();
    assert_eq!(ids, expected, "every judgement is there exactly once");
    for line in &exported {
        let j = store
            .judgement(line["id"].as_i64().unwrap())
            .unwrap()
            .unwrap();
        let blob = store.blobs().get(&j.context[0]).unwrap();
        assert_eq!(
            line["context"],
            serde_json::from_str::<Value>(&blob).unwrap()
        );
        assert_eq!(line["outcome"], json!(j.outcome));
        assert_eq!(line["choice"], j.choice.as_str());
        assert_eq!(line["decider"], j.decider.as_str());
        assert_eq!(line["run"], run.id);
        assert_eq!(line["card"], card);
        assert_eq!(
            line["split"],
            if held_out(j.id) { "held_out" } else { "train" }
        );
    }
    let text = std::fs::read_to_string(folder.join("tool_call.jsonl")).unwrap();
    assert!(!text.contains("sk-ant-api03") && text.contains("[redacted:anthropic-key]"));

    // The person's decision to export was recorded, with the counts.
    let decisions = store.judgements("export").unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].decider, "person");
    let context: Value =
        serde_json::from_str(&store.blobs().get(&decisions[0].context[0]).unwrap()).unwrap();
    assert_eq!(context["kinds"], "stop,tool_call");
    assert_eq!(context["export"], "1");
    assert_eq!(
        serde_json::from_str::<Value>(context["counts"].as_str().unwrap()).unwrap(),
        made["kinds"]
    );

    // Exports are listed, newest first; a second one can be of one kind; what is stored is counted per kind.
    let (status, second) = api.post("/v1/exports", json!({"kinds": ["stop"]})).await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second["id"], 2);
    assert!(
        !server
            .app
            .data_dir
            .join("exports/2/tool_call.jsonl")
            .exists()
    );
    let (_, listed) = api.call(reqwest::Method::GET, "/v1/exports").await;
    assert_eq!(listed, json!([second, made]));
    let (_, counted) = api.call(reqwest::Method::GET, "/v1/judgements/kinds").await;
    let names: Vec<&str> = counted
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["export", "stop", "tool_call"]);
    assert_eq!(counted[0]["by_person"], 2);
    assert_eq!(counted[2], made["kinds"][1]);

    // A kind there is none of, and a file there is not, are refused.
    let (status, _) = api.post("/v1/exports", json!({"kinds": ["nothing"]})).await;
    assert_eq!(status, 422);
    assert!(!server.app.data_dir.join("exports/3").exists());
    for path in ["/v1/exports/1/nothing.jsonl", "/v1/exports/9/stop.jsonl"] {
        assert_eq!(api.download(path).await.0, 404, "{path}");
    }
}
