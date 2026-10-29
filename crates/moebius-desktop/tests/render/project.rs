//! A project's Files and Editor tabs: the tree of the default branch, files open as tabs, saving with Ctrl+S, and
//! what the agent is told of the file and the cursor.

use moebius_desktop::app::{App, Message};
use moebius_desktop::client::{self, ClientError};
use moebius_desktop::screens::project;
use serde_json::json;

use crate::board::HOLD;
use crate::fixtures::{clicked, draw, radiance_app, shows, simulator};

/// A directory of the radiance repository as the server lists it.
fn tree(path: &str, entries: &[(&str, &str, u64)]) -> client::Tree {
    let entries: Vec<_> = entries
        .iter()
        .map(|(name, kind, size)| {
            let full = match path {
                "" => name.to_string(),
                dir => format!("{dir}/{name}"),
            };
            json!({"name": name, "path": full, "kind": kind, "size": size})
        })
        .collect();
    serde_json::from_value(json!({"path": path, "entries": entries})).unwrap()
}

fn text_file(path: &str, text: &str) -> client::FileDetail {
    serde_json::from_value(json!({"path": path, "size": text.len(), "text": text})).unwrap()
}

const MAIN_RS: &str = "//! The radiance server.\n\nuse std::net::TcpListener;\n\n/// Serves until the listener closes.\nfn main() {\n    let listener = TcpListener::bind(\"127.0.0.1:0\").unwrap();\n    for stream in listener.incoming() {\n        println!(\"{:?}\", stream);\n    }\n}\n";

pub fn project(message: project::Message) -> Message {
    Message::Project(1, message)
}

/// The Files tab of the radiance project with `src` open.
fn files_app() -> App {
    let mut app = radiance_app();
    let _ = app.update(project(project::Message::Tab(project::Tab::Files)));
    let _ = app.update(project(project::Message::Listed(
        String::new(),
        Ok(tree(
            "",
            &[
                (".moebius", "dir", 0),
                ("src", "dir", 0),
                ("Cargo.toml", "file", 412),
                ("README.md", "file", 2_310),
                ("logo.png", "file", 48_211),
            ],
        )),
    )));
    let _ = app.update(project(project::Message::Toggle("src".into())));
    let _ = app.update(project(project::Message::Listed(
        "src".into(),
        Ok(tree(
            "src",
            &[
                ("draft", "dir", 0),
                ("main.rs", "file", 214),
                ("sched.rs", "file", 9_870),
            ],
        )),
    )));
    app
}

/// The Editor of the radiance project with `main.rs` (changed, unsaved) and the Ready column's script open.
fn editor_app() -> App {
    use iced::widget::text_editor::{Action, Edit};
    let mut app = files_app();
    for (path, text) in [(".moebius/ready.luau", HOLD), ("src/main.rs", MAIN_RS)] {
        let _ = app.update(project(project::Message::Open(path.into())));
        let _ = app.update(project(project::Message::Read(
            path.into(),
            Ok(text_file(path, text)),
        )));
    }
    let _ = app.update(project(project::Message::Edit(Action::Edit(Edit::Insert(
        '\n',
    )))));
    app
}

#[test]
fn a_projects_tabs_lead_from_its_board_to_its_files() {
    let app = radiance_app();
    assert!(shows(&app, "Board"));
    let mut ui = simulator(&app);
    ui.click(iced::widget::Id::from("project-tab-Files"))
        .unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        matches!(
            &messages[..],
            [Message::Project(
                1,
                project::Message::Tab(project::Tab::Files)
            )]
        ),
        "{messages:?}"
    );
    let mut app = app;
    let _ = app.update(messages.into_iter().next().unwrap());
    assert!(shows(&app, "Reading the default branch."));
    assert!(!shows(&app, "New issue"));
}

