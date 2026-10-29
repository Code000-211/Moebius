//! The harness configuration repository: agents and rules come from a git repository, a run uses them, an edit through
//! the API is a commit to it, and a commit someone else pushes arrives at the next refresh.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use moebius_core::app::PERSON;
use moebius_core::board::Runs;
use moebius_core::config::Config;
use moebius_core::decide::rules;
use moebius_core::repos::git;
use moebius_core::router::Protocol;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use serde_json::{Value, json};

const CODER: &str = "model = \"coder-model\"\nthinking = \"low\"\nskills = [\"house-style\"]\nextensions = [\"extensions/log.ts\"]\nmcp = [\"docs\"]\n";

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn commit_all(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]).unwrap();
    git(dir, &["commit", "--quiet", "-m", message]).unwrap();
}

/// A harness configuration repository with two agents, a skill, an extension, an MCP server and rules, and two broken
/// agents. Returns the bare repository the server clones.
fn harness_origin(dir: &Path) -> PathBuf {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&work, "agents/Coder.toml", CODER);
    write(&work, "agents/Coder.md", "Keep changes small.\n");
    write(
        &work,
        "agents/Reviewer.toml",
        "model = \"reviewer-model\"\nprotocol = \"anthropic\"\ntoken_budget = 500\n",
    );
    write(&work, "agents/Typo.toml", "modle = \"x\"\n");
    write(
        &work,
        "agents/NoSkill.toml",
        "model = \"x\"\nskills = [\"missing\"]\n",
    );
    write(
        &work,
        "skills/house-style/SKILL.md",
        "---\nname: house-style\ndescription: \"Write plainly.\"\n---\nBody.\n",
    );
    write(&work, "skills/house-style/notes/extra.md", "More.\n");
    write(&work, "extensions/log.ts", "export default () => {};\n");
    write(
        &work,
        "agents/Token.toml",
        "model = \"x\"\nmcp = [\"token\"]\n",
    );
    write(
        &work,
        "agents/Secret.toml",
        "model = \"x\"\nmcp = [\"leak\"]\n",
    );
    write(
        &work,
        "mcp.toml",
        "# The servers.\n[docs]\ncommand = \"docs-server\"\n\
         [token]\ncommand = \"x\"\nenv = { TOKEN = \"${DOCS_TOKEN}\" }\n\
         [leak]\ncommand = \"x\"\nenv = { KEY = \"${OPENAI_API_KEY}\" }\n",
    );
    write(&work, "rules.md", "## no-vendoring\nDo not vendor.\n");
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

/// A source repository for the cards of runs.
fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&source, "README.md", "hello\n");
    commit_all(&source, "first");
    source
}

fn server(dir: &Path, harness: &Path) -> Server {
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\nmcp_env = [\"DOCS_TOKEN\"]\n",
        fake_pi.display().to_string(),
        harness.display().to_string()
    );
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap()
}

fn agent_model(server: &Server, name: &str) -> Option<String> {
    let set = server.harness.agents.read().unwrap();
    set.agents.get(name).map(|agent| agent.model.clone())
}

#[test]
fn the_agents_and_rules_are_what_the_repository_says_and_a_broken_agent_is_left_out_alone() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), &harness_origin(dir.path()));
    let clone = dir.path().join(".moebius/harness");

    let set = server.harness.agents.read().unwrap().clone();
    assert_eq!(
        set.agents.keys().collect::<Vec<_>>(),
        ["Coder", "Reviewer", "Token"]
    );
    let coder = &set.agents["Coder"];
    assert_eq!(coder.model, "coder-model");
    assert_eq!(coder.protocol, Protocol::OpenAi);
    assert_eq!(coder.thinking.as_deref(), Some("low"));
    assert_eq!(coder.instructions.as_deref(), Some("Keep changes small."));
    assert_eq!(coder.skills, [clone.join("skills/house-style")]);
    assert_eq!(coder.extensions, [clone.join("extensions/log.ts")]);
    let reviewer = &set.agents["Reviewer"];
    assert_eq!(reviewer.protocol, Protocol::Anthropic);
    assert_eq!(reviewer.token_budget, Some(500));
    assert_eq!(reviewer.instructions, None);
    assert_eq!(server.harness.token_budget("Reviewer"), Some(500));

    // The agents that cannot be used are left out with what is wrong, and listed so.
    assert_eq!(
        set.errors.keys().collect::<Vec<_>>(),
        ["NoSkill", "Secret", "Typo"]
    );
    // A server that takes a variable `mcp_env` does not list leaves out the agents that name it.
    assert!(set.errors["Secret"].contains("${OPENAI_API_KEY} is not in mcp_env"));
    assert_eq!(coder.mcp.keys().collect::<Vec<_>>(), ["docs"]);
    assert!(set.errors["NoSkill"].contains("\"missing\" is not in skills/"));
    assert!(set.errors["Typo"].contains("modle"));
    let listed = server.harness.agents();
    let broken: Vec<_> = listed
        .iter()
        .filter(|agent| agent.error.is_some())
        .map(|agent| (agent.name.as_str(), agent.model.as_str()))
        .collect();
    assert_eq!(broken, [("NoSkill", ""), ("Secret", ""), ("Typo", "")]);
    assert!(!server.harness.has_agent("Typo"));

    let rules = rules::applicable(server.config.rules_path().as_deref(), None).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "no-vendoring");
}

