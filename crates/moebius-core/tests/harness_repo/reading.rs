//! Reading the repository: the agents and rules it says, a broken agent left out alone, a run of an agent defined
//! only there, the skills and MCP servers listed without secrets, a built-in agent with its own files, and a commit
//! someone else pushes arriving at the next refresh.

use moebius_core::board::Runs;
use moebius_core::decide::rules;
use moebius_core::repos::git;
use moebius_core::router::Protocol;
use serde_json::{Value, json};

use super::fixtures::*;

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

// Unix only: the run is on the fake Pi, a Python script started by its `#!` line.
#[cfg(unix)]
#[test]
fn an_agent_defined_only_in_the_repository_runs_with_its_model_and_instructions() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), &harness_origin(dir.path()));
    let card = new_card(&server, dir.path());
    let argv = argv_of_a_run(&server, dir.path(), &card, "Coder");
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
            "moebius-report",
            "moebius-autonomous",
            "tdd",
            "diagnosing-bugs",
            "code-review",
            "grilling",
            "to-tickets",
            "house-style"
        ]
    );
    for skill in &skills.as_array().unwrap()[..8] {
        assert_eq!(skill["built_in"], true);
        assert!(skill["description"].as_str().unwrap().contains("Use it"));
    }
    // The engineering skills are built in too, and say where they came from.
    for skill in &skills.as_array().unwrap()[8..13] {
        assert_eq!(skill["built_in"], true);
        assert!(skill["source"]["commit"].is_string(), "{skill}");
    }
    assert_eq!(
        skills[13],
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
    assert_eq!(read["built_in"]["skills"].as_array().unwrap().len(), 7);

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
