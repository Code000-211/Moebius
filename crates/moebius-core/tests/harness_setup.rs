//! `POST /v1/harness/setup`: the agents and rules of `moebius.toml` move into a new harness configuration repository
//! next to it, the file names the repository instead, and a server started on the new file reads the agents from it
//! and pushes its edits to it. A server that has a repository already, a directory that is taken and an agent that
//! cannot move are refused with nothing changed.

use std::path::Path;
use std::sync::Arc;

use moebius_core::config::Config;
use moebius_core::decide::Actor;
use moebius_core::harness::repo::{AgentDefinition, AgentSettings};
use moebius_core::repos::git;
use moebius_core::router::Protocol;
use moebius_core::server::Server;
use serde_json::{Value, json};

const FILE: &str = "# Moebius, edited by hand.\nrun_limit = 4 # runs at a time\n\n[router]\nlisten = \"127.0.0.1:0\"\n\n\
# The agents.\n[agents.Coder]\nmodel = \"coder-model\"\ninstructions = \"Keep changes small.\"\n\
skills = [\"skills/house-style\"]\nextensions = [\"ext/log.ts\"]\n\n\
[agents.Reviewer]\nmodel = \"reviewer-model\"\nprotocol = \"anthropic\"\nthinking = \"high\"\ntoken_budget = 9000\n";

const RULES: &str = "## No secrets\n\nNever print a key.\n";

/// A server on `moebius.toml` in `dir`, and the address and token of its API.
async fn serve(dir: &Path) -> (Arc<Server>, String, String) {
    let config = Config::load(&dir.join("moebius.toml")).unwrap();
    let api = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let router = tokio::net::TcpListener::bind(config.router_listen)
        .await
        .unwrap();
    let (api_addr, router_addr) = (api.local_addr().unwrap(), router.local_addr().unwrap());
    let server = Arc::new(Server::new(config, router_addr).unwrap());
    let token = server.token.to_string();
    tokio::spawn(server.clone().serve(api, router));
    (server, format!("http://{api_addr}"), token)
}

async fn set_up(api: &str, token: &str) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(format!("{api}/v1/harness/setup"))
        .bearer_auth(token)
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