#[test]
fn an_agent_defined_only_in_the_repository_runs_with_its_model_and_instructions() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), &harness_origin(dir.path()));
    let repo = server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let card = server
        .app
        .create_issue(repo.id, "Write done.txt", "", PERSON)
        .unwrap();
    let run = server
        .harness
        .start(&server.app, &card, "Coder", "code", None)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while server
        .app
        .store
        .run(run.id)
        .unwrap()
        .unwrap()
        .ended_at
        .is_none()
    {
        assert!(Instant::now() < deadline, "the run did not end");
        std::thread::sleep(Duration::from_millis(20));
    }

    let argv = dir
        .path()
        .join(format!(".moebius/runs/{}/agent/argv.json", run.id));
    let argv: Vec<String> = serde_json::from_slice(&std::fs::read(argv).unwrap()).unwrap();
    assert!(argv.windows(2).any(|w| w == ["--model", "coder-model"]));
    assert!(argv.windows(2).any(|w| w == ["--thinking", "low"]));
    assert!(
        argv.windows(2)
            .any(|w| w == ["--append-system-prompt", "Keep changes small."])
    );
    let clone = dir.path().join(".moebius/harness");
    let skill = clone.join("skills/house-style");
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--skill" && w[1] == skill.to_str().unwrap())
    );

    // An agent that is left out is refused with its error.
    let error = server
        .harness
        .start(&server.app, &card, "Typo", "code", None)
        .unwrap_err();
    assert!(
        error.contains("cannot be used") && error.contains("modle"),
        "{error}"
    );
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

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> (u16, String) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        (response.status().as_u16(), response.text().await.unwrap())
    }
}

fn head(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"]).unwrap()
}

