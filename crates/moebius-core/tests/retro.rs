//! The Retro: a day's errors become a report, and the Retro analyst, a fake Pi here, turns the report into a proposal
//! branch that is pushed to the harness configuration repository's origin and listed by the API, or into proposals
//! for Moebius's own configuration, which a person merges or dismisses from the inbox.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use moebius_core::bus::AppEvent;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use serde_json::{Value, json};

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
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

/// A server with the fake Pi, with the harness configuration repository `harness` if there is one.
fn server(dir: &Path, harness: Option<&Path>) -> Arc<Server> {
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let mut text = format!("[pi_bolt]\nprogram = {:?}\n", fake_pi.display().to_string());
    if let Some(harness) = harness {
        text += &format!("[harness]\nurl = {:?}\n", harness.display().to_string());
    } else {
        text += "[agents.Coder]\nmodel = \"coder-model\"\n";
    }
    std::fs::write(dir.join("moebius.toml"), &text).unwrap();
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    Arc::new(Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap())
}

/// A failed run whose error says `error`, and a refused tool call of another run.
fn a_bad_day(server: &Server, error: &str) {
    let store = &server.app.store;
    let failed = store.add_run(None, "Coder", "pi-bolt", "code").unwrap();
    store
        .append_event(failed.id, "moebius_error", &json!({ "message": error }))
        .unwrap();
    store.set_run_status(failed.id, "failed").unwrap();
    let refused = store.add_run(None, "Coder", "pi-bolt", "code").unwrap();
    let check = json!({ "tool": "bash", "tool_call_id": "c1", "decision": "refuse", "reason": "no-destructive-commands: rm -rf" });
    store
        .append_event(refused.id, "tool_check", &check)
        .unwrap();
    store.set_run_status(refused.id, "succeeded").unwrap();
}

/// A server with a harness configuration repository and a bad day today, whose error says `error(day)`, with the
/// directory, the origin and the day. Should midnight pass while the errors are made, they are not all of one day, so
/// they are made again on a new server.
fn bad_day_server(
    error: impl Fn(&str) -> String,
) -> (tempfile::TempDir, PathBuf, Arc<Server>, String) {
    loop {
        let dir = tempfile::tempdir().unwrap();
        let origin = harness_origin(dir.path());
        let server = server(dir.path(), Some(&origin));
        let day = server.app.store.today().unwrap();
        a_bad_day(&server, &error(&day));
        if server.app.store.today().unwrap() == day {
            return (dir, origin, server, day);
        }
    }
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
}

