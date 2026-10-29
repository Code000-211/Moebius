//! Trials: a proposal of the Retro is tried on the card whose error it addresses, with a fake Pi. A proposal that
//! makes the Coder do its work is told apart from one that changes nothing that matters; the positive one reaches the
//! inbox, and a person merges it into the harness configuration repository or dismisses it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use moebius_core::app::PERSON;
use moebius_core::board::Runs;
use moebius_core::bus::AppEvent;
use moebius_core::config::{Config, TimeOfDay};
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use moebius_core::{improve::retro, improve::trial};
use serde_json::{Value, json};

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn commit_all(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]).unwrap();
    git(dir, &["commit", "--quiet", "-m", message]).unwrap();
}

/// A harness configuration repository with the Coder; returns the bare repository the server clones.
fn harness_origin(dir: &Path) -> PathBuf {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&work, "agents/Coder.toml", "model = \"coder-model\"\n");
    write(&work, "agents/Coder.md", "Keep changes small.\n");
    commit_all(&work, "Start");
    let bare = dir.join("harness.git");
    git(
        dir,
        &["init", "--quiet", "--bare", "-b", "main", "harness.git"],
    )
    .unwrap();
    git(&work, &["push", "--quiet", bare.to_str().unwrap(), "main"]).unwrap();
    bare
}

/// A repository whose checks want `done.txt`, which the fake Pi writes when it works.
fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&source, "README.md", "hello\n");
    write(&source, ".moebius/checks", "test -f done.txt\n");
    commit_all(&source, "first");
    source
}

fn server(dir: &Path, harness: &Path) -> Arc<Server> {
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\n",
        fake_pi.display().to_string(),
        harness.display().to_string()
    );
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap())
}

/// The day of the latest Retro, which the proposals belong to.
fn the_retro_day(server: &Server) -> String {
    server.app.store.retros().unwrap()[0].date.clone()
}

/// The clone of the harness configuration repository, where the analyst leaves its branches.
fn clone_of(dir: &Path) -> PathBuf {
    dir.join(".moebius/harness")
}