#[tokio::test]
async fn the_skills_and_mcp_servers_an_agent_can_name_are_listed_without_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), &harness_origin(dir.path()));
    let api = Api::start(&server).await;

    let (status, skills) = api.call(reqwest::Method::GET, "/v1/skills", None).await;
    assert_eq!(status, 200);
    let skills: Value = serde_json::from_str(&skills).unwrap();
    assert_eq!(skills[0]["name"], "moebius");
    assert_eq!(skills[0]["built_in"], true);
    assert!(
        skills[0]["description"]
            .as_str()
            .unwrap()
            .contains("Moebius")
    );
    // The task skills of the Moebius agent are built in too, and come before the repository's.
    let names: Vec<&str> = skills
        .as_array()
        .unwrap()
        .iter()
        .map(|skill| skill["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "moebius",
            "moebius-operate",
            "moebius-configure",
            "moebius-workflows",
            "moebius-extensions",
            "moebius-plugins",
            "house-style"
        ]
    );
    for skill in &skills.as_array().unwrap()[..6] {
        assert_eq!(skill["built_in"], true);
        assert!(skill["description"].as_str().unwrap().contains("Use it"));
    }
    assert_eq!(
        skills[6],
        json!({"name": "house-style", "description": "Write plainly.", "built_in": false})
    );

    let (status, servers) = api
        .call(reqwest::Method::GET, "/v1/mcp-servers", None)
        .await;
    assert_eq!(status, 200);
    assert!(!servers.contains("DOCS_TOKEN") && !servers.contains("docs-server"));
    let servers: Value = serde_json::from_str(&servers).unwrap();
    let by_name = |name: &str| {
        servers
            .as_array()
            .unwrap()
            .iter()
            .find(|server| server["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(by_name("docs"), json!({"name": "docs", "kind": "stdio"}));
    assert_eq!(by_name("token")["kind"], "stdio");
    assert!(
        by_name("leak")["error"]
            .as_str()
            .unwrap()
            .contains("OPENAI_API_KEY")
    );
}

#[tokio::test]
async fn a_built_in_agent_is_read_with_its_built_in_part_and_its_own_files() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;

    // Without files, only what Moebius brings.
    let (status, body) = api
        .call(reqwest::Method::GET, "/v1/agents/Moebius", None)
        .await;
    assert_eq!(status, 200, "{body}");
    let read: Value = serde_json::from_str(&body).unwrap();
    assert!(read.get("settings").is_none(), "{read}");
    assert_eq!(read["built_in"]["skills"].as_array().unwrap().len(), 6);

    // With files, both; what the person saves is only the files.
    let edit = json!({"settings": {"model": "own-model"}, "instructions": "Prefer small scripts."});
    let (status, body) = api
        .call(reqwest::Method::PUT, "/v1/agents/Moebius", Some(edit))
        .await;
    assert_eq!(status, 200, "{body}");
    let saved: Value = serde_json::from_str(&body).unwrap();
    assert!(saved.get("built_in").is_none(), "{saved}");
    let (_, body) = api
        .call(reqwest::Method::GET, "/v1/agents/Moebius", None)
        .await;
    let read: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(read["settings"]["model"], "own-model");
    assert_eq!(
        read["instructions"].as_str().unwrap().trim(),
        "Prefer small scripts."
    );
    assert!(
        read["built_in"]["instructions"]
            .as_str()
            .unwrap()
            .contains("moebius-configure")
    );

    // An agent of the person's has no built-in part.
    let (_, body) = api
        .call(reqwest::Method::GET, "/v1/agents/Coder", None)
        .await;
    let read: Value = serde_json::from_str(&body).unwrap();
    assert!(read.get("built_in").is_none(), "{read}");
}

#[tokio::test]
async fn an_edit_through_the_api_is_a_commit_that_is_pushed_and_a_failed_push_undoes_it() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");
    let start = head(&clone);

    // An agent is read with its instructions.
    let (status, body) = api
        .call(reqwest::Method::GET, "/v1/agents/Coder", None)
        .await;
    assert_eq!(status, 200, "{body}");
    let read: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(read["name"], "Coder");
    assert_eq!(read["settings"]["model"], "coder-model");
    assert_eq!(read["settings"]["mcp"], json!(["docs"]));
    assert_eq!(read["instructions"], "Keep changes small.\n");
    assert_eq!(
        api.call(reqwest::Method::GET, "/v1/agents/Nobody", None)
            .await
            .0,
        404
    );

    // A name that is no agent name reads no file.
    let (status, _) = api
        .call(reqwest::Method::GET, "/v1/agents/..%2Frules", None)
        .await;
    assert_eq!(status, 404);

    // A change is a commit on the clone's default branch and on the origin's, and the agent is used from then on.
    let edit = json!({
        "settings": { "model": "better-model", "thinking": "high", "skills": ["house-style"] },
        "instructions": "Be careful.\n",
    });
    let (status, body) = api
        .call(reqwest::Method::PUT, "/v1/agents/Coder", Some(edit))
        .await;
    assert_eq!(status, 200, "{body}");
    let saved: Value = serde_json::from_str(&body).unwrap();
    let short = git(&clone, &["rev-parse", "--short", "HEAD"]).unwrap();
    assert_eq!(saved["commit"], short.as_str());
    for repository in [&clone, &origin] {
        assert_eq!(
            git(repository, &["log", "-1", "--format=%s", "main"]).unwrap(),
            "Update the agent Coder"
        );
        assert_eq!(
            git(repository, &["log", "-1", "--format=%an", "main"]).unwrap(),
            "Moebius"
        );
    }
    assert_eq!(head(&clone), git(&origin, &["rev-parse", "main"]).unwrap());
    assert_eq!(
        git(&clone, &["show", "HEAD:agents/Coder.md"]).unwrap(),
        "Be careful."
    );
    assert_eq!(
        agent_model(&server, "Coder").as_deref(),
        Some("better-model")
    );
    let set = server.harness.agents.read().unwrap().clone();
    assert_eq!(
        set.agents["Coder"].instructions.as_deref(),
        Some("Be careful.")
    );
    assert!(set.agents["Coder"].extensions.is_empty());
    let judgements = server.app.store.judgements("edit_agent").unwrap();
    assert_eq!(judgements.len(), 1);
    assert_eq!(
        (
            judgements[0].decider.as_str(),
            judgements[0].choice.as_str()
        ),
        ("person", "edit_agent")
    );

    // An agent that is new is added, and an edit that changes nothing commits nothing.
    let new = json!({ "settings": { "model": "other" } });
    let (status, body) = api
        .call(reqwest::Method::PUT, "/v1/agents/Fresh", Some(new.clone()))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Add the agent Fresh"
    );
    let added = head(&clone);
    assert_eq!(
        api.call(reqwest::Method::PUT, "/v1/agents/Fresh", Some(new))
            .await
            .0,
        200
    );
    assert_eq!(head(&clone), added);
    assert_eq!(server.app.store.judgements("edit_agent").unwrap().len(), 2);

    // An invalid edit is refused and commits nothing.
    for (name, edit) in [
        (
            "Coder",
            json!({ "settings": { "model": "m", "skills": ["missing"] } }),
        ),
        (
            "Coder",
            json!({ "settings": { "model": "m", "mcp": ["nothing"] } }),
        ),
        (
            "Coder",
            json!({ "settings": { "model": "m", "extensions": ["../outside.ts"] } }),
        ),
        ("Coder", json!({ "settings": { "model": " " } })),
        (
            "Coder",
            json!({ "settings": { "model": "m", "color": "red" } }),
        ),
        ("bad name", json!({ "settings": { "model": "m" } })),
    ] {
        let (status, body) = api
            .call(
                reqwest::Method::PUT,
                &format!("/v1/agents/{}", name.replace(' ', "%20")),
                Some(edit),
            )
            .await;
        assert_eq!(status, 422, "{name}: {body}");
    }
    assert_eq!(head(&clone), added);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    assert_eq!(server.app.store.judgements("edit_agent").unwrap().len(), 2);

    // A push that fails undoes the commit, so the clone does not drift from the origin.
    let hook = origin.join("hooks/pre-receive");
    std::fs::write(&hook, "#!/bin/sh\necho 'closed for edits' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let edit = json!({ "settings": { "model": "never" } });
    let (status, body) = api
        .call(reqwest::Method::PUT, "/v1/agents/Coder", Some(edit))
        .await;
    assert_eq!(status, 500);
    assert!(body.contains("closed for edits"), "{body}");
    assert_eq!(head(&clone), added);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    assert_eq!(
        agent_model(&server, "Coder").as_deref(),
        Some("better-model")
    );
    let (status, _) = api
        .call(reqwest::Method::DELETE, "/v1/agents/Fresh", None)
        .await;
    assert_eq!(status, 500);
    assert_eq!(head(&clone), added);
    assert!(clone.join("agents/Fresh.toml").is_file());
    // Neither edit happened, so neither is a judgement.
    assert_eq!(server.app.store.judgements("edit_agent").unwrap().len(), 2);
    std::fs::remove_file(&hook).unwrap();

    // An agent is deleted the same way.
    let (status, body) = api
        .call(reqwest::Method::DELETE, "/v1/agents/Fresh", None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Delete the agent Fresh"
    );
    assert!(!server.harness.has_agent("Fresh"));
    assert_eq!(
        api.call(reqwest::Method::DELETE, "/v1/agents/Fresh", None)
            .await
            .0,
        404
    );
    assert_ne!(head(&clone), start);
}