/// Waits for the `retro_done` event of a Retro that was started after `events` subscribed.
async fn done(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> Value {
    let wait = async {
        loop {
            let event = events.recv().await.unwrap();
            if event.kind == "retro_done" {
                return event.data;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(120), wait)
        .await
        .expect("the Retro did not end")
}

#[tokio::test]
async fn a_day_with_errors_gets_a_report_and_the_analyst_proposes_a_branch_that_is_pushed() {
    let (_dir, origin, server, day) = bad_day_server(|day| {
        format!("stalled [propose proposal/{day}-kinder agents/Coder.md cards=7,9]")
    });
    let branch = format!("proposal/{day}-kinder");
    let api = Api::start(&server).await;
    let mut events = server.app.bus.subscribe();

    let (status, started) = api
        .call(
            reqwest::Method::POST,
            "/v1/retros",
            Some(json!({ "date": day })),
        )
        .await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["date"], day);
    // The errors are in the order they happened, which within a second is not told; so the kinds are compared sorted.
    let mut kinds: Vec<&str> = started["report"]["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    kinds.sort();
    assert_eq!(kinds, ["run_failed", "tool_refused"]);
    let ended = done(&mut events).await;
    assert_eq!(
        ended,
        json!({ "date": day, "status": "proposed", "proposals": 1 })
    );

    let (_, retro) = api
        .call(reqwest::Method::GET, &format!("/v1/retros/{day}"), None)
        .await;
    assert_eq!(retro["status"], "proposed");
    assert_eq!(retro["warnings"], json!([]));
    let run = retro["run_id"].as_i64().unwrap();
    let run = server.app.store.run(run).unwrap().unwrap();
    assert_eq!(
        (run.purpose.as_str(), run.agent.as_str()),
        ("retro", "Retro-analyst")
    );
    assert_eq!(run.card_id, None);
    assert!(!PathBuf::from(run.worktree.unwrap()).exists());

    let (_, proposals) = api.call(reqwest::Method::GET, "/v1/proposals", None).await;
    let proposal = &proposals[0];
    assert_eq!(proposals.as_array().unwrap().len(), 1);
    assert_eq!(proposal["branch"], branch);
    assert_eq!(proposal["title"], "Change agents/Coder.md");
    assert_eq!(proposal["body"], "It addresses the errors.");
    assert_eq!(proposal["agents"], json!(["Coder"]));
    // The `Cards:` line that ends the commit message is the cards, not part of the body.
    assert_eq!(proposal["cards"], json!([7, 9]));
    assert_eq!(proposal["trials"], json!([]));
    assert_eq!(proposal["status"], "open");
    assert_eq!(retro["proposals"], json!([proposal]));

    // The branch is on the origin and changes only the agent's file; the default branch is as it was.
    let changed = git(
        &origin,
        &["diff", "--name-only", &format!("main...{branch}")],
    )
    .unwrap();
    assert_eq!(changed, "agents/Coder.md");

    // The list has no reports.
    let (_, list) = api.call(reqwest::Method::GET, "/v1/retros", None).await;
    assert_eq!(
        list,
        json!([{ "date": day, "status": "proposed", "run_id": retro["run_id"], "errors": 2,
                 "proposals": 1, "at": retro["at"] }])
    );

    // A day has one Retro.
    let (status, _) = api
        .call(
            reqwest::Method::POST,
            "/v1/retros",
            Some(json!({ "date": day })),
        )
        .await;
    assert_eq!(status, 409);
    let (status, _) = api
        .call(
            reqwest::Method::POST,
            "/v1/retros",
            Some(json!({ "date": "yesterday" })),
        )
        .await;
    assert_eq!(status, 422);
    let (status, _) = api
        .call(reqwest::Method::GET, "/v1/retros/2001-01-01", None)
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_branch_that_does_not_change_harness_configuration_is_left_out_with_a_warning() {
    let (_dir, origin, server, day) =
        bad_day_server(|day| format!("[propose proposal/{day}-code src/main.rs]"));
    let branch = format!("proposal/{day}-code");
    let mut events = server.app.bus.subscribe();
    let started = Api::start(&server).await;
    started
        .call(
            reqwest::Method::POST,
            "/v1/retros",
            Some(json!({ "date": day })),
        )
        .await;
    done(&mut events).await;

    let retro = server.app.store.retro(&day).unwrap().unwrap();
    assert_eq!(retro.status, "proposed");
    assert_eq!(retro.warnings.len(), 1, "{:?}", retro.warnings);
    assert!(
        retro.warnings[0].contains("src/main.rs"),
        "{:?}",
        retro.warnings
    );
    assert!(server.app.store.proposals(None).unwrap().is_empty());
    assert!(git(&origin, &["rev-parse", "--verify", "--quiet", &branch]).is_err());
}

#[tokio::test]
async fn a_day_without_errors_makes_no_run() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), Some(&harness_origin(dir.path())));
    let api = Api::start(&server).await;
    let mut events = server.app.bus.subscribe();
    let (status, retro) = api
        .call(reqwest::Method::POST, "/v1/retros", Some(json!({})))
        .await;
    assert_eq!(status, 200);
    assert_eq!(retro["status"], "reported");
    assert_eq!(retro["report"]["errors"], json!([]));
    assert_eq!(retro["run_id"], Value::Null);
    assert_eq!(done(&mut events).await["status"], "reported");
    assert!(server.app.store.active_runs().unwrap().is_empty());
    assert!(server.app.store.chats().unwrap().is_empty());
    assert!(server.app.store.run(1).unwrap().is_none());
}

#[tokio::test]
async fn without_a_harness_repository_only_the_report_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), None);
    a_bad_day(&server, "stalled");
    let api = Api::start(&server).await;
    let (status, retro) = api
        .call(reqwest::Method::POST, "/v1/retros", Some(json!({})))
        .await;
    assert_eq!(status, 200);
    assert_eq!(retro["status"], "reported");
    assert_eq!(retro["run_id"], Value::Null);
    assert_eq!(retro["report"]["errors"].as_array().unwrap().len(), 2);
    assert_eq!(retro["proposals"], json!([]));
    // Only the two runs of the bad day exist.
    assert!(server.app.store.run(3).unwrap().is_none());
}