#[test]
fn the_files_tab_shows_the_tree_with_an_open_directory() {
    let app = files_app();
    draw(
        &app,
        "project-files",
        &[
            "Projects",
            "radiance",
            "Board",
            "Files",
            "Editor",
            ".moebius",
            "src",
            "draft",
            "main.rs",
            "sched.rs",
            "Cargo.toml",
            "README.md",
            "48.2 KB",
        ],
    );
    let mut ui = simulator(&app);
    ui.click(iced::widget::Id::from("tree-src/main.rs"))
        .unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        matches!(&messages[..], [Message::Project(1, project::Message::Open(path))] if path == "src/main.rs"),
        "{messages:?}"
    );
    let mut ui = simulator(&app);
    ui.click(iced::widget::Id::from("tree-.moebius")).unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        matches!(&messages[..], [Message::Project(1, project::Message::Toggle(path))] if path == ".moebius"),
        "{messages:?}"
    );

    // A directory opened for the first time is read; a second click closes it.
    let mut app = app;
    let _ = app.update(project(project::Message::Toggle(".moebius".into())));
    assert!(shows(&app, "Reading."));
    let _ = app.update(project(project::Message::Listed(
        ".moebius".into(),
        Ok(tree(".moebius", &[("ready.luau", "file", 180)])),
    )));
    assert!(shows(&app, "ready.luau"));
    let _ = app.update(project(project::Message::Toggle(".moebius".into())));
    assert!(!shows(&app, "ready.luau"));
    let _ = app.update(project(project::Message::Toggle("src".into())));
    assert!(!shows(&app, "sched.rs"));
}

#[test]
fn the_editor_shows_open_files_as_tabs_highlighted_with_the_unsaved_one_marked() {
    let app = editor_app();
    draw(
        &app,
        "project-editor",
        &[
            "Editor · 2",
            "ready.luau",
            "main.rs",
            "src/main.rs",
            "Rust",
            "Ln 2, Col 1",
            "Unsaved changes.",
            "Save",
        ],
    );
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Project(1, project::Message::Save)]
    ));
}

#[test]
fn saving_clears_the_mark_and_a_refusal_is_shown() {
    let mut app = editor_app();
    let _ = app.update(project(project::Message::Save));
    assert!(shows(&app, "Saving."));
    let mut edited = MAIN_RS.to_string();
    edited.insert(0, '\n');
    let refusal = ClientError::Server {
        status: 409,
        message: "git push --quiet origin main failed: protected branch".into(),
    };
    let _ = app.update(project(project::Message::Saved(
        "src/main.rs".into(),
        edited.clone(),
        Err(refusal),
    )));
    assert!(
        shows(
            &app,
            "The server refused: git push --quiet origin main failed: protected branch"
        ) || shows(
            &app,
            "git push --quiet origin main failed: protected branch"
        )
    );
    assert!(app.page_context().contains("(unsaved changes)"));

    let _ = app.update(project(project::Message::Save));
    let _ = app.update(project(project::Message::Saved(
        "src/main.rs".into(),
        edited.clone(),
        Ok(text_file("src/main.rs", &edited)),
    )));
    assert!(shows(&app, "Committed to the default branch."));
    assert!(!app.page_context().contains("(unsaved changes)"));
    assert!(!shows(&app, "Unsaved changes."));
}

#[test]
fn ctrl_s_saves_the_file_the_editor_shows() {
    use iced::keyboard::{self, Key, Modifiers};
    let press = |modifiers| {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Character("s".into()),
            modified_key: Key::Character("s".into()),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyS),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    };
    let key = |event| {
        project::key(
            event,
            iced::event::Status::Ignored,
            iced::window::Id::unique(),
        )
    };
    assert!(matches!(
        key(press(Modifiers::COMMAND)),
        Some(Message::SaveFile)
    ));
    assert!(key(press(Modifiers::empty())).is_none());

    let mut app = editor_app();
    let _ = app.update(Message::SaveFile);
    assert!(shows(&app, "Saving."));
    // On the board it does nothing.
    let mut board = editor_app();
    let _ = board.update(project(project::Message::Tab(project::Tab::Board)));
    let _ = board.update(Message::SaveFile);
    let _ = board.update(project(project::Message::Tab(project::Tab::Editor)));
    assert!(shows(&board, "Unsaved changes."));
}

