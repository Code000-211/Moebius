//! A skill's other files through the API: read, written and deleted as commits pushed to the origin, a built-in
//! skill's files read only, and a path that would leave the skill's directory refused.

use std::path::Path;

use moebius_core::repos::git;
use reqwest::Method;
use serde_json::{Value, json};

use super::fixtures::*;

#[tokio::test]
async fn a_skills_other_files_are_read_written_and_deleted_as_commits() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let server = server(dir.path(), &origin);
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");
    let subject =
        |repository: &Path| git(repository, &["log", "-1", "--format=%s", "main"]).unwrap();
    let judgements = || server.app.store.judgements("edit_skill").unwrap().len();
    let json = |body: &str| serde_json::from_str::<Value>(body).unwrap();

    // A file of a skill is read as a repository's file is.
    let (status, body) = api
        .call(
            Method::GET,
            "/v1/skills/house-style/files/notes/extra.md",
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let extra = std::fs::read_to_string(clone.join("skills/house-style/notes/extra.md")).unwrap();
    assert_eq!(
        json(&body),
        json!({"path": "notes/extra.md", "size": extra.len(), "text": extra})
    );
    // A built-in skill's files are read from the binary.
    let (status, body) = api
        .call(
            Method::GET,
            "/v1/skills/moebius/files/guide/agents.md",
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(json(&body)["text"].as_str().unwrap().starts_with("# "));

    // A new file, in a new directory, is a commit on the origin and a decision; the answer is the skill with it.
    let new = json!({"text": "#!/bin/sh\necho fill\n"});
    let (status, body) = api
        .call(
            Method::PUT,
            "/v1/skills/house-style/files/scripts/fill.sh",
            Some(new.clone()),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let saved = json(&body);
    assert_eq!(saved["files"], json!(["notes/extra.md", "scripts/fill.sh"]));
    assert_eq!(
        saved["commit"],
        git(&clone, &["rev-parse", "--short", "HEAD"])
            .unwrap()
            .as_str()
    );
    assert_eq!(
        subject(&origin),
        "Add scripts/fill.sh of the skill house-style"
    );
    assert_eq!(
        git(
            &origin,
            &["show", "main:skills/house-style/scripts/fill.sh"]
        )
        .unwrap(),
        "#!/bin/sh\necho fill"
    );
    assert_eq!(judgements(), 1);
    // The same text again commits nothing; another text updates the file.
    let before = head(&clone);
    let (status, body) = api
        .call(
            Method::PUT,
            "/v1/skills/house-style/files/scripts/fill.sh",
            Some(new),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(json(&body).get("commit").is_none(), "{body}");
    assert_eq!(head(&clone), before);
    let (status, _) = api
        .call(
            Method::PUT,
            "/v1/skills/house-style/files/notes/extra.md",
            Some(json!({"text": "Even more.\n"})),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        subject(&origin),
        "Update notes/extra.md of the skill house-style"
    );

    // What is not one of a skill's other files, or of a skill that can be edited, changes nothing.
    let before = head(&clone);
    let text = || Some(json!({"text": "x"}));
    for (method, path, body, expected) in [
        (Method::PUT, "house-style/files/SKILL.md", text(), 422),
        (
            Method::PUT,
            "house-style/files/a/..%2F..%2F..%2Fagents/Coder.toml",
            text(),
            422,
        ),
        (Method::PUT, "house-style/files/.git/config", text(), 422),
        (Method::PUT, "house-style/files/notes", text(), 422),
        (
            Method::PUT,
            "house-style/files/x.md",
            Some(json!({"text": "x", "mode": 1})),
            422,
        ),
        (Method::PUT, "moebius/files/guide/agents.md", text(), 409),
        (Method::DELETE, "moebius/files/guide/agents.md", None, 409),
        (Method::PUT, "nothing/files/x.md", text(), 404),
        (Method::GET, "house-style/files/missing.md", None, 404),
        (Method::GET, "moebius/files/missing.md", None, 404),
        (Method::DELETE, "house-style/files/missing.md", None, 404),
    ] {
        let route = format!("/v1/skills/{path}");
        let (status, body) = api.call(method.clone(), &route, body).await;
        assert_eq!(status, expected, "{method} {path}: {body}");
    }
    assert_eq!(head(&clone), before);
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");

    // A link in the skill's directory is neither read nor written through. (Unix only: making a link needs a
    // privilege on Windows.)
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path(), clone.join("skills/house-style/outside")).unwrap();
        for method in [Method::GET, Method::PUT] {
            let body = (method == Method::PUT).then(|| json!({"text": "x"}));
            let (status, body) = api
                .call(
                    method,
                    "/v1/skills/house-style/files/outside/moebius.toml",
                    body,
                )
                .await;
            assert_eq!(status, 422, "{body}");
            assert!(body.contains("link"), "{body}");
        }
        std::fs::remove_file(clone.join("skills/house-style/outside")).unwrap();
    }

    // A file is deleted, and the directory it leaves empty with it.
    let (status, body) = api
        .call(
            Method::DELETE,
            "/v1/skills/house-style/files/scripts/fill.sh",
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(json(&body)["files"], json!(["notes/extra.md"]));
    assert_eq!(
        subject(&origin),
        "Delete scripts/fill.sh of the skill house-style"
    );
    assert!(!clone.join("skills/house-style/scripts").exists());
    assert!(clone.join("skills/house-style/SKILL.md").is_file());
    assert_eq!(judgements(), 3);

    // A push the origin refuses undoes the edit.
    let hook = refuse_pushes(&origin);
    let before = head(&clone);
    let (status, body) = api
        .call(
            Method::PUT,
            "/v1/skills/house-style/files/new/file.md",
            Some(json!({"text": "x"})),
        )
        .await;
    assert_eq!(status, 500, "{body}");
    assert_eq!(head(&clone), before);
    assert!(!clone.join("skills/house-style/new").exists());
    assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "");
    assert_eq!(judgements(), 3);
    std::fs::remove_file(hook).unwrap();
}