/// A proposal branch in the clone that changes `file` to `text`, pushed to the origin, and stored as a proposal that
/// addresses `cards`. Returns its id.
fn propose(
    server: &Server,
    dir: &Path,
    name: &str,
    (file, text): (&str, &str),
    cards: &[i64],
) -> i64 {
    let day = the_retro_day(server);
    let branch = format!("proposal/{day}-{name}");
    let clone = clone_of(dir);
    git(&clone, &["checkout", "--quiet", "-b", &branch, "main"]).unwrap();
    write(&clone, file, text);
    commit_all(
        &clone,
        &format!("Change {file} for {name}\n\nIt addresses the errors."),
    );
    git(&clone, &["push", "--quiet", "origin", &branch]).unwrap();
    git(&clone, &["checkout", "--quiet", "main"]).unwrap();
    let agents: Vec<String> = file
        .strip_prefix("agents/")
        .and_then(|f| f.strip_suffix(".md"))
        .map(String::from)
        .into_iter()
        .collect();
    let title = format!("Change {file} for {name}");
    let store = &server.app.store;
    store
        .add_proposal(
            &day,
            &branch,
            &title,
            "It addresses the errors.",
            &agents,
            cards,
        )
        .unwrap();
    store
        .proposals(None)
        .unwrap()
        .into_iter()
        .find(|p| p.branch == branch)
        .unwrap()
        .id
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

/// A card whose first code run changed nothing and so failed, and the Retro of today that reports it. Returns the card's
/// id.
fn a_card_that_failed(server: &Server, dir: &Path) -> i64 {
    let repo = server
        .app
        .add_repository("demo", source(dir).to_str().unwrap())
        .unwrap();
    let card = server
        .app
        .create_issue(repo.id, "Fix it [nothing]", "", PERSON)
        .unwrap();
    let before = server.app.store.today().unwrap();
    let run = server
        .harness
        .start(&server.app, &card, "Coder", "code", None)
        .unwrap();
    wait_until_ended(server, run.id);
    assert_eq!(
        server.app.store.run(run.id).unwrap().unwrap().status,
        "failed"
    );
    // The run failed on the day it started or, had midnight passed meanwhile, on the next.
    let after = server.app.store.today().unwrap();
    let (day, report) = [before, after]
        .into_iter()
        .map(|day| {
            let report = retro::report(&server.app.store, &day).unwrap();
            (day, report)
        })
        .find(|(_, report)| !report.errors.is_empty())
        .unwrap();
    assert_eq!(report.errors[0].kind, "run_failed");
    assert_eq!(report.errors[0].card, Some(card.id));
    let value = serde_json::to_value(&report).unwrap();
    server
        .app
        .store
        .add_retro(&day, &value, "proposed")
        .unwrap();
    card.id
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

    async fn post(&self, id: i64, verb: &str, body: Value) -> (u16, Value) {
        let path = format!("/v1/proposals/{id}/{verb}");
        self.call(reqwest::Method::POST, &path, Some(body)).await
    }

    /// Tries a proposal now and waits for the status it ends with; the `trial_done` events come with it.
    async fn try_now(
        &self,
        server: &Server,
        id: i64,
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    ) -> (String, Vec<Value>) {
        let (status, started) = self
            .call(
                reqwest::Method::POST,
                &format!("/v1/proposals/{id}/trial"),
                None,
            )
            .await;
        assert_eq!(status, 200, "{started}");
        assert_eq!(started["status"], "trying");
        let wait = async {
            let mut done = vec![];
            loop {
                let event = events.recv().await.unwrap();
                if event.kind == "trial_done" {
                    done.push(event.data);
                } else if event.kind == "proposal_status"
                    && event.data["proposal"] == id
                    && event.data["status"] != "trying"
                {
                    return (event.data["status"].as_str().unwrap().to_string(), done);
                }
            }
        };
        let ended = tokio::time::timeout(Duration::from_secs(120), wait)
            .await
            .expect("the trial did not end");
        assert_eq!(
            server.app.store.proposal(id).unwrap().unwrap().status,
            ended.0
        );
        ended
    }
}

fn argv_of(dir: &Path, run: i64) -> Vec<String> {
    let file = dir.join(format!(".moebius/runs/{run}/agent/argv.json"));
    serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
}

fn instructions_of(dir: &Path, run: i64) -> String {
    let argv = argv_of(dir, run);
    let at = argv.iter().position(|a| a == "--append-system-prompt");
    argv[at.unwrap() + 1].clone()
}

#[tokio::test]
async fn a_proposal_that_fixes_the_error_is_told_apart_from_one_that_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let card = a_card_that_failed(&server, dir.path());
    let helps = propose(
        &server,
        dir.path(),
        "helps",
        (
            "agents/Coder.md",
            "Keep changes small.\nAlways write a file. [write anyway]\n",
        ),
        &[card],
    );
    let nothing = propose(
        &server,
        dir.path(),
        "kinder",
        ("agents/Coder.md", "Keep changes small.\nBe kind.\n"),
        &[card],
    );
    let api = Api::start(&server).await;
    let mut events = server.app.bus.subscribe();
    let store = &server.app.store;

    // The proposal that makes the Coder write is positive.
    let (status, done) = api.try_now(&server, helps, &mut events).await;
    assert_eq!(status, "positive");
    assert_eq!(
        done,
        [json!({ "proposal": helps, "card": card, "outcome": "positive" })]
    );
    let (_, proposals) = api.call(reqwest::Method::GET, "/v1/proposals", None).await;
    let positive = proposals
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == helps)
        .unwrap();
    assert_eq!(positive["status"], "positive");
    assert_eq!(positive["cards"], json!([card]));
    let trial = &positive["trials"][0];
    assert_eq!(trial["card_id"], card);
    assert_eq!(trial["outcome"], "positive");
    assert!(
        trial["reason"]
            .as_str()
            .unwrap()
            .contains("the run succeeded, run_failed did not happen again, the checks pass"),
        "{trial}"
    );
    assert_eq!(trial["measurements"]["trial"]["succeeded"], true);
    assert_eq!(trial["measurements"]["trial"]["checks"], true);
    assert_eq!(trial["measurements"]["trial"]["errors"], json!([]));
    assert_eq!(trial["measurements"]["trial"]["rounds"], 1);
    assert_eq!(
        trial["measurements"]["original"]["errors"],
        json!(["run_failed"])
    );
    assert_eq!(trial["measurements"]["original"]["rounds"], 1);

    // The trial run is the Coder's, on the card, with the definition of the proposal's branch.
    let run = store
        .run(trial["run_id"].as_i64().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            run.purpose.as_str(),
            run.agent.as_str(),
            run.card_id,
            run.status.as_str()
        ),
        ("trial", "Coder", Some(card), "succeeded")
    );
    let told = instructions_of(dir.path(), run.id);
    assert!(told.contains("[write anyway]"), "{told}");
    let prompt = std::fs::read_to_string(
        dir.path()
            .join(format!(".moebius/runs/{}/agent/prompt.txt", run.id)),
    )
    .unwrap();
    assert!(
        prompt.starts_with("Work on issue #1: Fix it [nothing]"),
        "{prompt}"
    );
    // The default branch's definition was not touched.
    let coder = server.harness.agents.read().unwrap().agents["Coder"].clone();
    assert_eq!(coder.instructions.as_deref(), Some("Keep changes small."));

    // Only the stored result stays: no branch, no worktree, and the card's own history is as it was.
    let clone = PathBuf::from(&store.repository(1).unwrap().unwrap().path);
    assert_eq!(
        git(&clone, &["branch", "--list", "moebius/trial-*"]).unwrap(),
        ""
    );
    assert!(!PathBuf::from(run.worktree.unwrap()).exists());
    let leftovers = std::fs::read_dir(dir.path().join(".moebius/worktrees/harness"))
        .unwrap()
        .count();
    assert_eq!(leftovers, 0);
    assert_eq!(store.runs_for_card(card).unwrap().len(), 1);
    assert_eq!(store.tokens_for_card(card).unwrap(), 0);

    // It is in the inbox, with the trials' reasons, and the events said so.
    let inbox = store.inbox().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].kind, "proposal");
    assert_eq!(inbox[0].title, positive["title"]);
    assert_eq!(inbox[0].data["proposal"], helps);
    assert_eq!(inbox[0].data["trials"][0]["outcome"], "positive");
    assert_eq!(inbox[0].data["trials"][0]["reason"], trial["reason"]);

    // The one that changes nothing that matters is negative: the run changed nothing again.
    let (status, done) = api.try_now(&server, nothing, &mut events).await;
    assert_eq!(status, "negative");
    assert_eq!(
        done,
        [json!({ "proposal": nothing, "card": card, "outcome": "negative" })]
    );
    let negative = store.proposal(nothing).unwrap().unwrap();
    assert_eq!(negative.trials[0].outcome, "negative");
    assert_eq!(
        negative.trials[0].reason,
        "the run did not succeed; it still has run_failed"
    );
    assert_eq!(
        negative.trials[0].measurements["trial"]["checks"],
        Value::Null
    );
    let run = store
        .run(negative.trials[0].run_id.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(run.status, "failed");
    assert!(instructions_of(dir.path(), run.id).contains("Be kind."));
    // A negative proposal is not offered to anyone.
    assert_eq!(store.inbox().unwrap().len(), 1);
    assert_eq!(
        git(&clone, &["branch", "--list", "moebius/trial-*"]).unwrap(),
        ""
    );

    // Neither can be tried again, nor while another is being tried.
    for id in [helps, nothing] {
        let (status, _) = api
            .call(
                reqwest::Method::POST,
                &format!("/v1/proposals/{id}/trial"),
                None,
            )
            .await;
        assert_eq!(status, 409);
    }
    let (status, _) = api
        .call(reqwest::Method::POST, "/v1/proposals/99/trial", None)
        .await;
    assert_eq!(status, 404);

    // A person merges the positive one: a merge commit on the origin's default branch, the agents read again.
    let (status, merged) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/proposals/{helps}/merge"),
            Some(json!({ "reason": "it works" })),
        )
        .await;
    assert_eq!(status, 200, "{merged}");
    assert_eq!(merged["status"], "merged");
    let text = git(&origin, &["show", "main:agents/Coder.md"]).unwrap();
    assert!(text.contains("[write anyway]"), "{text}");
    let parents = git(&origin, &["rev-list", "--parents", "-n", "1", "main"]).unwrap();
    assert_eq!(parents.split(' ').count(), 3, "a merge commit: {parents}");
    let coder = server.harness.agents.read().unwrap().agents["Coder"].clone();
    assert!(coder.instructions.unwrap().contains("[write anyway]"));
    assert!(store.inbox().unwrap().is_empty());
    let judgements = store.judgements("merge_proposal").unwrap();
    assert_eq!(judgements.len(), 1);
    assert_eq!(
        (
            judgements[0].choice.as_str(),
            judgements[0].reason.as_str(),
            judgements[0].decider.as_str()
        ),
        ("merge_proposal", "it works", "person")
    );
    // Merged is final, and a negative proposal cannot be merged or dismissed.
    for (id, verb) in [(helps, "merge"), (nothing, "merge"), (nothing, "dismiss")] {
        assert_eq!(api.post(id, verb, json!({})).await.0, 409);
    }
}