#[test]
fn closing_an_unsaved_file_asks_and_a_saved_one_closes_at_once() {
    let app = editor_app();
    let mut ui = simulator(&app);
    ui.click(iced::widget::Id::from("close-src/main.rs"))
        .unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        matches!(&messages[..], [Message::Project(1, project::Message::Close(path))] if path == "src/main.rs"),
        "{messages:?}"
    );
    let mut app = app;
    let _ = app.update(project(project::Message::Close("src/main.rs".into())));
    assert!(shows(&app, "Close main.rs without saving?"));
    let _ = app.update(project(project::Message::Keep));
    assert!(!shows(&app, "Close main.rs without saving?"));
    let _ = app.update(project(project::Message::Close("src/main.rs".into())));
    let _ = app.update(project(project::Message::Discard("src/main.rs".into())));
    assert!(!shows(&app, "main.rs"));
    assert!(shows(&app, "ready.luau"));
    // The script has no changes and closes at once, leaving no file open.
    let _ = app.update(project(project::Message::Close(
        ".moebius/ready.luau".into(),
    )));
    assert!(shows(&app, "No file open."));
}

#[test]
fn a_binary_or_too_large_file_is_said_to_be_so_and_not_edited() {
    let mut app = files_app();
    for (path, detail) in [
        (
            "logo.png",
            json!({"path": "logo.png", "size": 48211, "binary": true}),
        ),
        (
            "data.bin",
            json!({"path": "data.bin", "size": 2000000, "too_large": true}),
        ),
    ] {
        let _ = app.update(project(project::Message::Open(path.into())));
        let _ = app.update(project(project::Message::Read(
            path.into(),
            Ok(serde_json::from_value(detail).unwrap()),
        )));
    }
    assert!(shows(
        &app,
        "data.bin is over 1 MB (2000000 bytes), not editable here."
    ));
    let _ = app.update(project(project::Message::Show("logo.png".into())));
    assert!(shows(
        &app,
        "logo.png is binary (48211 bytes), not editable here."
    ));
    assert!(app.page_context().contains("It is a binary file"));
}

/// What the agent is told in a project: the directories open in Files, and in the Editor the file, the cursor, the
/// selection and the lines around the cursor.
#[test]
fn the_page_context_names_the_file_the_cursor_and_the_selection() {
    use iced::widget::text_editor::{Action, Motion};
    let files = files_app().page_context();
    assert!(files.starts_with("Page: Projects > Files"), "{files}");
    assert!(files.contains("Repository: radiance (id 1)"), "{files}");
    assert!(files.contains("open directories: src"), "{files}");

    let mut app = editor_app();
    for _ in 0..5 {
        let _ = app.update(project(project::Message::Edit(Action::Move(Motion::Down))));
    }
    for _ in 0..3 {
        let _ = app.update(project(project::Message::Edit(Action::Move(Motion::Right))));
    }
    for _ in 0..4 {
        let _ = app.update(project(project::Message::Edit(Action::Select(
            Motion::Right,
        ))));
    }
    let context = app.page_context();
    assert!(context.starts_with("Page: Projects > Editor"), "{context}");
    assert!(context.contains("Repository: radiance (id 1)"), "{context}");
    assert!(
        context.contains("Open file: src/main.rs (unsaved changes)"),
        "{context}"
    );
    assert!(
        context.contains("Also open: .moebius/ready.luau"),
        "{context}"
    );
    assert!(context.contains("Cursor: line 7, column 8"), "{context}");
    assert!(context.contains("Selected text:\nmain"), "{context}");
    assert!(context.contains("    7 | fn main() {"), "{context}");

    // A long file is cut to the lines around the cursor, and a long line is cut too.
    let mut app = files_app();
    let long: String = (1..=500)
        .map(|n| format!("line {n} {}\n", "x".repeat(120)))
        .collect();
    let _ = app.update(project(project::Message::Open("long.txt".into())));
    let _ = app.update(project(project::Message::Read(
        "long.txt".into(),
        Ok(text_file("long.txt", &long)),
    )));
    for _ in 0..250 {
        let _ = app.update(project(project::Message::Edit(Action::Move(Motion::Down))));
    }
    let context = app.page_context();
    assert!(context.contains("Cursor: line 251, column 1"), "{context}");
    assert!(context.contains("  251 | line 251"), "{context}");
    assert!(!context.contains("line 200 "), "{context}");
    assert!(context.contains("[cut: the first"), "{context}");
    assert!(
        context.chars().count() < 3_600,
        "{}",
        context.chars().count()
    );
}

