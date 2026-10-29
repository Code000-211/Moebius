//! The engineering skills Moebius brings for the Coder, the Reviewer and the Grilling partner: listed with their source,
//! given to a run, turned off by the agent, and replaced by a skill of the repository with the same name.

use reqwest::Method;
use serde_json::{Value, json};

use super::fixtures::*;

/// The `--skill` arguments of a run.
fn skills(argv: &[String]) -> Vec<String> {
    argv.windows(2)
        .filter(|pair| pair[0] == "--skill")
        .map(|pair| pair[1].clone())
        .collect()
}

#[tokio::test]
async fn the_engineering_skills_are_listed_given_to_runs_turned_off_and_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let data = server.app.data_dir.join("skills");
    let json = |body: &str| serde_json::from_str::<Value>(body).unwrap();

    // They are listed as built in, with where they were copied from.
    let (status, body) = api.call(Method::GET, "/v1/skills", None).await;
    assert_eq!(status, 200, "{body}");
    let listed = json(&body);
    let tdd = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|skill| skill["name"] == "tdd")
        .unwrap();
    assert_eq!(tdd["built_in"], true);
    assert_eq!(tdd["source"]["url"], "https://github.com/mattpocock/skills");
    assert_eq!(tdd["source"]["commit"].as_str().unwrap().len(), 40);
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|skill| skill["name"] == "moebius-autonomous" && skill["source"].is_null())
    );
    let (_, body) = api.call(Method::GET, "/v1/skills/tdd", None).await;
    let view = json(&body);
    assert_eq!(view["files"], json!(["tests.md", "mocking.md", "LICENSE"]));
    assert_eq!(view["source"], tdd["source"]);
    let (status, body) = api
        .call(Method::GET, "/v1/skills/tdd/files/tests.md", None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(
        json(&body)["text"]
            .as_str()
            .unwrap()
            .starts_with("# Good and Bad Tests")
    );
    // Without a skill of the repository they are read only.
    let (status, _) = api
        .call(
            Method::PUT,
            "/v1/skills/tdd/files/tests.md",
            Some(json!({"text": "x"})),
        )
        .await;
    assert_eq!(status, 409);

    // The Coder and the Reviewer are given theirs and the adapter, before their own skills.
    let card = new_card(&server, dir.path());
    let coder = skills(&argv_of_a_run(&server, dir.path(), &card, "Coder"));
    let expected = [
        data.join("tdd"),
        data.join("diagnosing-bugs"),
        data.join("moebius-autonomous"),
    ]
    .map(|path| path.to_string_lossy().into_owned());
    assert_eq!(coder[..3], expected);
    assert!(coder[3].ends_with("skills/house-style"), "{coder:?}");
    assert!(data.join("tdd/tests.md").is_file() && data.join("tdd/LICENSE").is_file());
    assert!(
        data.join("diagnosing-bugs/scripts/hitl-loop.template.sh")
            .is_file()
    );
    let reviewer = skills(&argv_of_a_run(&server, dir.path(), &card, "Reviewer"));
    assert_eq!(
        reviewer,
        [data.join("code-review"), data.join("moebius-autonomous")]
            .map(|path| path.to_string_lossy().into_owned())
    );

    // A skill of the repository with the name of one replaces it, for the agents that have it.
    let own = json!({"description": "Our test-first way.", "text": "Write the test first.\n"});
    let (status, body) = api.call(Method::PUT, "/v1/skills/tdd", Some(own)).await;
    assert_eq!(status, 200, "{body}");
    let (_, body) = api.call(Method::GET, "/v1/skills", None).await;
    let listed = json(&body);
    let tdds: Vec<&Value> = listed
        .as_array()
        .unwrap()
        .iter()
        .filter(|skill| skill["name"] == "tdd")
        .collect();
    assert_eq!(tdds.len(), 1);
    assert_eq!(tdds[0]["built_in"], false);
    assert!(tdds[0]["source"].is_null());
    let coder = skills(&argv_of_a_run(&server, dir.path(), &card, "Coder"));
    assert!(
        coder[0].ends_with(".moebius/harness/skills/tdd"),
        "{coder:?}"
    );
    assert_eq!(coder[1], data.join("diagnosing-bugs").to_string_lossy());

    // An agent turns them off, and keeps its own.
    let (_, body) = api.call(Method::GET, "/v1/agents/Coder", None).await;
    let agent = json(&body);
    let mut edit = json!({"settings": agent["settings"], "instructions": agent["instructions"]});
    edit["settings"]["engineering_skills"] = json!(false);
    let (status, body) = api.call(Method::PUT, "/v1/agents/Coder", Some(edit)).await;
    assert_eq!(status, 200, "{body}");
    let (_, body) = api.call(Method::GET, "/v1/agents/Coder", None).await;
    assert_eq!(json(&body)["settings"]["engineering_skills"], false);
    let coder = skills(&argv_of_a_run(&server, dir.path(), &card, "Coder"));
    assert_eq!(coder.len(), 1, "{coder:?}");
    assert!(coder[0].ends_with("skills/house-style"));
}
