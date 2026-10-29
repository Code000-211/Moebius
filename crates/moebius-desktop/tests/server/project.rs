//! A project against a real server: a repository added and an issue created, the script editor and the bubble's
//! floating chat, and files listed, read and saved as commits.

use std::sync::Arc;

use iced_test::Simulator;
use moebius_core::repos::git;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{Client, ClientError, NewRepository, Snapshot};

use crate::fixtures::{origin, settle, shown, start, start_with_fake_pi};

#[tokio::test(flavor = "multi_thread")]
async fn the_client_adds_a_repository_from_a_bare_clone_and_creates_an_issue_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start(dir.path()).await;
    let bare = dir.path().join("bare.git");
    git(
        dir.path(),
        &[
            "clone",
            "--quiet",
            "--bare",
            origin(dir.path()).to_str().unwrap(),
            "bare.git",
        ],
    )
    .unwrap();
    let client = Client::new(settings);
    let request = NewRepository {
        name: "demo".into(),
        url: bare.display().to_string(),
        forge: "none",
        github: None,
        trusted: Vec::new(),
    };
    let repository = client.add_repository(&request).await.unwrap();
    assert_eq!(
        (repository.name.as_str(), repository.forge.as_str()),
        ("demo", "none")
    );
    let again = client.add_repository(&request).await.unwrap_err();
    assert!(matches!(again, ClientError::Server { status: 422, .. }));

    let card = client
        .create_issue(repository.id, "Add a cache", "Keep the files.")
        .await
        .unwrap();
    assert_eq!((card.number, card.column.as_str()), (1, "Backlog"));
    let snapshot = client.snapshot().await.unwrap();
    let board = snapshot
        .boards
        .iter()
        .find(|board| board.repository.id == repository.id)
        .unwrap();
    assert!(
        board
            .cards
            .iter()
            .any(|c| c.title == "Add a cache" && c.body == "Keep the files.")
    );
    let blank = client
        .create_issue(repository.id, " ", "")
        .await
        .unwrap_err();
    assert!(matches!(blank, ClientError::Server { status: 422, .. }));
}

