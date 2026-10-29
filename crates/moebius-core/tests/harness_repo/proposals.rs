//! A chat's branch in the repository: offered as a change whose merge lands on the origin and reaches the agent,
//! refused when it touches more than harness configuration or breaks an agent, colliding changes, and the chats that
//! need a harness repository.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use moebius_core::app::PERSON;
use moebius_core::board::Runs;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::store::Run;
use serde_json::{Value, json};

use super::fixtures::*;

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

async fn wait_for(server: &Server, id: i64, done: impl Fn(&Run) -> bool) -> Run {
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
    let argv = argv(dir.path(), run.id);
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