/// A directory with `moebius.toml` (`text`), the rules file, the Coder's skill and its extension.
fn configuration(text: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("moebius.toml"), text).unwrap();
    std::fs::write(dir.path().join("rules.md"), RULES).unwrap();
    let skill = dir.path().join("skills/house-style");
    std::fs::create_dir_all(skill.join("examples")).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: house-style\ndescription: Write plainly.\n---\n\nShort sentences.\n",
    )
    .unwrap();
    std::fs::write(skill.join("examples/good.md"), "Good.\n").unwrap();
    std::fs::create_dir_all(dir.path().join("ext")).unwrap();
    std::fs::write(dir.path().join("ext/log.ts"), "export default () => {};\n").unwrap();
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agents_and_rules_move_into_a_new_repository_that_the_restarted_server_uses() {
    let dir = configuration(FILE);
    let (server, api, token) = serve(dir.path()).await;
    let (status, answer) = set_up(&api, &token).await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["restart_needed"], true);
    assert_eq!(answer["restart_keys"], json!(["harness.url"]));

    // The repository: one commit on main with every file, and a clean working tree.
    let harness = dir.path().join("harness");
    let git = |args: &[&str]| git(&harness, args).unwrap();
    assert_eq!(git(&["branch", "--show-current"]), "main");
    assert_eq!(
        git(&["log", "--format=%s"]),
        "Set up the harness configuration repository"
    );
    assert_eq!(
        git(&["ls-files"]).lines().collect::<Vec<_>>(),
        [
            "README.md",
            "agents/Coder.md",
            "agents/Coder.toml",
            "agents/Reviewer.toml",
            "extensions/log.ts",
            "mcp.toml",
            "rules.md",
            "skills/house-style/SKILL.md",
            "skills/house-style/examples/good.md",
        ]
    );
    assert_eq!(git(&["status", "--porcelain"]), "");
    let read = |file: &str| std::fs::read_to_string(harness.join(file)).unwrap();
    let coder: AgentSettings = toml::from_str(&read("agents/Coder.toml")).unwrap();
    assert_eq!(coder.model, "coder-model");
    assert_eq!(coder.skills, ["house-style"]);
    assert_eq!(coder.extensions, ["extensions/log.ts"]);
    assert_eq!(read("agents/Coder.md"), "Keep changes small.\n");
    let reviewer: AgentSettings = toml::from_str(&read("agents/Reviewer.toml")).unwrap();
    assert_eq!(reviewer.protocol, Protocol::Anthropic);
    assert_eq!(reviewer.thinking.as_deref(), Some("high"));
    assert_eq!(reviewer.token_budget, Some(9000));
    assert_eq!(read("rules.md"), RULES);
    assert_eq!(read("extensions/log.ts"), "export default () => {};\n");

    // The file names the repository, keeps its other keys and comments, and has no agents.
    let text = std::fs::read_to_string(dir.path().join("moebius.toml")).unwrap();
    assert!(
        text.starts_with("# Moebius, edited by hand.\nrun_limit = 4 # runs at a time\n"),
        "{text}"
    );
    assert!(!text.contains("[agents"), "{text}");
    let config = Config::load(&dir.path().join("moebius.toml")).unwrap();
    assert_eq!(config.harness_url, Some(harness.display().to_string()));
    assert!(config.agents.is_empty());
    assert_eq!(config.rules_file, None);
    assert_eq!(config.run_limit, 4);

    // The person's decision, and the running server unchanged until it restarts.
    let judgements = server.app.store.judgements("setup_harness").unwrap();
    assert_eq!(judgements.len(), 1);
    assert!(server.harness_repo.is_none());

    // Once more is refused: the file has [harness] now.
    let (status, why) = set_up(&api, &token).await;
    assert_eq!(status, 409, "{why}");

    // A server started on the new file (with data of its own) reads the agents from its clone and pushes edits to the
    // repository, whose working tree follows.
    let other = tempfile::tempdir().unwrap();
    let config = Config::parse(&text, other.path(), &other.path().join("moebius.toml")).unwrap();
    let restarted = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    let repo = restarted
        .harness_repo
        .clone()
        .expect("a harness repository");
    let agents = restarted.harness.agents.read().unwrap().clone();
    assert_eq!(
        agents.agents.keys().collect::<Vec<_>>(),
        ["Coder", "Reviewer"]
    );
    assert!(agents.errors.is_empty(), "{:?}", agents.errors);
    assert_eq!(
        agents.agents["Coder"].instructions.as_deref(),
        Some("Keep changes small.")
    );
    assert!(agents.agents["Coder"].skills[0].ends_with("skills/house-style"));
    let definition = AgentDefinition {
        settings: AgentSettings {
            model: "planner-model".into(),
            protocol: Protocol::OpenAi,
            thinking: None,
            token_budget: None,
            skills: vec!["house-style".into()],
            extensions: vec![],
            mcp: vec![],
            harness: Default::default(),
            tool_output: Default::default(),
        },
        instructions: String::new(),
    };
    let saved = repo
        .save_agent(&restarted.app, &Actor::Person, "Planner", &definition)
        .unwrap();
    assert!(saved.commit.is_some());
    assert!(read("agents/Planner.toml").contains("planner-model"));
    assert_eq!(git(&["status", "--porcelain"]), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_setup_that_cannot_be_made_changes_nothing() {
    // An agent whose skill is not there: nothing is made and the file stays.
    let missing = FILE.replace("skills/house-style", "skills/gone");
    let dir = configuration(&missing);
    let (server, api, token) = serve(dir.path()).await;
    let (status, why) = set_up(&api, &token).await;
    assert_eq!(status, 422, "{why}");
    assert!(why.as_str().unwrap().contains("skills/gone"), "{why}");
    assert!(!dir.path().join("harness").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("moebius.toml")).unwrap(),
        missing
    );
    assert!(
        server
            .app
            .store
            .judgements("setup_harness")
            .unwrap()
            .is_empty()
    );

    // A directory that is taken is left as it is.
    let dir = configuration(FILE);
    std::fs::create_dir(dir.path().join("harness")).unwrap();
    std::fs::write(dir.path().join("harness/notes.txt"), "mine\n").unwrap();
    let (_server, api, token) = serve(dir.path()).await;
    let (status, why) = set_up(&api, &token).await;
    assert_eq!(status, 409, "{why}");
    assert!(why.as_str().unwrap().contains("not empty"), "{why}");
    assert_eq!(
        std::fs::read_dir(dir.path().join("harness"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("moebius.toml")).unwrap(),
        FILE
    );

    // An empty directory is used, and stays when the setup fails.
    let dir = configuration(&missing);
    std::fs::create_dir(dir.path().join("harness")).unwrap();
    let (_server, api, token) = serve(dir.path()).await;
    assert_eq!(set_up(&api, &token).await.0, 422);
    assert_eq!(
        std::fs::read_dir(dir.path().join("harness"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_with_a_harness_repository_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin");
    std::fs::create_dir(&origin).unwrap();
    moebius_core::harness::repo::create(&origin, &Default::default(), None).unwrap();
    let text = format!(
        "[router]\nlisten = \"127.0.0.1:0\"\n[harness]\nurl = {:?}\n",
        origin.display().to_string()
    );
    std::fs::write(dir.path().join("moebius.toml"), &text).unwrap();
    let (_server, api, token) = serve(dir.path()).await;
    let (status, why) = set_up(&api, &token).await;
    assert_eq!(status, 409, "{why}");
    assert!(!dir.path().join("harness").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("moebius.toml")).unwrap(),
        text
    );
}
