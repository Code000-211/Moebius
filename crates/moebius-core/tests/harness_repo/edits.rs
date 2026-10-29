//! Edits through the API: agents, rules, skills and MCP servers changed as commits pushed to the origin, a push the
//! origin refuses undone, and the rest of a file kept.

use std::path::Path;

use moebius_core::board::Runs;
use moebius_core::repos::git;
use serde_json::{Value, json};

use super::fixtures::*;

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
    #[cfg(unix)]
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

// Unix only: the run is on the fake Pi, a Python script started by its `#!` line.
#[cfg(unix)]
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
    let card = new_card(&server, dir.path());
    let argv = argv_of_a_run(&server, dir.path(), &card, "Greeter");
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
