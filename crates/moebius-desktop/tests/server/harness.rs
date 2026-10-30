//! The harness configuration repository against a real server: agents, rules, skills, a skill's files and MCP servers
//! edited as commits, and setting up the repository from `moebius.toml`.

use moebius_core::repos::git;
use moebius_desktop::client::{Client, ClientError};

use crate::fixtures::{harness_origin, start_with};

/// Editing an agent in the app is a commit in the harness configuration repository, and an invalid edit is refused with
/// the server's message.
#[tokio::test(flavor = "multi_thread")]
async fn editing_an_agent_is_a_commit_on_the_harness_repository_and_a_bad_edit_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let more = format!("[harness]\nurl = {:?}\n", origin.display().to_string());
    let (settings, _server) = start_with(dir.path(), &more).await;
    let client = Client::new(settings);

    let agents = client.agents().await.unwrap();
    assert!(
        agents
            .iter()
            .any(|agent| agent.name == "Coder" && agent.model == "coder-model")
    );
    let skills = client.skills().await.unwrap();
    assert_eq!(skills[0].name, "moebius");
    assert!(skills[0].built_in);
    // The built-in skills come first, then the harness repository's own.
    let own = skills.iter().position(|skill| !skill.built_in).unwrap();
    assert!(skills[..own].iter().all(|skill| skill.built_in));
    assert_eq!(skills[own].name, "house-style");
    assert_eq!(skills[own].description, "Write plainly.");
    let servers = client.mcp_servers().await.unwrap();
    assert_eq!(
        (servers[0].name.as_str(), servers[0].kind.as_str()),
        ("docs", "stdio")
    );

    let coder = client.agent("Coder").await.unwrap();
    assert_eq!(coder.instructions, "Keep changes small.\n");
    assert!(coder.built_in.is_none());
    let moebius = client.agent("Moebius").await.unwrap();
    assert!(moebius.settings.is_none());
    let built_in = moebius.built_in.expect("the Moebius agent is built in");
    assert!(
        built_in
            .instructions
            .starts_with("You are the Moebius agent.")
    );
    assert_eq!(built_in.skills.len(), 7);
    assert_eq!(built_in.skills[1].name, "moebius-operate");
    let mut coder = coder.settings.expect("the Coder has files");
    coder.model = "better-model".into();
    coder.thinking = Some("high".into());
    coder.mcp = vec!["docs".into()];
    let saved = client
        .save_agent("Coder", &coder, "Be careful.")
        .await
        .unwrap();
    let commit = saved.commit.expect("the edit is a commit");
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Update the agent Coder"
    );
    assert!(
        git(&origin, &["rev-parse", "main"])
            .unwrap()
            .starts_with(&commit)
    );
    let file = git(&origin, &["show", "main:agents/Coder.toml"]).unwrap();
    assert!(file.contains("better-model") && file.contains("thinking = \"high\""));
    assert_eq!(
        client.agent("Coder").await.unwrap().instructions,
        "Be careful."
    );

    // A new agent is a commit too, and an agent that names what is not there is refused with the reason.
    let mut strong = coder.clone();
    strong.model = "big".into();
    client.save_agent("Strong", &strong, "").await.unwrap();
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Add the agent Strong"
    );
    let head = git(&origin, &["rev-parse", "main"]).unwrap();
    strong.skills = vec!["missing".into()];
    let Err(ClientError::Server { status, message }) =
        client.save_agent("Strong", &strong, "").await
    else {
        panic!("the edit was accepted");
    };
    assert_eq!(status, 422);
    assert!(
        message.contains("\"missing\" is not in skills/"),
        "{message}"
    );
    assert_eq!(git(&origin, &["rev-parse", "main"]).unwrap(), head);

    client.delete_agent("Strong").await.unwrap();
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Delete the agent Strong"
    );
}

/// Saving the rules in the app is a commit in the harness configuration repository, and text that does not parse is
/// refused with the server's message.
#[tokio::test(flavor = "multi_thread")]
async fn saving_the_rules_is_a_commit_on_the_harness_repository_and_bad_text_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let more = format!("[harness]\nurl = {:?}\n", origin.display().to_string());
    let (settings, _server) = start_with(dir.path(), &more).await;
    let client = Client::new(settings);

    let view = client.rules().await.unwrap();
    assert_eq!(view.patterns.len(), 5);
    assert!(view.prose.editable && view.prose.rules.is_empty());

    let text = "## leave-ci-alone\nDo not edit .github/.";
    let saved = client.save_rules(text).await.unwrap();
    let commit = saved.commit.expect("the save is a commit");
    assert_eq!(saved.prose.rules[0].name, "leave-ci-alone");
    assert_eq!(
        git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap(),
        "Update the rules"
    );
    assert!(
        git(&origin, &["rev-parse", "main"])
            .unwrap()
            .starts_with(&commit)
    );
    assert_eq!(git(&origin, &["show", "main:rules.md"]).unwrap(), text);
    assert!(client.save_rules(text).await.unwrap().commit.is_none());

    let head = git(&origin, &["rev-parse", "main"]).unwrap();
    let Err(ClientError::Server { status, message }) = client.save_rules("## a\n## a\n").await
    else {
        panic!("the text was accepted");
    };
    assert_eq!(status, 422);
    assert!(message.contains("named twice"), "{message}");
    assert_eq!(git(&origin, &["rev-parse", "main"]).unwrap(), head);
}

