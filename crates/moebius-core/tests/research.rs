//! Autoresearch: a brief a person confirms, a baseline, and the Autoresearcher (a fake Pi here) that proposes levers,
//! each measured on a branch of its own; the lever that beats the baseline is offered as a change.

// Unix only: the fake Pi (testdata/fake-pi) is a Python script started by its `#!` line. The measures run with `sh`.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use moebius_core::board::Runs;
use moebius_core::bus::AppEvent;
use moebius_core::config::{Config, TimeOfDay};
use moebius_core::improve::research;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use moebius_core::store::Usage;
use serde_json::{Value, json};

/// A repository whose `score` file holds 100.
fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(source.join("score"), "100\n").unwrap();
    git(&source, &["add", "-A"]).unwrap();
    git(&source, &["commit", "--quiet", "-m", "first"]).unwrap();
    source
}

/// A server with the fake Pi and a repository `demo`; returns the server and the repository's id.
fn server(dir: &Path) -> (Arc<Server>, i64) {
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[agents.Coder]\nmodel = \"coder-model\"\n",
        fake_pi.display().to_string()
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
            router: server.router.clone(),
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

    /// Stores a brief for the repository and confirms it; returns the research's id.
    async fn confirmed(&self, repository: i64, goal: &str, measure: &str, budget: i64) -> i64 {
        let brief = json!({ "repository": repository, "goal": goal, "measure": measure,
                            "better": "lower", "token_budget": budget });
        let (status, created) = self.post("/v1/research", brief).await;
        assert_eq!(status, 200, "{created}");
        assert_eq!(created["status"], "draft");
        let id = created["id"].as_i64().unwrap();
        let (status, confirmed) = self
            .post(&format!("/v1/research/{id}/confirm"), json!({}))
            .await;
        assert_eq!(status, 200, "{confirmed}");
        assert_eq!(confirmed["status"], "confirmed");
        id
    }
}

/// Waits for the research to end and returns the events that came meanwhile.
async fn ended(
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    research: i64,
) -> (String, Vec<AppEvent>) {
    let wait = async {
        let mut seen = vec![];
        loop {
            let event = events.recv().await.unwrap();
            if event.kind == "research_status"
                && event.data["research"] == research
                && !matches!(
                    event.data["status"].as_str(),
                    Some("draft" | "confirmed" | "running")
                )
            {
                let status = event.data["status"].as_str().unwrap().to_string();
                seen.push(event);
                return (status, seen);
            }
            seen.push(event);
        }
    };
    tokio::time::timeout(Duration::from_secs(120), wait)
        .await
        .expect("the research did not end")
}