#[test]
fn a_commit_someone_else_pushes_arrives_at_the_next_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let mut events = server.app.bus.subscribe();
    assert_eq!(
        agent_model(&server, "Reviewer").as_deref(),
        Some("reviewer-model")
    );

    let other = dir.path().join("other");
    git(
        dir.path(),
        &["clone", "--quiet", origin.to_str().unwrap(), "other"],
    )
    .unwrap();
    write(
        &other,
        "agents/Reviewer.toml",
        "model = \"stricter-model\"\n",
    );
    write(&other, "agents/Planner.toml", "model = \"planner-model\"\n");
    write(&other, "agents/Typo.toml", "model = \"fixed\"\n");
    write(&other, "agents/Odd.toml", "model = 3\n");
    commit_all(&other, "Change the agents");
    git(&other, &["push", "--quiet", "origin", "main"]).unwrap();

    assert_eq!(
        agent_model(&server, "Reviewer").as_deref(),
        Some("reviewer-model")
    );
    server.refresh_harness_repository();
    assert_eq!(
        agent_model(&server, "Reviewer").as_deref(),
        Some("stricter-model")
    );
    assert_eq!(
        agent_model(&server, "Planner").as_deref(),
        Some("planner-model")
    );
    assert_eq!(agent_model(&server, "Typo").as_deref(), Some("fixed"));
    let set = server.harness.agents.read().unwrap().clone();
    assert_eq!(
        set.errors.keys().collect::<Vec<_>>(),
        ["NoSkill", "Odd", "Secret"]
    );
    let mut reported = Vec::new();
    while let Ok(event) = events.try_recv() {
        if event.kind == "harness_error" {
            reported.push(event.data["agent"].as_str().unwrap().to_string());
        }
    }
    assert_eq!(reported, ["NoSkill", "Odd", "Secret"]);

    // Nothing new: nothing is read again.
    server.refresh_harness_repository();
    assert!(events.try_recv().is_err());
}

/// Starts a chat in the harness configuration repository that writes what `message` says, and returns its id.
async fn harness_chat(api: &Api, server: &Server, message: &str) -> i64 {
    let body = json!({ "message": message, "harness": true });
    let (status, run) = api
        .call(reqwest::Method::POST, "/v1/chats", Some(body))
        .await;
    assert_eq!(status, 200, "{run}");
    let run: Value = serde_json::from_str(&run).unwrap();
    assert_eq!(run["harness_repository"], true);
    let id = run["id"].as_i64().unwrap();
    wait_for(server, id, |run| run.status == "waiting").await;
    id
}

/// Ends the chat and waits until it has ended.
async fn end_chat(api: &Api, server: &Server, id: i64) {
    let (status, body) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/chats/{id}/end"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let run = wait_for(server, id, |run| run.ended_at.is_some()).await;
    assert_eq!(run.status, "succeeded");
}