#[tokio::test]
async fn a_proposal_without_cards_is_skipped_and_offered_as_it_is_and_a_conflict_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let card = a_card_that_failed(&server, dir.path());
    let (merged, conflicting, dismissed) = (
        propose(
            &server,
            dir.path(),
            "a",
            ("agents/Coder.md", "Keep changes small.\nA.\n"),
            &[],
        ),
        propose(
            &server,
            dir.path(),
            "b",
            ("agents/Coder.md", "Keep changes small.\nB.\n"),
            &[],
        ),
        propose(
            &server,
            dir.path(),
            "c",
            ("rules.md", "## c\nBe plain.\n"),
            &[card + 1],
        ),
    );
    let api = Api::start(&server).await;
    let mut events = server.app.bus.subscribe();
    let store = &server.app.store;
    for id in [merged, conflicting, dismissed] {
        assert_eq!(api.try_now(&server, id, &mut events).await.0, "untried");
    }
    let trial = &store.proposal(merged).unwrap().unwrap().trials[0];
    assert_eq!(
        (trial.outcome.as_str(), trial.card_id, trial.run_id),
        ("skipped", None, None)
    );
    assert!(trial.reason.contains("names no cards"), "{}", trial.reason);
    // A card that does not exist is skipped too.
    let trial = &store.proposal(dismissed).unwrap().unwrap().trials[0];
    assert_eq!(
        (trial.outcome.as_str(), trial.card_id),
        ("skipped", Some(card + 1))
    );
    assert_eq!(trial.reason, format!("card {} does not exist", card + 1));
    // No run was started for them: only the card's first one exists.
    assert!(store.run(2).unwrap().is_none());
    assert_eq!(store.inbox().unwrap().len(), 3);

    let (status, done) = api.post(merged, "merge", json!({})).await;
    assert_eq!(status, 200, "{done}");

    // The second one changes the same lines: it does not merge, and nothing changes.
    let before = git(&origin, &["rev-parse", "main"]).unwrap();
    let (status, error) = api.post(conflicting, "merge", json!({})).await;
    assert_eq!(status, 409, "{error}");
    assert_eq!(git(&origin, &["rev-parse", "main"]).unwrap(), before);
    assert_eq!(
        git(&clone_of(dir.path()), &["status", "--porcelain"]).unwrap(),
        ""
    );
    assert_eq!(
        store.proposal(conflicting).unwrap().unwrap().status,
        "untried"
    );
    assert_eq!(store.inbox().unwrap().len(), 2);
    assert!(store.judgements("merge_proposal").unwrap().len() == 1);

    // A person dismisses the third.
    let (status, done) = api
        .post(dismissed, "dismiss", json!({ "reason": "not now" }))
        .await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["status"], "dismissed");
    assert_eq!(store.inbox().unwrap().len(), 1);
    let judgements = store.judgements("merge_proposal").unwrap();
    assert_eq!(
        (judgements[1].choice.as_str(), judgements[1].reason.as_str()),
        ("dismiss", "not now")
    );
    assert_eq!(git(&origin, &["rev-parse", "main"]).unwrap(), before);

    // The Retro of the day shows its proposals with their trials.
    let day = the_retro_day(&server);
    let (_, retro) = api
        .call(reqwest::Method::GET, &format!("/v1/retros/{day}"), None)
        .await;
    let statuses: Vec<&str> = retro["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["dismissed", "untried", "merged"]);
    assert_eq!(retro["proposals"][0]["trials"][0]["outcome"], "skipped");
}