/// The pencil's editor against a real server: it reads the built-in script, saves a script as a commit, shows the
/// server's refusal of a bad one, and gives the script back. The speech bubble starts a draft that is not sent until
/// "Send" is pressed, and the chat then works in the repository.
#[tokio::test(flavor = "multi_thread")]
async fn the_script_editor_reads_saves_refuses_and_gives_back_and_the_bubble_asks_the_floating_chat()
 {
    use iced::widget::text_editor::{Action, Edit};
    use moebius_desktop::widgets::script_editor;

    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with_fake_pi(dir.path()).await;
    let client = Client::new(settings.clone());
    let repository = client
        .add_repository(&NewRepository {
            name: "demo".into(),
            url: origin(dir.path()).display().to_string(),
            forge: "none",
            github: None,
            trusted: Vec::new(),
        })
        .await
        .unwrap();
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    let clone = std::path::PathBuf::from(&repository.path);
    let edit = |app: &mut App, text: &str| {
        let _ = app.update(Message::Script(script_editor::Message::Text(
            Action::SelectAll,
        )));
        let paste = Edit::Paste(Arc::new(text.to_string()));
        let _ = app.update(Message::Script(script_editor::Message::Text(Action::Edit(
            paste,
        ))));
    };

    // The pencil of the Ready column reads the built-in script.
    let mut ui = Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1440.0, 900.0),
        app.view(),
    );
    ui.click(iced::widget::Id::from("edit-script-Ready"))
        .unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    let task = app.update(messages.into_iter().next().unwrap());
    settle(&mut app, task).await;
    assert!(shown(&app, ".moebius/ready.luau"));
    assert!(shown(&app, "Built in"));

    // A script that fails the check is refused with the server's message and nothing is committed.
    edit(&mut app, "this is not luau");
    let task = app.update(Message::Script(script_editor::Message::Save));
    settle(&mut app, task).await;
    let refusal = client
        .save_script(repository.id, "Ready", "this is not luau")
        .await
        .unwrap_err();
    let ClientError::Server {
        status: 422,
        message,
    } = refusal
    else {
        panic!("{refusal:?}")
    };
    assert!(shown(&app, &message), "{message}");
    assert!(!clone.join(".moebius").exists());

    // A good one is committed and the editor says the script is the repository's.
    edit(&mut app, "function on_card(card) moebius.wait('held') end");
    let task = app.update(Message::Script(script_editor::Message::Save));
    settle(&mut app, task).await;
    assert!(shown(&app, "Repository"));
    assert!(shown(&app, "Saved as a commit on the default branch."));
    assert_eq!(
        std::fs::read_to_string(clone.join(".moebius/ready.luau")).unwrap(),
        "function on_card(card) moebius.wait('held') end\n"
    );
    assert_eq!(
        client.script(repository.id, "Ready").await.unwrap().source,
        "repository"
    );

    // The bubble opens the floating chat over the editor with a message begun; nothing is sent until the person sends
    // it, which starts a chat in this repository.
    let _ = app.update(Message::Script(script_editor::Message::AskAgent));
    assert_ne!(app.page(), Page::Chat);
    assert!(client.chats().await.unwrap().is_empty());
    let mut ui = Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1440.0, 900.0),
        app.view(),
    );
    ui.click("Send").unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    let task = app.update(messages.into_iter().next().unwrap());
    settle(&mut app, task).await;
    let chats = client.chats().await.unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(
        (chats[0].agent.as_str(), chats[0].repository_id),
        ("Moebius", Some(repository.id))
    );
    assert_eq!(
        chats[0].title,
        "About the Ready column's script (.moebius/ready.luau):"
    );
    // The agent was told where the person is (the fake Pi answers with its prompt); the chat shows only the message.
    let answer = loop {
        let events = client.run_events(chats[0].id, 0).await.unwrap();
        let answered: String = events
            .iter()
            .filter(|event| event.kind == "message_update")
            .filter_map(|event| event.data["assistantMessageEvent"]["delta"].as_str())
            .collect();
        if events.iter().any(|event| event.kind == "agent_end") {
            let said: Vec<_> = events.iter().filter(|event| event.kind == "said").collect();
            assert_eq!(said.len(), 1);
            assert_eq!(
                said[0].data["message"],
                "About the Ready column's script (.moebius/ready.luau):"
            );
            break answered;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    assert!(
        answer.contains("[Where the person is in the Moebius app.")
            && answer.contains("Editing the script of the Ready column (.moebius/ready.luau)")
            && answer.contains("moebius.wait('held')")
            && answer.ends_with("About the Ready column's script (.moebius/ready.luau):"),
        "{answer}"
    );

    // Back in the editor, the built-in script is used again.
    let task = app.update(Message::EditScript(repository.id, "Ready".into()));
    settle(&mut app, task).await;
    let task = app.update(Message::Script(script_editor::Message::UseBuiltIn));
    settle(&mut app, task).await;
    assert!(!clone.join(".moebius/ready.luau").exists());
    assert_eq!(
        client.script(repository.id, "Ready").await.unwrap().source,
        "built-in"
    );
}

/// The Files and Editor tabs against a real server: the tree of the default branch, a file read into the Editor, and
/// a save that is a commit in the server's clone, after which the file is no longer marked unsaved.
#[tokio::test(flavor = "multi_thread")]
async fn the_project_lists_reads_and_saves_a_file_as_a_commit() {
    use iced::widget::text_editor::{Action, Edit};
    use moebius_desktop::screens::project;

    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start(dir.path()).await;
    let source = origin(dir.path());
    std::fs::create_dir_all(source.join("src")).unwrap();
    std::fs::write(source.join("src/main.rs"), "fn main() {}\n").unwrap();
    git(&source, &["add", "."]).unwrap();
    git(&source, &["commit", "--quiet", "-m", "code"]).unwrap();
    let client = Client::new(settings.clone());
    let repository = client
        .add_repository(&NewRepository {
            name: "demo".into(),
            url: source.display().to_string(),
            forge: "none",
            github: None,
            trusted: Vec::new(),
        })
        .await
        .unwrap();
    let id = repository.id;
    let clone = std::path::PathBuf::from(&repository.path);

    let root = client.tree(id, "").await.unwrap();
    let names: Vec<(&str, &str)> = root
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.kind.as_str()))
        .collect();
    assert_eq!(names, [("src", "dir"), ("README.md", "file")]);
    let src = client.tree(id, "src").await.unwrap();
    assert_eq!(src.entries[0].path, "src/main.rs");
    assert_eq!(src.entries[0].size, 13);

    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    let task = app.update(Message::Project(
        id,
        project::Message::Tab(project::Tab::Files),
    ));
    settle(&mut app, task).await;
    assert!(shown(&app, "README.md"));
    let task = app.update(Message::Project(id, project::Message::Toggle("src".into())));
    settle(&mut app, task).await;
    assert!(shown(&app, "main.rs"));
    let task = app.update(Message::Project(
        id,
        project::Message::Open("src/main.rs".into()),
    ));
    settle(&mut app, task).await;
    assert!(app.page_context().contains("Open file: src/main.rs"));
    assert!(app.page_context().contains("    1 | fn main() {}"));

    let _ = app.update(Message::Project(
        id,
        project::Message::Edit(Action::Edit(Edit::Paste(Arc::new("// hi\n".into())))),
    ));
    assert!(shown(&app, "Unsaved changes."));
    let task = app.update(Message::SaveFile);
    settle(&mut app, task).await;
    assert!(shown(&app, "Saved as a commit on the default branch."));
    assert!(!shown(&app, "Unsaved changes."));
    assert_eq!(
        std::fs::read_to_string(clone.join("src/main.rs")).unwrap(),
        "// hi\nfn main() {}\n"
    );
    assert_eq!(
        git(&clone, &["log", "-1", "--format=%s"]).unwrap(),
        "Update src/main.rs"
    );
    let read = client.file(id, "src/main.rs").await.unwrap();
    assert_eq!(read.text.as_deref(), Some("// hi\nfn main() {}\n"));
    assert!(client.file(id, "nowhere.rs").await.is_err());
}