/// Editing a skill and an MCP server in the app is a commit in the harness configuration repository; one an agent names
/// is not deleted, and an invalid server is refused with the server's message.
#[tokio::test(flavor = "multi_thread")]
async fn editing_a_skill_and_an_mcp_server_is_a_commit_and_a_named_one_is_not_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let more = format!(
        "[harness]\nurl = {:?}\nmcp_env = [\"DOCS_TOKEN\"]\n",
        origin.display().to_string()
    );
    let (settings, _server) = start_with(dir.path(), &more).await;
    let client = Client::new(settings);
    let subject = || git(&origin, &["log", "-1", "--format=%s", "main"]).unwrap();

    let saved = client
        .save_skill("greeting", "Say hello.", "# Greeting\n\nBe kind.")
        .await
        .unwrap();
    assert!(saved.commit.is_some());
    assert_eq!(subject(), "Add the skill greeting");
    let read = client.skill("greeting").await.unwrap();
    assert_eq!(
        (read.description.as_str(), read.text.as_str()),
        ("Say hello.", "# Greeting\n\nBe kind.\n")
    );
    let Err(ClientError::Server { status, .. }) = client.save_skill("moebius", "x", "").await
    else {
        panic!("the built-in skill was changed");
    };
    assert_eq!(status, 409);

    // A skill's other files are written, read and deleted, each a commit; a path is sent encoded.
    let saved = client
        .save_skill_file("greeting", "examples/a #1.md", "Hello, you.\n")
        .await
        .unwrap();
    assert_eq!(saved.files, ["examples/a #1.md"]);
    assert!(saved.commit.is_some());
    assert_eq!(subject(), "Add examples/a #1.md of the skill greeting");
    let read = client
        .skill_file("greeting", "examples/a #1.md")
        .await
        .unwrap();
    assert_eq!(read.text.as_deref(), Some("Hello, you.\n"));
    let built_in = client
        .skill_file("moebius", "guide/agents.md")
        .await
        .unwrap();
    assert!(built_in.text.is_some());
    let Err(ClientError::Server { status, .. }) =
        client.save_skill_file("greeting", "SKILL.md", "x").await
    else {
        panic!("SKILL.md was written as a file");
    };
    assert_eq!(status, 422);
    let deleted = client
        .delete_skill_file("greeting", "examples/a #1.md")
        .await
        .unwrap();
    assert!(deleted.files.is_empty());
    assert_eq!(subject(), "Delete examples/a #1.md of the skill greeting");

    let definition = moebius_desktop::client::McpDefinition {
        command: Some("search".into()),
        args: vec!["--fast".into()],
        env: [("KEY".to_string(), "${DOCS_TOKEN}".to_string())].into(),
        ..Default::default()
    };
    client.save_mcp_server("search", &definition).await.unwrap();
    assert_eq!(subject(), "Add the MCP server search");
    let read = client.mcp_server("search").await.unwrap();
    assert_eq!(read.definition, definition);
    let leaking = moebius_desktop::client::McpDefinition {
        env: [("KEY".to_string(), "${OPENAI_API_KEY}".to_string())].into(),
        ..definition
    };
    let Err(ClientError::Server { status, message }) =
        client.save_mcp_server("search", &leaking).await
    else {
        panic!("the edit was accepted");
    };
    assert_eq!(status, 422);
    assert!(message.contains("is not in mcp_env"), "{message}");
    assert_eq!(subject(), "Add the MCP server search");

    // house-style and docs are named by the Coder.
    let mut coder = client.agent("Coder").await.unwrap().settings.unwrap();
    coder.mcp = vec!["docs".into()];
    client.save_agent("Coder", &coder, "").await.unwrap();
    for result in [
        client.delete_skill("house-style").await,
        client.delete_mcp_server("docs").await,
    ] {
        let Err(ClientError::Server { status, message }) = result else {
            panic!("a named one was deleted");
        };
        assert_eq!(status, 409);
        assert!(message.contains("Coder"), "{message}");
    }
    client.delete_skill("greeting").await.unwrap();
    assert_eq!(subject(), "Delete the skill greeting");
    client.delete_mcp_server("search").await.unwrap();
    assert_eq!(subject(), "Delete the MCP server search");
}

/// "Set up" against a real server: the agents of `moebius.toml` move into a new harness configuration repository next
/// to it, which the file then names; the server uses it once it restarts.
#[tokio::test(flavor = "multi_thread")]
async fn setting_up_a_harness_repository_moves_the_agents_out_of_moebius_toml() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("rules.md"),
        "## Small\n\nKeep changes small.\n",
    )
    .unwrap();
    let (settings, _server) = start_with(
        dir.path(),
        "[agents.Coder]\nmodel = \"coder-model\"\ninstructions = \"Be brief.\"\n",
    )
    .await;
    let client = Client::new(settings);
    let saved = client.set_up_harness().await.unwrap();
    assert!(saved.restart_needed);
    let harness = dir.path().join("harness");
    assert_eq!(
        std::fs::read_to_string(harness.join("agents/Coder.md")).unwrap(),
        "Be brief.\n"
    );
    assert!(
        std::fs::read_to_string(harness.join("agents/Coder.toml"))
            .unwrap()
            .contains("model = \"coder-model\"")
    );
    assert_eq!(
        std::fs::read_to_string(harness.join("rules.md")).unwrap(),
        "## Small\n\nKeep changes small.\n"
    );
    let text = std::fs::read_to_string(dir.path().join("moebius.toml")).unwrap();
    assert!(
        text.contains(&format!("[harness]\nurl = \"{}\"", harness.display())),
        "{text}"
    );
    assert!(!text.contains("[agents"), "{text}");
    // A second setup is refused with the server's reason.
    let error = client.set_up_harness().await.unwrap_err();
    assert!(
        matches!(&error, ClientError::Server { status: 409, .. }),
        "{error:?}"
    );
}