#[tokio::test]
async fn trials_start_in_the_window_only_one_at_a_time_and_a_server_start_reopens_what_was_going() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let card = a_card_that_failed(&server, dir.path());
    let first = propose(
        &server,
        dir.path(),
        "a",
        ("rules.md", "## a\nBe plain.\n"),
        &[],
    );
    let second = propose(
        &server,
        dir.path(),
        "b",
        ("rules.md", "## b\nBe plain.\n"),
        &[],
    );
    let store = &server.app.store;
    let runs: Arc<dyn Runs> = server.harness.clone();
    let repo = server.harness_repo.as_ref();
    let now = store.minutes_into_the_day().unwrap();
    let around =
        |from: u16, to: u16| (TimeOfDay((now + from) % 1440), TimeOfDay((now + to) % 1440));
    let open = |id: i64| store.proposal(id).unwrap().unwrap().status == "open";

    // Outside the window, and with no run to spare, nothing starts.
    trial::on_schedule(&server.app, &runs, repo, around(120, 180), 3).unwrap();
    trial::on_schedule(&server.app, &runs, repo, around(1380, 60), 0).unwrap();
    assert!(open(first) && open(second));
    assert!(trial::in_window(TimeOfDay(now), around(1380, 60)));
    assert!(!trial::in_window(TimeOfDay(now), around(120, 180)));
    assert!(trial::in_window(
        TimeOfDay(3 * 60),
        server.config.trial_window
    ));

    // Inside it, the oldest open proposal is tried.
    let mut events = server.app.bus.subscribe();
    trial::on_schedule(&server.app, &runs, repo, around(1380, 60), 3).unwrap();
    let wait = async {
        loop {
            let event = events.recv().await.unwrap();
            if event.kind == "proposal_status" && event.data["status"] == "untried" {
                return event.data;
            }
        }
    };
    let ended = tokio::time::timeout(Duration::from_secs(60), wait)
        .await
        .unwrap();
    assert_eq!(ended["proposal"], first);
    assert!(open(second));

    // While one is going, no other starts.
    assert!(store.start_trying(second).unwrap());
    trial::on_schedule(&server.app, &runs, repo, around(1380, 60), 3).unwrap();
    assert_eq!(store.proposal(first).unwrap().unwrap().status, "untried");
    let (status, _) = Api::start(&server)
        .await
        .call(
            reqwest::Method::POST,
            &format!("/v1/proposals/{first}/trial"),
            None,
        )
        .await;
    assert_eq!(status, 409);

    // A server that starts again finds the proposal left trying open again, without the trials it had made.
    store
        .add_trial(second, Some(card), None, "skipped", "left over", &json!({}))
        .unwrap();
    let again = self::server(dir.path(), &origin);
    let reopened = again.app.store.proposal(second).unwrap().unwrap();
    assert_eq!(reopened.status, "open");
    assert!(reopened.trials.is_empty());
    assert_eq!(
        again.app.store.proposal(first).unwrap().unwrap().status,
        "untried"
    );
}