const PATHS: [&str; 9] = [
    ".moebius/ready.luau",
    "Cargo.toml",
    "README.md",
    "docs/domain.md",
    "src/draft/main_old.rs",
    "src/main.rs",
    "src/sched.rs",
    "src/server/main.rs",
    "tests/remain.rs",
];

fn find(message: project::find::Message) -> Message {
    Message::Find(message)
}

/// The Files tab with "Go to file" open and `mr` typed.
fn go_app() -> App {
    use project::find::Message::{Go, Paths, Query};
    let mut app = files_app();
    let _ = app.update(find(Go));
    let paths = client::PathList {
        paths: PATHS.iter().map(|p| p.to_string()).collect(),
    };
    let _ = app.update(find(Paths(Ok(paths))));
    let _ = app.update(find(Query("mr".into())));
    app
}

fn hit(line: u64, text: &str) -> client::SearchHit {
    client::SearchHit {
        line,
        text: text.into(),
    }
}

/// The Editor tab with "Search in files" open and the hits of `main` listed.
fn search_app() -> App {
    use project::find::Message::{Found, Query, Search, Submit};
    let mut app = editor_app();
    let _ = app.update(find(Search));
    let _ = app.update(find(Query("main".into())));
    let _ = app.update(find(Submit));
    let found = client::SearchResult {
        files: vec![
            client::SearchFile {
                path: "src/main.rs".into(),
                hits: vec![
                    hit(6, "fn main() {"),
                    hit(9, "        println!(\"{:?}\", stream);"),
                ],
            },
            client::SearchFile {
                path: "src/server/main.rs".into(),
                hits: vec![hit(3, "mod main_loop;")],
            },
        ],
        total: 3,
        truncated: false,
    };
    let _ = app.update(find(Found(1, Ok(found))));
    app
}

#[test]
fn go_to_file_lists_the_best_matches_first_and_enter_opens_the_first() {
    let app = go_app();
    draw(
        &app,
        "project-go-to-file",
        &["Go to file", "mr", "main.rs", "src/server", "remain.rs"],
    );
    let mut ui = simulator(&app);
    ui.click(iced::widget::Id::from("go-src/server/main.rs"))
        .unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        matches!(&messages[..], [Message::Project(1, project::Message::Find(project::find::Message::Pick(i)))] if *i < 3),
        "{messages:?}"
    );

    let mut app = app;
    let _ = app.update(find(project::find::Message::Submit));
    // The panel closes and the file opens in the Editor; the agent is told where it came from.
    assert!(!shows(&app, "Go to file"));
    let context = app.page_context();
    assert!(context.starts_with("Page: Projects > Editor"), "{context}");
    assert!(context.contains("Open file: "), "{context}");
    assert!(
        context.contains("came from: Go to file, typed \"mr\""),
        "{context}"
    );
}

#[test]
fn the_agent_is_told_of_the_open_panel() {
    let context = go_app().page_context();
    assert!(
        context.contains("\"Go to file\" is open; typed: \"mr\""),
        "{context}"
    );
    assert!(context.contains("Best matches: "), "{context}");
    let context = search_app().page_context();
    assert!(
        context.contains("\"Search in files\" is open; typed: \"main\""),
        "{context}"
    );
    assert!(context.contains("found 3 hits in 2 files"), "{context}");
    assert!(context.contains("src/main.rs:6: fn main() {"), "{context}");
}