fn wait_for<T>(what: &str, mut found: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(found) = found() {
            return found;
        }
        assert!(Instant::now() < deadline, "{what} did not happen");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn prompt_of(dir: &Path, run: i64) -> String {
    let file = dir.join(format!(".moebius/runs/{run}/agent/prompt.txt"));
    std::fs::read_to_string(file).unwrap()
}

#[tokio::test]
async fn a_baseline_and_two_experiments_keep_the_better_lever() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let clone = PathBuf::from(server.app.store.repository(repo).unwrap().unwrap().path);
    let main_before = git(&clone, &["rev-parse", "main"]).unwrap();
    let api = Api::start(&server).await;
    let goal =
        "Make the score smaller. [lever research/1-down score 80] [lever research/1-up score 120]";
    let id = api.confirmed(repo, goal, "cat score", 1_000_000).await;
    assert_eq!(id, 1);
    let mut events = server.app.bus.subscribe();

    let (status, started) = api
        .post(&format!("/v1/research/{id}/start"), json!({}))
        .await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["status"], "running");
    // Only one research runs at a time, and a running one is not started again.
    let (status, _) = api
        .post(&format!("/v1/research/{id}/start"), json!({}))
        .await;
    assert_eq!(status, 409);
    let (status, seen) = ended(&mut events, id).await;
    assert_eq!(status, "done");

    let (_, research) = api.get(&format!("/v1/research/{id}")).await;
    assert_eq!(research["status"], "done");
    assert_eq!(research["baseline"], 100.0);
    assert_eq!(research["better"], "lower");
    assert_eq!(research["measure"], "cat score");
    let experiments = research["experiments"].as_array().unwrap();
    let summary: Vec<_> = experiments
        .iter()
        .map(|e| {
            (
                e["branch"].as_str().unwrap(),
                e["value"].as_f64(),
                e["kept"].as_bool(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("research/1-down", Some(80.0), Some(true)),
            ("research/1-up", Some(120.0), Some(false)),
        ]
    );
    assert_eq!(experiments[0]["title"], "Set score to 80");
    assert_eq!(experiments[0]["checks"], Value::Null);
    assert!(
        experiments[1]["reason"]
            .as_str()
            .unwrap()
            .contains("not better")
    );
    assert_eq!(experiments[1]["change_id"], Value::Null);

    // Only the kept lever is offered, as a change without a card that waits in the inbox.
    let change = experiments[0]["change_id"].as_i64().unwrap();
    let changes = server.app.store.changes(repo).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        (
            changes[0].id,
            changes[0].branch.as_str(),
            changes[0].card_id
        ),
        (change, "research/1-down", None)
    );
    let inbox = server.app.store.inbox().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].kind, "ready_to_merge");
    assert_eq!(inbox[0].data["change"], change);
    assert_eq!(inbox[0].data["branch"], "research/1-down");

    // The events: the experiments, then the end.
    let done: Vec<_> = seen
        .iter()
        .filter(|e| e.kind == "experiment_done")
        .map(|e| e.data.clone())
        .collect();
    assert_eq!(
        done,
        [
            json!({ "research": id, "lever": "research/1-down", "value": 80.0, "kept": true }),
            json!({ "research": id, "lever": "research/1-up", "value": 120.0, "kept": false }),
        ]
    );

    // Two rounds of the Autoresearcher, both with no card; the second was told what the first found.
    let runs = server.app.store.runs_of_research(id).unwrap();
    assert_eq!(runs.len(), 2);
    assert!(
        runs.iter()
            .all(|r| r.agent == "Autoresearcher" && r.card_id.is_none())
    );
    assert!(runs.iter().all(|r| r.status == "succeeded"));
    assert!(
        runs.iter()
            .all(|r| !PathBuf::from(r.worktree.clone().unwrap()).exists())
    );
    let first = prompt_of(dir.path(), runs[0].id);
    assert!(first.contains(goal), "{first}");
    assert!(first.contains("measures 100"), "{first}");
    assert!(first.contains("none yet"), "{first}");
    let second = prompt_of(dir.path(), runs[1].id);
    assert!(
        second.contains("research/1-down \"Set score to 80\": 80, kept"),
        "{second}"
    );
    assert!(
        second.contains("research/1-up \"Set score to 120\": 120, not kept"),
        "{second}"
    );

    // The repository was not touched: the base branch, its files and its worktrees are as they were.
    assert_eq!(git(&clone, &["rev-parse", "main"]).unwrap(), main_before);
    assert_eq!(
        std::fs::read_to_string(clone.join("score")).unwrap(),
        "100\n"
    );
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    let worktrees = git(&clone, &["worktree", "list", "--porcelain"]).unwrap();
    assert_eq!(worktrees.matches("worktree ").count(), 1, "{worktrees}");
    let lever = git(&clone, &["show", "research/1-down:score"]).unwrap();
    assert_eq!(lever, "80");
    let list = api.get("/v1/research").await.1;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["experiments"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_brief_is_checked_and_only_a_confirmed_one_starts() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let api = Api::start(&server).await;
    let brief = |change: Value| {
        let mut brief = json!({ "repository": repo, "goal": "g", "measure": "cat score",
                                "better": "lower", "token_budget": 10 });
        for (key, value) in change.as_object().unwrap() {
            brief[key] = value.clone();
        }
        brief
    };
    for bad in [
        json!({ "goal": " " }),
        json!({ "measure": "" }),
        json!({ "better": "smaller" }),
        json!({ "token_budget": 0 }),
    ] {
        let (status, body) = api.post("/v1/research", brief(bad.clone())).await;
        assert_eq!(status, 422, "{bad} {body}");
    }
    let (status, _) = api
        .post("/v1/research", brief(json!({ "repository": 99 })))
        .await;
    assert_eq!(status, 404);
    let (status, _) = api.get("/v1/research/7").await;
    assert_eq!(status, 404);

    let (_, draft) = api.post("/v1/research", brief(json!({}))).await;
    let id = draft["id"].as_i64().unwrap();
    assert_eq!(draft["baseline"], Value::Null);
    assert_eq!(draft["experiments"], json!([]));
    let (status, _) = api
        .post(&format!("/v1/research/{id}/start"), json!({}))
        .await;
    assert_eq!(status, 409);

    // The confirmation is a decision of a person.
    api.post(&format!("/v1/research/{id}/confirm"), json!({}))
        .await;
    let decisions = server.app.store.judgements("confirm_brief").unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(
        (decisions[0].choice.as_str(), decisions[0].decider.as_str()),
        ("confirm", "person")
    );
    let (status, _) = api
        .post(&format!("/v1/research/{id}/confirm"), json!({}))
        .await;
    assert_eq!(status, 409);

    // A research that has not run can be stopped; a stopped one cannot be started or stopped again.
    let (status, stopped) = api
        .post(&format!("/v1/research/{id}/stop"), json!({}))
        .await;
    assert_eq!((status, stopped["status"].as_str()), (200, Some("stopped")));
    let (status, _) = api
        .post(&format!("/v1/research/{id}/stop"), json!({}))
        .await;
    assert_eq!(status, 409);
    let (status, _) = api
        .post(&format!("/v1/research/{id}/start"), json!({}))
        .await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn stopping_a_research_stops_its_run() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let api = Api::start(&server).await;
    let id = api
        .confirmed(repo, "wait [hang]", "cat score", 1_000_000)
        .await;
    let mut events = server.app.bus.subscribe();
    api.post(&format!("/v1/research/{id}/start"), json!({}))
        .await;
    let run = wait_for("the Autoresearcher's run", || {
        server
            .app
            .store
            .runs_of_research(id)
            .unwrap()
            .first()
            .map(|run| run.id)
    });

    let (status, stopped) = api
        .post(&format!("/v1/research/{id}/stop"), json!({}))
        .await;
    assert_eq!(status, 200, "{stopped}");
    assert_eq!(stopped["status"], "stopped");
    let (status, _) = ended(&mut events, id).await;
    assert_eq!(status, "stopped");
    wait_for("the run to end", || {
        server.app.store.run(run).unwrap().unwrap().ended_at
    });
    assert_eq!(
        server.app.store.run(run).unwrap().unwrap().status,
        "stopped"
    );
    // The research stays stopped when its run ends, and no new round starts.
    let (_, research) = api.get(&format!("/v1/research/{id}")).await;
    assert_eq!(research["status"], "stopped");
    assert_eq!(research["baseline"], 100.0);
    assert_eq!(server.app.store.runs_of_research(id).unwrap().len(), 1);
}