async fn wait_for(
    server: &Server,
    id: i64,
    done: impl Fn(&moebius_core::store::Run) -> bool,
) -> moebius_core::store::Run {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let run = server.app.store.run(id).unwrap().unwrap();
        if done(&run) {
            return run;
        }
        assert!(
            Instant::now() < deadline,
            "the chat did not get there: {run:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn error_messages(server: &Server, id: i64) -> Vec<String> {
    server
        .app
        .store
        .events(id)
        .unwrap()
        .iter()
        .filter(|event| event.kind == "moebius_error")
        .map(|event| event.data["message"].as_str().unwrap().to_string())
        .collect()
}

const TESTING_SKILL: &str =
    "---\nname: testing\ndescription: \"Test first.\"\n---\nWrite the test first.\n";

/// A message that makes the fake Pi write a new skill and give it to the Coder.
fn skill_message() -> String {
    format!(
        "Add a skill.\n[put skills/testing/SKILL.md]\n{TESTING_SKILL}[/put]\n\
         [put agents/Coder.toml]\nmodel = \"coder-model\"\nskills = [\"house-style\", \"testing\"]\n\
         extensions = [\"extensions/log.ts\"]\nmcp = [\"docs\"]\n[/put]"
    )
}

#[tokio::test]
async fn a_chat_in_the_harness_configuration_offers_a_change_whose_merge_lands_on_origin_and_reaches_the_coder()
 {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");
    let start = head(&clone);

    let chat = harness_chat(&api, &server, &skill_message()).await;
    // The chat works on a branch of its own in a worktree of the clone; the default branch has not moved.
    let run = server.app.store.run(chat).unwrap().unwrap();
    assert_eq!((run.repository_id, run.harness_repository), (None, true));
    let worktree = PathBuf::from(run.worktree.unwrap());
    assert!(worktree.join("skills/testing/SKILL.md").is_file());
    assert_eq!(head(&clone), start);
    end_chat(&api, &server, chat).await;

    // The work is offered as a change of the harness configuration repository, which has no card.
    let changes = server.app.store.harness_changes().unwrap();
    assert_eq!(changes.len(), 1);
    let change = &changes[0];
    let branch = format!("moebius/chat-{chat}");
    assert_eq!(
        (
            change.repository_id,
            change.harness_repository,
            change.card_id,
            change.branch.as_str(),
            change.state.as_str()
        ),
        (None, true, None, branch.as_str(), "open")
    );
    let inbox = server.app.store.inbox().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].kind, "ready_to_merge");
    assert_eq!(inbox[0].data["change"], change.id);
    assert!(error_messages(&server, chat).is_empty());
    let (_, snapshot) = api.call(reqwest::Method::GET, "/v1/snapshot", None).await;
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["harness_changes"][0]["id"], change.id);
    let (_, chats) = api.call(reqwest::Method::GET, "/v1/chats", None).await;
    let chats: Value = serde_json::from_str(&chats).unwrap();
    assert_eq!(chats[0]["harness_repository"], true);
    // The Coder does not have the skill yet.
    let skills = server.harness.agents.read().unwrap().agents["Coder"]
        .skills
        .len();
    assert_eq!(skills, 1);

    // Merging lands a merge commit on origin and reads the agents again.
    let (status, merged) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/changes/{}/merge", change.id),
            Some(json!({ "reason": "good skill" })),
        )
        .await;
    assert_eq!(status, 200, "{merged}");
    let merged: Value = serde_json::from_str(&merged).unwrap();
    let commit = merged["commit"].as_str().unwrap();
    assert_eq!(git(&origin, &["rev-parse", "main"]).unwrap(), commit);
    assert_eq!(head(&clone), commit);
    let parents = git(&origin, &["rev-list", "--parents", "-n1", "main"]).unwrap();
    assert_eq!(parents.split_whitespace().count(), 3, "{parents}");
    assert!(git(&origin, &["show", "main:skills/testing/SKILL.md"]).is_ok());
    let stored = server.app.store.change(change.id).unwrap().unwrap();
    assert_eq!(stored.state, "merged");
    assert!(server.app.store.inbox().unwrap().is_empty());
    let skills = server.harness.agents.read().unwrap().agents["Coder"]
        .skills
        .clone();
    assert_eq!(
        skills,
        [
            clone.join("skills/house-style"),
            clone.join("skills/testing")
        ]
    );
    let judgements = server.app.store.judgements("merge").unwrap();
    assert_eq!(judgements.len(), 1);
    assert_eq!(judgements[0].decider, "person");
    assert_eq!(judgements[0].reason, "good skill");

    // The Coder's next run gets the skill.
    let repo = server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let card = server
        .app
        .create_issue(repo.id, "Write done.txt", "", PERSON)
        .unwrap();
    let run = server
        .harness
        .start(&server.app, &card, "Coder", "code", None)
        .unwrap();
    wait_for(&server, run.id, |run| run.ended_at.is_some()).await;
    let argv = dir
        .path()
        .join(format!(".moebius/runs/{}/agent/argv.json", run.id));
    let argv: Vec<String> = serde_json::from_slice(&std::fs::read(argv).unwrap()).unwrap();
    let skill = clone.join("skills/testing");
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--skill" && w[1] == skill.to_str().unwrap()),
        "{argv:?}"
    );

    // Continuing the ended chat works in the harness configuration again; nothing new is offered when nothing changed.
    let (status, run) = api
        .call(
            reqwest::Method::POST,
            "/v1/chats",
            Some(json!({ "message": "Thanks.", "continues": chat })),
        )
        .await;
    assert_eq!(status, 200, "{run}");
    let run: Value = serde_json::from_str(&run).unwrap();
    assert_eq!(run["harness_repository"], true);
    let continued = run["id"].as_i64().unwrap();
    wait_for(&server, continued, |run| run.status == "waiting").await;
    end_chat(&api, &server, continued).await;
    assert_eq!(server.app.store.harness_changes().unwrap().len(), 1);

    // Naming a repository takes the chat out of the harness configuration.
    let named =
        json!({ "message": "In the repository.", "continues": chat, "repository": repo.id });
    let (status, run) = api
        .call(reqwest::Method::POST, "/v1/chats", Some(named))
        .await;
    assert_eq!(status, 200, "{run}");
    let run: Value = serde_json::from_str(&run).unwrap();
    assert_eq!(run["harness_repository"], false);
    assert_eq!(run["repository_id"], repo.id);
    let other = run["id"].as_i64().unwrap();
    wait_for(&server, other, |run| run.status == "waiting").await;
    end_chat(&api, &server, other).await;
}

#[tokio::test]
async fn a_chat_that_touches_what_is_not_harness_configuration_or_breaks_an_agent_is_not_offered() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), &harness_origin(dir.path()));
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");
    let start = head(&clone);

    let outside = harness_chat(&api, &server, "Write it. [write README.md]").await;
    end_chat(&api, &server, outside).await;
    let errors = error_messages(&server, outside);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("README.md") && errors[0].contains("not harness configuration"),
        "{errors:?}"
    );

    let broken = harness_chat(
        &api,
        &server,
        "Break it.\n[put agents/Coder.toml]\nmodel = \"x\"\nskills = [\"missing\"]\n[/put]",
    )
    .await;
    end_chat(&api, &server, broken).await;
    let errors = error_messages(&server, broken);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("the agent Coder cannot be used") && errors[0].contains("missing"),
        "{errors:?}"
    );

    // Nothing is offered, and the branches stay for the person; the default branch is as it was.
    assert!(server.app.store.harness_changes().unwrap().is_empty());
    assert!(server.app.store.inbox().unwrap().is_empty());
    for chat in [outside, broken] {
        let branch = format!("moebius/chat-{chat}");
        assert!(git(&clone, &["rev-parse", "--verify", &branch]).is_ok());
    }
    assert_eq!(head(&clone), start);
}

