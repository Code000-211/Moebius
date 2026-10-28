//! The harness configuration repository: agents and rules come from a git repository, a run uses them, an edit through
//! the API is a commit to it, and a commit someone else pushes arrives at the next refresh.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use moebius_core::api::{self, ApiState};
use moebius_core::app::PERSON;
use moebius_core::board::Runs;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::router::Protocol;
use moebius_core::rules;
use moebius_core::server::Server;
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
        "[docs]\ncommand = \"docs-server\"\n\
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
    assert_eq!(
        skills[1],
        json!({"name": "house-style", "description": "Write plainly.", "built_in": false})
    );
    assert_eq!(skills.as_array().unwrap().len(), 2);

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