#[tokio::test]
async fn a_measure_that_fails_or_prints_no_number_fails_the_research() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let api = Api::start(&server).await;
    let mut events = server.app.bus.subscribe();
    for (measure, why) in [
        ("echo 5; exit 3", "exit status 3"),
        ("echo no number here", "printed no number"),
        ("echo 5 >&2", "printed no number"),
    ] {
        let id = api.confirmed(repo, "goal", measure, 1_000_000).await;
        api.post(&format!("/v1/research/{id}/start"), json!({}))
            .await;
        let (status, _) = ended(&mut events, id).await;
        assert_eq!(status, "failed", "{measure}");
        let (_, research) = api.get(&format!("/v1/research/{id}")).await;
        assert_eq!(research["baseline"], Value::Null);
        let reason = research["reason"].as_str().unwrap();
        assert!(
            reason.contains("the baseline failed") && reason.contains(why),
            "{reason}"
        );
    }
    // No round was run.
    assert!(server.app.store.runs_of_research(1).unwrap().is_empty());
}

#[tokio::test]
async fn a_research_whose_runs_used_the_budget_is_spent_before_the_next_round() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let api = Api::start(&server).await;
    let id = api.confirmed(repo, "goal", "cat score", 1000).await;
    // A round of an earlier start used the tokens.
    let store = &server.app.store;
    let earlier = store
        .add_run(None, "Autoresearcher", "pi-bolt", "research")
        .unwrap();
    store.add_research_run(id, earlier.id).unwrap();
    store
        .add_usage(&Usage {
            run_id: Some(earlier.id),
            model: "m".into(),
            input_tokens: 900,
            output_tokens: 100,
            cached_tokens: 0,
            duration_ms: 1,
            tools: Vec::new(),
        })
        .unwrap();
    let mut events = server.app.bus.subscribe();
    api.post(&format!("/v1/research/{id}/start"), json!({}))
        .await;
    let (status, _) = ended(&mut events, id).await;
    assert_eq!(status, "spent");
    let (_, research) = api.get(&format!("/v1/research/{id}")).await;
    assert_eq!(research["tokens"], 1000);
    assert_eq!(research["baseline"], 100.0);
    assert_eq!(store.runs_of_research(id).unwrap().len(), 1);
}

#[tokio::test]
async fn the_schedule_starts_the_oldest_confirmed_research_inside_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let api = Api::start(&server).await;
    let id = api.confirmed(repo, "goal", "exit 1", 1000).await;
    let runs: Arc<dyn Runs> = server.harness.clone();
    let minutes = server.app.store.minutes_into_the_day().unwrap();
    let at = |minutes: u16| TimeOfDay(minutes % (24 * 60));
    // A window that is over, then one that holds now.
    let outside = (at(minutes + 120), at(minutes + 180));
    research::on_schedule(&server.app, &runs, outside, research::MEASURE_LIMIT).unwrap();
    assert_eq!(
        server.app.store.research(id).unwrap().unwrap().status,
        "confirmed"
    );
    let mut events = server.app.bus.subscribe();
    let inside = (at(minutes + 23 * 60), at(minutes + 60));
    research::on_schedule(&server.app, &runs, inside, research::MEASURE_LIMIT).unwrap();
    let (status, _) = ended(&mut events, id).await;
    assert_eq!(status, "failed");
}

#[tokio::test]
async fn a_research_a_server_left_running_goes_back_to_confirmed_and_keeps_what_it_measured() {
    let dir = tempfile::tempdir().unwrap();
    let (server, repo) = server(dir.path());
    let store = &server.app.store;
    let research = store
        .add_research(repo, "goal", "cat score", "lower", 10)
        .unwrap();
    store
        .move_research(research.id, &["draft"], "confirmed", "")
        .unwrap();
    assert!(store.start_research(research.id).unwrap());
    store.set_baseline(research.id, 100.0).unwrap();
    store.reconfirm_running_research().unwrap();
    let again = store.research(research.id).unwrap().unwrap();
    assert_eq!(
        (again.status.as_str(), again.baseline),
        ("confirmed", Some(100.0))
    );
}