#[tokio::test]
async fn two_changes_that_collide_cannot_both_be_merged() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");

    let mut changes = Vec::new();
    for model in ["first-model", "second-model"] {
        let message =
            format!("Change the Coder.\n[put agents/Coder.toml]\nmodel = \"{model}\"\n[/put]");
        let chat = harness_chat(&api, &server, &message).await;
        end_chat(&api, &server, chat).await;
        changes.push(server.app.store.harness_changes().unwrap().pop().unwrap());
    }
    let api = &api;
    let merge = |id: i64| async move {
        let path = format!("/v1/changes/{id}/merge");
        api.call(reqwest::Method::POST, &path, Some(json!({})))
            .await
    };
    let (status, body) = merge(changes[0].id).await;
    assert_eq!(status, 200, "{body}");
    let merged = head(&clone);
    let (status, body) = merge(changes[1].id).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(head(&clone), merged);
    assert_eq!(git(&origin, &["rev-parse", "main"]).unwrap(), merged);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    let second = server.app.store.change(changes[1].id).unwrap().unwrap();
    assert_eq!(second.state, "open");
    assert_eq!(
        agent_model(&server, "Coder").as_deref(),
        Some("first-model")
    );
}

#[tokio::test]
async fn a_chat_in_the_harness_configuration_needs_a_harness_repository_and_no_repository() {
    let dir = tempfile::tempdir().unwrap();
    // Without `[harness]` the request is refused.
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[agents.Coder]\nmodel = \"coder-model\"\n",
        fake_pi.display().to_string()
    );
    let config = Config::parse(&text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
    let plain = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    let api = Api::start(&plain).await;
    let (status, body) = api
        .call(
            reqwest::Method::POST,
            "/v1/chats",
            Some(json!({ "message": "Hi", "harness": true })),
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("[harness]"), "{body}");

    // With it, a repository cannot be named too.
    let other = tempfile::tempdir().unwrap();
    let server = server(other.path(), &harness_origin(other.path()));
    let api = Api::start(&server).await;
    let repo = server
        .app
        .add_repository("demo", source(other.path()).to_str().unwrap())
        .unwrap();
    let (status, body) = api
        .call(
            reqwest::Method::POST,
            "/v1/chats",
            Some(json!({ "message": "Hi", "harness": true, "repository": repo.id })),
        )
        .await;
    assert_eq!(status, 422, "{body}");
}

