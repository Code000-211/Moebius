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

/// An agent's `engineering_skills` is true, false or a list of the engineering skills' names: a list gives the agent
/// exactly those, under any name, and a name that is not one makes the agent unusable instead of being ignored.
#[tokio::test]
async fn a_list_of_engineering_skills_gives_an_agent_exactly_those_and_a_wrong_name_is_its_error() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let data = server.app.data_dir.join("skills");
    let path = |name: &str| data.join(name).to_string_lossy().into_owned();
    let card = new_card(&server, dir.path());

    // A copy of the Coder under another name, as the API saves it, has the Coder's skills.
    let coder_skills = json!(["tdd", "diagnosing-bugs", "moebius-autonomous"]);
    let copy =
        json!({"settings": {"model": "m", "engineering_skills": coder_skills}, "instructions": ""});
    let (status, body) = api
        .call(Method::PUT, "/v1/agents/Coder-fast", Some(copy))
        .await;
    assert_eq!(status, 200, "{body}");
    let (_, body) = api.call(Method::GET, "/v1/agents/Coder-fast", None).await;
    let shown: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(shown["settings"]["engineering_skills"], coder_skills);
    let fast = skills(&argv_of_a_run(&server, dir.path(), &card, "Coder-fast"));
    assert_eq!(
        fast,
        ["tdd", "diagnosing-bugs", "moebius-autonomous"].map(path)
    );

    // Exactly the list, for the Reviewer too, and nothing it does not name.
    let only =
        json!({"settings": {"model": "m", "engineering_skills": ["tdd"]}, "instructions": ""});
    let (status, body) = api
        .call(Method::PUT, "/v1/agents/Reviewer", Some(only))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        skills(&argv_of_a_run(&server, dir.path(), &card, "Reviewer")),
        [path("tdd")]
    );

    // A name that is not an engineering skill is refused, and in a file it is the agent's error.
    let wrong = json!({"settings": {"model": "m", "engineering_skills": ["tdd", "house-style"]}, "instructions": ""});
    let (status, body) = api.call(Method::PUT, "/v1/agents/Wrong", Some(wrong)).await;
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("not an engineering skill"), "{body}");
    let other = dir.path().join("other");
    moebius_core::repos::git(
        dir.path(),
        &["clone", "--quiet", origin.to_str().unwrap(), "other"],
    )
    .unwrap();
    write(
        &other,
        "agents/Wrong.toml",
        "model = \"x\"\nengineering_skills = [\"nope\"]\n",
    );
    write(
        &other,
        "agents/Flag.toml",
        "model = \"x\"\nengineering_skills = \"yes\"\n",
    );
    commit_all(&other, "Add agents");
    moebius_core::repos::git(&other, &["push", "--quiet", "origin", "main"]).unwrap();
    server.refresh_harness_repository();
    let set = server.harness.agents.read().unwrap().clone();
    assert!(
        set.errors["Wrong"].contains("nope is not an engineering skill"),
        "{:?}",
        set.errors
    );
    assert!(
        set.errors["Flag"].contains("true, false or a list"),
        "{:?}",
        set.errors
    );
    assert!(!set.agents.contains_key("Wrong"));
}