#[test]
fn search_in_files_lists_the_hits_per_file_and_a_click_opens_the_file_at_the_line() {
    let app = search_app();
    draw(
        &app,
        "project-search",
        &[
            "Search in files",
            "main",
            "Regex",
            "src/main.rs",
            "src/server/main.rs",
            "fn main() {",
            "mod main_loop;",
            "3 hits in 2 files",
        ],
    );
    let mut ui = simulator(&app);
    ui.click(iced::widget::Id::from("hit-src/main.rs:6"))
        .unwrap();
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        matches!(
            &messages[..],
            [Message::Project(
                1,
                project::Message::Find(project::find::Message::Pick(0))
            )]
        ),
        "{messages:?}"
    );

    let mut app = app;
    for message in messages {
        let _ = app.update(message);
    }
    // main.rs was open already; the cursor is on the line of the hit and the agent is told so.
    let context = app.page_context();
    assert!(context.contains("Open file: src/main.rs"), "{context}");
    assert!(context.contains("Cursor: line 6, column 1"), "{context}");
    assert!(
        context.contains("came from: Search in files for \"main\", hit at line 6"),
        "{context}"
    );

    // A hit in a file that is not open yet opens it and goes to the line once it is read.
    let _ = app.update(find(project::find::Message::Pick(2)));
    let _ = app.update(project(project::Message::Read(
        "src/server/main.rs".into(),
        Ok(text_file(
            "src/server/main.rs",
            "use a;\nuse b;\nmod main_loop;\n",
        )),
    )));
    let context = app.page_context();
    assert!(
        context.contains("Open file: src/server/main.rs"),
        "{context}"
    );
    assert!(context.contains("Cursor: line 3, column 1"), "{context}");
}

#[test]
fn enter_searches_and_then_opens_the_selected_hit_and_a_failure_is_shown() {
    use project::find::Message::{Found, Move, Query, Regex, Search, Submit};
    let mut app = files_app();
    let _ = app.update(find(Search));
    let _ = app.update(find(Regex(true)));
    let _ = app.update(find(Query("(".into())));
    let _ = app.update(find(Submit));
    assert!(shows(&app, "Searching."));
    let refusal = ClientError::Server {
        status: 422,
        message: "fatal: unmatched parenthesis".into(),
    };
    // An answer to an older search is dropped.
    let _ = app.update(find(Found(
        7,
        Ok(client::SearchResult {
            files: vec![],
            total: 0,
            truncated: false,
        }),
    )));
    assert!(shows(&app, "Searching."));
    let _ = app.update(find(Found(1, Err(refusal))));
    assert!(shows(&app, "fatal: unmatched parenthesis"));

    let mut app = search_app();
    let _ = app.update(find(Move(moebius_desktop::search::Direction::Down)));
    let _ = app.update(find(Move(moebius_desktop::search::Direction::Down)));
    let _ = app.update(find(Submit));
    assert!(app.page_context().contains("Open file: src/server/main.rs"));
}

#[test]
fn ctrl_p_and_ctrl_shift_f_open_the_panels() {
    use iced::keyboard::{self, Key, Modifiers};
    let press = |character: &str, modifiers| {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Character(character.into()),
            modified_key: Key::Character(character.into()),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyP),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    };
    let key = |event| {
        project::find_key(
            event,
            iced::event::Status::Ignored,
            iced::window::Id::unique(),
        )
    };
    assert!(matches!(
        key(press("p", Modifiers::COMMAND)),
        Some(Message::Find(project::find::Message::Go))
    ));
    assert!(key(press("p", Modifiers::COMMAND | Modifiers::SHIFT)).is_none());
    assert!(matches!(
        key(press("F", Modifiers::COMMAND | Modifiers::SHIFT)),
        Some(Message::Find(project::find::Message::Search))
    ));
    assert!(key(press("f", Modifiers::COMMAND)).is_none());
    assert!(key(press("p", Modifiers::empty())).is_none());

    // On the board the keys show the Files tab with the panel.
    let mut app = radiance_app();
    let _ = app.update(Message::Find(project::find::Message::Search));
    assert!(shows(&app, "Search in files"));
    assert!(shows(&app, "Cargo.toml") || shows(&app, "Reading the default branch."));
    let _ = app.update(Message::Find(project::find::Message::Close));
    assert!(!shows(&app, "Search in files"));
}