/// The fake analyst's directive to make the branch `proposal/<day>-<name>` with one commit that writes `file` with
/// `text`.
fn propose_file(day: &str, name: &str, file: &str, text: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    format!("[propose-file proposal/{day}-{name} {file} {encoded}]")
}

/// The rules the fake analyst proposes for the repository `app`.
const RULES: &str = "# Rules\n\n## no-force-push\n\nNever force-push.\n";

/// A server with a bad day whose errors make the fake analyst propose a change to `moebius.toml`, one to the rules of
/// the repository `app`, and one to a key the Retro may not change; with the repository's clone.
fn own_proposals_server() -> (tempfile::TempDir, Arc<Server>, String, PathBuf) {
    let (dir, _origin, server, day) = bad_day_server(|day| {
        [
            propose_file(
                day,
                "rounds",
                "moebius/settings.json",
                r#"{"set": {"board.round_limit": 7, "checks.timeout_seconds": 60}}"#,
            ),
            propose_file(
                day,
                "rules",
                "moebius/repositories/app/.moebius/rules.md",
                RULES,
            ),
            propose_file(
                day,
                "listen",
                "moebius/settings.json",
                r#"{"set": {"listen": "0.0.0.0:80"}}"#,
            ),
        ]
        .join(" ")
    });
    let repository = server
        .app
        .add_repository("app", dir.path().join("app").to_str().unwrap())
        .unwrap();
    (dir, server, day, PathBuf::from(repository.path))
}