/// Runs `agent` on a new card and gives the arguments Pi-Bolt was started with.
fn argv_of_a_run(server: &Server, dir: &Path, agent: &str) -> Vec<String> {
    let repo = server
        .app
        .add_repository("demo", source(dir).to_str().unwrap())
        .unwrap();
    let card = server
        .app
        .create_issue(repo.id, "Write done.txt", "", PERSON)
        .unwrap();
    let run = server
        .harness
        .start(&server.app, &card, agent, "code", None)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while server
        .app
        .store
        .run(run.id)
        .unwrap()
        .unwrap()
        .ended_at
        .is_none()
    {
        assert!(Instant::now() < deadline, "the run did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
    let argv = dir.join(format!(".moebius/runs/{}/agent/argv.json", run.id));
    serde_json::from_slice(&std::fs::read(argv).unwrap()).unwrap()
}

fn refuse_pushes(origin: &Path) -> PathBuf {
    let hook = origin.join("hooks/pre-receive");
    std::fs::write(&hook, "#!/bin/sh\necho 'closed for edits' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    hook
}

#[tokio::test]
async fn skills_are_edited_as_commits_and_an_agent_is_given_a_new_skill() {
    use reqwest::Method;
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");
    let subject =
        |repository: &Path| git(repository, &["log", "-1", "--format=%s", "main"]).unwrap();
    let judgements = || server.app.store.judgements("edit_skill").unwrap().len();

    // A skill is read with its text and the other files beside its SKILL.md.
    let (status, body) = api.call(Method::GET, "/v1/skills/house-style", None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"name": "house-style", "description": "Write plainly.", "text": "Body.\n",
               "files": ["notes/extra.md"]})
    );
    let (status, body) = api.call(Method::GET, "/v1/skills/moebius", None).await;
    assert_eq!(status, 200, "{body}");
    let built_in: Value = serde_json::from_str(&body).unwrap();
    assert!(built_in["files"].as_array().unwrap().len() > 5);
    let (status, body) = api
        .call(Method::GET, "/v1/skills/moebius-workflows", None)
        .await;
    assert_eq!(status, 200, "{body}");
    let task: Value = serde_json::from_str(&body).unwrap();
    assert!(task["text"].as_str().unwrap().contains("check-script"));
    assert_eq!(task["files"], json!([]));
    for missing in ["nothing", "..%2Fagents"] {
        let path = format!("/v1/skills/{missing}");
        assert_eq!(api.call(Method::GET, &path, None).await.0, 404);
    }

    // A skill is added: a commit on the origin and on the clone, and a decision.
    let new = json!({"description": "Say \"hi\": politely.", "text": "# Greeting\n\nBe kind."});
    let (status, body) = api
        .call(Method::PUT, "/v1/skills/greeting", Some(new))
        .await;
    assert_eq!(status, 200, "{body}");
    let saved: Value = serde_json::from_str(&body).unwrap();
    let short = git(&clone, &["rev-parse", "--short", "HEAD"]).unwrap();
    assert_eq!(saved["commit"], short.as_str());
    assert_eq!(subject(&origin), "Add the skill greeting");
    assert_eq!(subject(&clone), "Add the skill greeting");
    assert_eq!(
        git(&origin, &["show", "main:skills/greeting/SKILL.md"]).unwrap(),
        "---\nname: greeting\ndescription: \"Say \\\"hi\\\": politely.\"\n---\n# Greeting\n\nBe kind."
    );
    let (_, body) = api.call(Method::GET, "/v1/skills/greeting", None).await;
    let read: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(read["description"], "Say \"hi\": politely.");
    assert_eq!(read["text"], "# Greeting\n\nBe kind.\n");
    let (_, body) = api.call(Method::GET, "/v1/skills", None).await;
    assert!(body.contains("Say \\\"hi\\\": politely."), "{body}");
    assert_eq!(judgements(), 1);

    // Named by an agent, the new skill reaches the agent's next run.
    let agent = json!({"settings": {"model": "m", "skills": ["greeting"]}});
    let (status, body) = api
        .call(Method::PUT, "/v1/agents/Greeter", Some(agent))
        .await;
    assert_eq!(status, 200, "{body}");
    let argv = argv_of_a_run(&server, dir.path(), "Greeter");
    let skill = clone.join("skills/greeting");
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--skill" && w[1] == skill.to_str().unwrap())
    );

    // A skill is changed and its other files stay; the same edit again commits nothing.
    let changed = json!({"description": "Greet.", "text": "Changed.\n"});
    let (status, body) = api
        .call(Method::PUT, "/v1/skills/house-style", Some(changed.clone()))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(subject(&origin), "Update the skill house-style");
    assert!(clone.join("skills/house-style/notes/extra.md").is_file());
    assert_eq!(judgements(), 2);
    let before = head(&clone);
    let (status, body) = api
        .call(Method::PUT, "/v1/skills/house-style", Some(changed))
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(!body.contains("commit"), "{body}");
    assert_eq!(head(&clone), before);

    // A skill that is not valid, and the built-in one, change nothing.
    for (name, body, expected) in [
        ("greeting", json!({"description": " "}), 422),
        ("greeting", json!({"description": "two\nlines"}), 422),
        ("greeting", json!({"description": "x", "color": "red"}), 422),
        ("bad%20name", json!({"description": "x"}), 422),
        ("moebius", json!({"description": "x"}), 409),
        ("moebius-operate", json!({"description": "x"}), 409),
    ] {
        let path = format!("/v1/skills/{name}");
        let (status, text) = api.call(Method::PUT, &path, Some(body)).await;
        assert_eq!(status, expected, "{name}: {text}");
    }
    for name in ["moebius", "moebius-configure"] {
        let path = format!("/v1/skills/{name}");
        assert_eq!(api.call(Method::DELETE, &path, None).await.0, 409);
    }
    assert_eq!(head(&clone), before);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");

    // A skill an agent names is not deleted, and the answer says which agents.
    for (name, agent) in [("greeting", "Greeter"), ("house-style", "Coder")] {
        let path = format!("/v1/skills/{name}");
        let (status, body) = api.call(Method::DELETE, &path, None).await;
        assert_eq!(status, 409);
        assert!(body.contains(agent), "{body}");
    }
    assert_eq!(head(&clone), before);

    // A push that fails undoes the edit and records no decision.
    let unused = json!({"description": "Unused."});
    api.call(Method::PUT, "/v1/skills/unused", Some(unused))
        .await;
    let (before, count) = (head(&clone), judgements());
    let hook = refuse_pushes(&origin);
    for (method, path, body) in [
        (
            Method::PUT,
            "/v1/skills/greeting",
            Some(json!({"description": "never"})),
        ),
        (Method::DELETE, "/v1/skills/unused", None),
    ] {
        let (status, text) = api.call(method, path, body).await;
        assert_eq!(status, 500, "{path}");
        assert!(text.contains("closed for edits"), "{text}");
    }
    assert_eq!(head(&clone), before);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    assert!(clone.join("skills/unused/SKILL.md").is_file());
    assert_eq!(judgements(), count);
    std::fs::remove_file(&hook).unwrap();

    // Deleted, the skill is a commit.
    let (status, body) = api.call(Method::DELETE, "/v1/skills/unused", None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(subject(&origin), "Delete the skill unused");
    assert!(!clone.join("skills/unused").exists());
    assert_eq!(judgements(), count + 1);
    assert_eq!(
        api.call(Method::DELETE, "/v1/skills/unused", None).await.0,
        404
    );
}

#[tokio::test]
async fn mcp_servers_are_edited_as_commits_that_keep_the_rest_of_the_file() {
    use reqwest::Method;
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");
    let subject =
        |repository: &Path| git(repository, &["log", "-1", "--format=%s", "main"]).unwrap();
    let judgements = || {
        server
            .app
            .store
            .judgements("edit_mcp_server")
            .unwrap()
            .len()
    };

    // A server is read as written, with its references and never their values.
    let (status, body) = api.call(Method::GET, "/v1/mcp-servers/token", None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"name": "token", "command": "x", "env": {"TOKEN": "${DOCS_TOKEN}"}})
    );
    assert_eq!(
        api.call(Method::GET, "/v1/mcp-servers/nothing", None)
            .await
            .0,
        404
    );
    assert_eq!(
        api.call(Method::GET, "/v1/mcp-servers/leak", None).await.0,
        422
    );

    // A server is added, and the other tables and the comment stay.
    let new =
        json!({"command": "search", "args": ["--fast", "x y"], "env": {"KEY": "${DOCS_TOKEN}"}});
    let (status, body) = api
        .call(Method::PUT, "/v1/mcp-servers/search", Some(new.clone()))
        .await;
    assert_eq!(status, 200, "{body}");
    let saved: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        saved["commit"],
        git(&clone, &["rev-parse", "--short", "HEAD"])
            .unwrap()
            .as_str()
    );
    assert_eq!(subject(&origin), "Add the MCP server search");
    assert_eq!(subject(&clone), "Add the MCP server search");
    let file = git(&origin, &["show", "main:mcp.toml"]).unwrap();
    assert!(
        file.starts_with("# The servers.\n[docs]\ncommand = \"docs-server\"\n"),
        "{file}"
    );
    assert!(
        file.contains("[token]") && file.contains("[search]\ncommand = \"search\""),
        "{file}"
    );
    let (_, body) = api.call(Method::GET, "/v1/mcp-servers/search", None).await;
    let mut expected = new;
    expected["name"] = json!("search");
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), expected);
    assert_eq!(judgements(), 1);

    // It is changed into a remote server, and an agent that names it is read again.
    let agent = json!({"settings": {"model": "m", "mcp": ["search"]}});
    let (status, body) = api
        .call(Method::PUT, "/v1/agents/Searcher", Some(agent))
        .await;
    assert_eq!(status, 200, "{body}");
    let remote =
        json!({"url": "https://x.test/mcp", "headers": {"Authorization": "Bearer ${DOCS_TOKEN}"}});
    let (status, body) = api
        .call(Method::PUT, "/v1/mcp-servers/search", Some(remote))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(subject(&origin), "Update the MCP server search");
    assert_eq!(judgements(), 2);
    let mcp = server.harness.agents.read().unwrap().agents["Searcher"]
        .mcp
        .clone();
    assert_eq!(mcp["search"].kind(), "remote");

    // A server that would not read is refused with the reason, and nothing changes.
    let before = head(&clone);
    for (body, reason) in [
        (
            json!({"command": "x", "env": {"K": "${OPENAI_API_KEY}"}}),
            "is not in mcp_env",
        ),
        (json!({"command": "x", "url": "u"}), "either command or url"),
        (json!({}), "either command or url"),
        (
            json!({"url": "u", "args": ["a"]}),
            "belong to a server with a command",
        ),
        (json!({"command": " "}), "must not be empty"),
        (json!({"command": "x", "color": "red"}), "color"),
    ] {
        let (status, text) = api
            .call(Method::PUT, "/v1/mcp-servers/bad", Some(body))
            .await;
        assert_eq!(status, 422, "{text}");
        assert!(text.contains(reason), "{text}");
    }
    let (status, _) = api
        .call(
            Method::PUT,
            "/v1/mcp-servers/bad%20name",
            Some(json!({"command": "x"})),
        )
        .await;
    assert_eq!(status, 422);
    assert_eq!(head(&clone), before);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    assert_eq!(judgements(), 2);

    // A server an agent names is not deleted, and the answer says which agents.
    for (name, agent) in [("docs", "Coder"), ("search", "Searcher")] {
        let path = format!("/v1/mcp-servers/{name}");
        let (status, body) = api.call(Method::DELETE, &path, None).await;
        assert_eq!(status, 409);
        assert!(body.contains(agent), "{body}");
    }
    assert_eq!(head(&clone), before);

    // A push that fails undoes the edit and records no decision.
    api.call(
        Method::PUT,
        "/v1/mcp-servers/unused",
        Some(json!({"command": "u"})),
    )
    .await;
    let (before, count) = (head(&clone), judgements());
    let hook = refuse_pushes(&origin);
    for (method, body) in [
        (Method::PUT, Some(json!({"command": "never"}))),
        (Method::DELETE, None),
    ] {
        let (status, text) = api.call(method, "/v1/mcp-servers/unused", body).await;
        assert_eq!(status, 500);
        assert!(text.contains("closed for edits"), "{text}");
    }
    assert_eq!(head(&clone), before);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    let file = git(&clone, &["show", "HEAD:mcp.toml"]).unwrap();
    assert!(file.contains("[unused]\ncommand = \"u\""), "{file}");
    assert_eq!(judgements(), count);
    std::fs::remove_file(&hook).unwrap();

    // Deleted, the server is a commit, and the comment stays.
    let (status, body) = api
        .call(Method::DELETE, "/v1/mcp-servers/unused", None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(subject(&origin), "Delete the MCP server unused");
    let file = git(&origin, &["show", "main:mcp.toml"]).unwrap();
    assert!(!file.contains("[unused]") && file.contains("# The servers."));
    assert_eq!(judgements(), count + 1);
    assert_eq!(
        api.call(Method::DELETE, "/v1/mcp-servers/unused", None)
            .await
            .0,
        404
    );
}
