//! A chat's branch in the repository: offered as a change whose merge lands on the origin and reaches the agent,
//! refused when it touches more than harness configuration or breaks an agent, colliding changes, and the chats that
//! need a harness repository.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use moebius_core::app::PERSON;
use moebius_core::board::{CardRuns, Variant};
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
        .start(
            &server.app,
            &card,
            "Coder",
            ("code", None),
            Variant::default(),
        )
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
async fn the_work_a_chat_offers_says_what_the_chat_did_and_the_merge_is_named_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");

    // The work is offered while the chat still waits: the commit on its branch says what the agent did — the
    // first line of its last answer — and which chat it was, never just the chat's first message.
    let chat = harness_chat(&api, &server, &skill_message()).await;
    let (status, offered) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/chats/{chat}/offer"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200, "{offered}");
    let branch = format!("moebius/chat-{chat}");
    let agent = server.app.store.run(chat).unwrap().unwrap().agent;
    let message = git(&clone, &["log", "--format=%B", "-1", &branch]).unwrap();
    assert_eq!(
        message.lines().next().unwrap(),
        "You said: Add a skill.",
        "{message}"
    );
    assert!(
        message.contains("[put skills/testing/SKILL.md]"),
        "{message}"
    );
    assert!(
        message.contains(&format!("Chat {chat}: Add a skill (agent {agent}).")),
        "{message}"
    );
    assert!(
        message.contains("Made by Moebius when the chat's work was offered."),
        "{message}"
    );

    // The merge is named after what it carries: the subject of the change's last commit.
    let change = &server.app.store.harness_changes().unwrap()[0];
    let (status, merged) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/changes/{}/merge", change.id),
            Some(json!({ "reason": "named after what it carries" })),
        )
        .await;
    assert_eq!(status, 200, "{merged}");
    let merged_message = git(&origin, &["log", "--format=%B", "-1", "main"]).unwrap();
    assert_eq!(
        merged_message.lines().next().unwrap(),
        format!("Merge {branch}: You said: Add a skill."),
        "{merged_message}"
    );
    assert!(error_messages(&server, chat).is_empty());
}

#[tokio::test]
async fn ending_a_repository_chat_commits_its_work_in_a_message_that_says_what_it_did() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), &harness_origin(dir.path()));
    let api = Api::start(&server).await;

    // A repository chat without a forge: ending it commits what the agent left on the chat's own branch, and
    // that commit says what the chat did and which chat it was, never just the first message.
    let repo = server
        .app
        .add_repository("demo", source(dir.path()).to_str().unwrap())
        .unwrap();
    let (status, run) = api
        .call(
            reqwest::Method::POST,
            "/v1/chats",
            Some(json!({
                "message": "Write the note.\n[put note.txt]\nA note.\n[/put]",
                "repository": repo.id
            })),
        )
        .await;
    assert_eq!(status, 200, "{run}");
    let run: Value = serde_json::from_str(&run).unwrap();
    let chat = run["id"].as_i64().unwrap();
    let agent = run["agent"].as_str().unwrap().to_owned();
    wait_for(&server, chat, |run| run.status == "waiting").await;
    end_chat(&api, &server, chat).await;

    let repository = server.app.store.repository(repo.id).unwrap().unwrap();
    let message = git(
        Path::new(&repository.path),
        &["log", "--format=%B", "-1", &format!("moebius/chat-{chat}")],
    )
    .unwrap();
    assert_eq!(
        message.lines().next().unwrap(),
        "You said: Write the note.",
        "{message}"
    );
    assert!(message.contains("A note."), "{message}");
    assert!(
        message.contains(&format!("Chat {chat}: Write the note (agent {agent}).")),
        "{message}"
    );
    assert!(
        message.contains("Made by Moebius when the chat ended."),
        "{message}"
    );
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

#[tokio::test]
async fn a_chat_in_the_harness_configuration_whose_agent_went_away_is_taken_up_again_on_its_branch()
{
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;

    let chat = harness_chat(&api, &server, &skill_message()).await;
    let worktree = PathBuf::from(
        server
            .app
            .store
            .run(chat)
            .unwrap()
            .unwrap()
            .worktree
            .unwrap(),
    );
    let (status, body) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/runs/{chat}/stop"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let ended = wait_for(&server, chat, |run| run.ended_at.is_some()).await;
    assert_eq!(ended.status, "stopped");
    assert!(!worktree.exists(), "the worktree went with the agent");

    let (status, body) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/chats/{chat}/messages"),
            Some(json!({ "message": "Carry on" })),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let run = wait_for(&server, chat, |run| run.status == "waiting").await;
    assert_eq!((run.ended_at, run.harness_repository), (None, true));
    assert!(
        worktree.join("skills/testing/SKILL.md").is_file(),
        "the branch is back where its agent left it"
    );
    let (status, body) = api
        .call(
            reqwest::Method::POST,
            &format!("/v1/runs/{chat}/stop"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    wait_for(&server, chat, |run| run.ended_at.is_some()).await;
}

#[tokio::test]
async fn deleting_a_chat_in_the_harness_configuration_removes_its_branch_and_change_from_the_clone()
{
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");
    let start = head(&clone);

    let chat = harness_chat(&api, &server, &skill_message()).await;
    let other = harness_chat(&api, &server, &skill_message()).await;
    end_chat(&api, &server, chat).await;
    end_chat(&api, &server, other).await;
    assert_eq!(server.app.store.harness_changes().unwrap().len(), 2);
    let branch = format!("moebius/chat-{chat}");
    assert!(
        git(&clone, &["branch", "--list", &branch])
            .unwrap()
            .contains(&branch)
    );

    let (status, body) = api
        .call(
            reqwest::Method::DELETE,
            &format!("/v1/chats/{chat}"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(server.app.store.run(chat).unwrap().is_none());
    assert_eq!(git(&clone, &["branch", "--list", &branch]).unwrap(), "");
    // The other chat's change, inbox item and branch stay, and the default branch has not moved.
    let changes = server.app.store.harness_changes().unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].branch, format!("moebius/chat-{other}"));
    assert_eq!(server.app.store.inbox().unwrap().len(), 1);
    assert!(
        git(
            &clone,
            &["branch", "--list", &format!("moebius/chat-{other}")]
        )
        .unwrap()
        .contains("chat-")
    );
    assert_eq!(head(&clone), start);
}
