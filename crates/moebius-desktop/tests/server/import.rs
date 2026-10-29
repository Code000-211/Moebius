//! Importing skills against a real server, from the file a person chooses in the Import panel: the app sends the zip,
//! the Moebius agent (the fake Pi) is asked to import from where the server keeps it, and the skill it writes through
//! the API arrives in the harness configuration repository with its other files.

use std::path::Path;
use std::time::Duration;

use moebius_core::repos::git;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{Client, Snapshot};
use moebius_desktop::screens::skills;
use moebius_desktop::widgets::import;

use crate::fixtures::{harness_origin, settle, shown, start_with};

/// What the agent does with the request, as the fake Pi runs it: unzips the upload into its working directory and
/// writes each skill, then each of its other files, through the API with its run token.
const AGENT: &str = r#"
import json, os, sys, urllib.request, zipfile
zipfile.ZipFile(sys.argv[1]).extractall("import")
def put(route, body):
    request = urllib.request.Request(os.environ["MOEBIUS_URL"] + route, data=json.dumps(body).encode(), method="PUT",
        headers={"Authorization": "Bearer " + os.environ["MOEBIUS_TOKEN"], "Content-Type": "application/json"})
    urllib.request.urlopen(request).read()
for name in sorted(os.listdir("import")):
    root = os.path.join("import", name)
    _, front, text = open(os.path.join(root, "SKILL.md")).read().split("---\n", 2)
    description = [l.split(":", 1)[1].strip() for l in front.splitlines() if l.startswith("description:")][0]
    put("/v1/skills/" + name, {"description": description, "text": text})
    for directory, _, files in os.walk(root):
        for file in files:
            path = os.path.relpath(os.path.join(directory, file), root)
            if path != "SKILL.md":
                put("/v1/skills/" + name + "/files/" + path, {"text": open(os.path.join(directory, file)).read()})
print("imported")
"#;

/// The zip a person would choose: the skill `pdf-forms` with a script.
fn skills_zip(dir: &Path) -> std::path::PathBuf {
    let source = dir.join("source/pdf-forms");
    std::fs::create_dir_all(source.join("scripts")).unwrap();
    std::fs::write(
        source.join("SKILL.md"),
        "---\nname: pdf-forms\ndescription: Fill PDF forms.\n---\nRun scripts/fill.py.\n",
    )
    .unwrap();
    std::fs::write(source.join("scripts/fill.py"), "print('filled')\n").unwrap();
    let zip = dir.join("skills.zip");
    let made = std::process::Command::new("python3")
        .args(["-m", "zipfile", "-c"])
        .arg(&zip)
        .arg(&source)
        .status()
        .unwrap();
    assert!(made.success());
    zip
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chosen_zip_is_sent_and_the_agent_imports_the_skill_with_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-core/testdata/fake-pi");
    let more = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\n",
        fake_pi.display().to_string(),
        origin.display().to_string()
    );
    let (settings, server) = start_with(dir.path(), &more).await;
    let client = Client::new(settings.clone());
    let zip = skills_zip(dir.path());
    let agent = dir.path().join("agent.py");
    std::fs::write(&agent, AGENT).unwrap();

    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    let task = app.update(Message::Select(Page::Skills));
    settle(&mut app, task).await;
    let import = |message| Message::Skills(skills::Message::Import(message));
    let task = app.update(import(import::Message::Show(true)));
    settle(&mut app, task).await;

    // The file dialog answers with the zip; the app sends it, and the server keeps it.
    let task = app.update(import(import::Message::Chosen(Some(zip.clone()))));
    settle(&mut app, task).await;
    assert!(shown(&app, "skills.zip is on the server."));
    let uploads: Vec<_> = std::fs::read_dir(server.app.data_dir.join("uploads"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(uploads.len(), 1);
    assert_eq!(
        std::fs::read(&uploads[0]).unwrap(),
        std::fs::read(&zip).unwrap()
    );
    let upload = uploads[0].display().to_string();

    // The note makes the fake Pi do what the agent would; "Ask the agent" starts the chat.
    let note = format!("[run python3 {} {upload}]", agent.display());
    let task = app.update(import(import::Message::NoteEdited(note)));
    settle(&mut app, task).await;
    let task = app.update(import(import::Message::Ask));
    settle(&mut app, task).await;
    let chat = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let chats = client.chats().await.unwrap();
            if chats.first().is_some_and(|chat| chat.status == "waiting") {
                break chats[0].clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the agent did not answer in a minute");
    assert_eq!(chat.agent, "Moebius");
    let prompt = std::fs::read_to_string(
        server
            .app
            .data_dir
            .join(format!("runs/{}/agent/prompt.txt", chat.id)),
    )
    .unwrap();
    assert!(
        prompt.contains(&format!("unzip -q {upload} -d import")),
        "{prompt}"
    );

    // The skill and its script are commits on the origin, made by the agent.
    let skill = client.skill("pdf-forms").await.unwrap();
    assert_eq!(skill.description, "Fill PDF forms.");
    assert_eq!(skill.text, "Run scripts/fill.py.\n");
    assert_eq!(skill.files, ["scripts/fill.py"]);
    assert_eq!(
        git(&origin, &["show", "main:skills/pdf-forms/scripts/fill.py"]).unwrap(),
        "print('filled')"
    );
    assert_eq!(
        git(&origin, &["log", "-2", "--format=%s", "main"]).unwrap(),
        "Add scripts/fill.py of the skill pdf-forms\nAdd the skill pdf-forms"
    );
    let decisions = server.app.store.judgements("edit_skill").unwrap();
    assert_eq!(decisions.len(), 2);
}