/// Runs the Retro of `day` and returns its proposals for Moebius's own configuration by kind, and the Retro.
async fn own_proposals(api: &Api, server: &Server, day: &str) -> (Value, Value, Value) {
    let mut events = server.app.bus.subscribe();
    let (status, _) = api
        .call(
            reqwest::Method::POST,
            "/v1/retros",
            Some(json!({ "date": day })),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(done(&mut events).await["status"], "proposed");
    let (_, retro) = api
        .call(reqwest::Method::GET, &format!("/v1/retros/{day}"), None)
        .await;
    let of = |kind: &str| {
        retro["proposals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["kind"] == kind)
            .cloned()
            .unwrap_or_else(|| panic!("no {kind} proposal in {retro}"))
    };
    (of("settings"), of("repository"), retro)
}

#[tokio::test]
async fn proposals_for_moebius_itself_wait_in_the_inbox_as_diffs_and_merging_applies_them() {
    let (dir, server, day, clone) = own_proposals_server();
    let api = Api::start(&server).await;
    let (settings, repository, retro) = own_proposals(&api, &server, &day).await;

    // The key the Retro may not change is left out with a warning, and no harness proposal was made.
    assert_eq!(retro["proposals"].as_array().unwrap().len(), 2, "{retro}");
    let warnings = retro["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].as_str().unwrap().contains("it changes listen"),
        "{warnings:?}"
    );

    // Each is untried and waits in the inbox as a diff with the Retro's reason; none is tried.
    assert_eq!(settings["status"], "untried");
    assert_eq!(settings["branch"], format!("proposal/{day}-rounds"));
    assert_eq!(settings["body"], "It addresses the errors.");
    assert_eq!(
        settings["edits"],
        json!({ "set": { "board.round_limit": 7, "checks.timeout_seconds": 60 }, "remove": [] })
    );
    let diff = settings["diff"].as_str().unwrap();
    assert!(
        diff.starts_with("--- a/moebius.toml\n+++ b/moebius.toml\n"),
        "{diff}"
    );
    assert!(diff.contains("+round_limit = 7\n"), "{diff}");
    assert_eq!(repository["status"], "untried");
    assert_eq!(repository["repository"], 1);
    assert_eq!(
        repository["edits"],
        json!({ "files": { ".moebius/rules.md": RULES } })
    );
    assert!(
        repository["diff"]
            .as_str()
            .unwrap()
            .starts_with("--- /dev/null\n+++ b/.moebius/rules.md\n@@ -0,0 +1,5 @@\n+# Rules\n"),
        "{repository}"
    );
    let inbox = server.app.store.inbox().unwrap();
    let items: Vec<_> = inbox.iter().filter(|i| i.kind == "proposal").collect();
    assert_eq!(items.len(), 2);
    let item = items.iter().find(|i| i.data["kind"] == "settings").unwrap();
    assert_eq!(item.data["proposal"], settings["id"]);
    assert_eq!(item.data["diff"], settings["diff"]);
    assert_eq!(item.data["reason"], "It addresses the errors.");
    let (status, _) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/proposals/{}/trial", settings["id"]),
            None,
        )
        .await;
    assert_eq!(status, 409);
    // The branches were read and deleted, not pushed.
    let harness = server.harness_repo.as_ref().unwrap().path().to_path_buf();
    let left = git(&harness, &["branch", "--list", "proposal/*"]).unwrap();
    assert_eq!(left, "");

    // Merging the change to moebius.toml saves it: the live key at once, the other after the restart it asks for.
    let (status, merged) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/proposals/{}/merge", settings["id"]),
            Some(json!({ "reason": "Fewer rounds." })),
        )
        .await;
    assert_eq!(status, 200, "{merged}");
    assert_eq!(merged["status"], "merged");
    let file = std::fs::read_to_string(dir.path().join("moebius.toml")).unwrap();
    assert!(file.contains("round_limit = 7"), "{file}");
    assert!(file.contains("timeout_seconds = 60"), "{file}");
    assert_eq!(server.live.get().round_limit, 7);
    tokio::time::timeout(Duration::from_secs(30), server.live.restart_asked())
        .await
        .expect("no restart was asked for");

    // Merging the change to the repository's rules offers it as a change, which merges as any change does.
    let (status, merged) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/proposals/{}/merge", repository["id"]),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200, "{merged}");
    assert_eq!(merged["status"], "merged");
    let change = merged["change"].as_i64().unwrap();
    let branch = format!("moebius/proposal-{}", repository["id"]);
    let offered = git(&clone, &["show", &format!("{branch}:.moebius/rules.md")]).unwrap();
    assert_eq!(format!("{offered}\n"), RULES);
    assert!(git(&clone, &["show", "HEAD:.moebius/rules.md"]).is_err());
    let inbox = server.app.store.inbox().unwrap();
    assert!(inbox.iter().all(|i| i.kind != "proposal"), "{inbox:?}");
    let ready = inbox.iter().find(|i| i.kind == "ready_to_merge").unwrap();
    assert_eq!(ready.data["change"], change);
    let (status, _) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/changes/{change}/merge"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200);
    let rules = git(&clone, &["show", "HEAD:.moebius/rules.md"]).unwrap();
    assert_eq!(format!("{rules}\n"), RULES);
    let decisions = server
        .app
        .store
        .judgements_page(None, None, Some("merge_proposal"), 10)
        .unwrap();
    assert_eq!(decisions.len(), 2);
    assert!(
        decisions
            .iter()
            .all(|d| d.choice == "merge_proposal" && d.decider == "person")
    );
}

#[tokio::test]
async fn dismissing_a_proposal_for_moebius_itself_leaves_everything_as_it_was() {
    let (dir, server, day, clone) = own_proposals_server();
    let api = Api::start(&server).await;
    let file = std::fs::read_to_string(dir.path().join("moebius.toml")).unwrap();
    let head = git(&clone, &["rev-parse", "HEAD"]).unwrap();
    let (settings, repository, _) = own_proposals(&api, &server, &day).await;

    for proposal in [&settings, &repository] {
        let (status, dismissed) = api
            .call(
                reqwest::Method::POST,
                &format!("/v1/proposals/{}/dismiss", proposal["id"]),
                Some(json!({ "reason": "Not now." })),
            )
            .await;
        assert_eq!(status, 200, "{dismissed}");
        assert_eq!(dismissed["status"], "dismissed");
        assert_eq!(dismissed["change"], Value::Null);
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("moebius.toml")).unwrap(),
        file
    );
    assert_eq!(server.live.get().round_limit, 5);
    assert!(!server.live.restart_requested());
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]).unwrap(), head);
    assert_eq!(git(&clone, &["branch", "--list", "moebius/*"]).unwrap(), "");
    assert!(server.app.store.inbox().unwrap().is_empty());
    // A dismissed proposal is decided.
    let (status, _) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/proposals/{}/merge", settings["id"]),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 409);
}
