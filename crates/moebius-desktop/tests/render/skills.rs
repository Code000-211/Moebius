//! The Skills and MCP servers pages: a skill or server listed and edited, a skill's files listed and edited, the
//! built-in skill, setting up a harness repository, and the panel that imports a skill from a zip or a link.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, ClientError};
use moebius_desktop::screens::harness_setup;
use moebius_desktop::screens::mcp_servers;
use moebius_desktop::screens::skills;
use moebius_desktop::widgets::import;
use serde_json::json;

use crate::agents::{mcp_listing, skill_listing};
use crate::fixtures::{click, clicked, draw_sized, overview, settings, shows, simulator};
use crate::new_chat::moebius_chat;

fn house_style() -> client::SkillDetail {
    serde_json::from_value(json!({
        "name": "house-style", "description": "Write plainly.",
        "text": "# House style\n\nShort sentences. No jargon.\n",
        "files": ["examples/good.md", "examples/bad.md"]
    }))
    .unwrap()
}

/// The Skills page of a server with a harness repository, with the skill `house-style` open.
fn skills_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Skills));
    let skills = |message| Message::Skills(message);
    let _ = app.update(skills(skills::Message::Listed(Ok(skill_listing()))));
    let _ = app.update(skills(skills::Message::Select("house-style".into())));
    let _ = app.update(skills(skills::Message::Opened(
        "house-style".into(),
        Ok(house_style()),
    )));
    app
}

#[test]
fn the_skills_page_lists_the_skills_and_edits_one() {
    draw_sized(
        &skills_app(),
        "skills",
        &[
            "Skills",
            "New skill",
            "moebius",
            "Built in",
            "Build on Moebius.",
            "radiance-kernels",
            "house-style",
            "Write plainly.",
            "Delete",
            "Save",
            "Description",
            "Files",
            "New file",
            "SKILL.md",
            "examples/good.md",
            "examples/bad.md",
        ],
        Size::new(1440.0, 900.0),
    );
}

fn file(message: skills::FileMessage) -> Message {
    Message::Skills(skills::Message::File(message))
}

fn good_example() -> client::FileDetail {
    serde_json::from_value(json!({
        "path": "examples/good.md", "size": 44,
        "text": "We ship on Fridays.\nThe tests must pass.\n"
    }))
    .unwrap()
}

#[test]
fn a_skills_other_file_opens_in_the_editor_beside_the_list() {
    let mut app = skills_app();
    assert!(matches!(
        &clicked(&app, "examples/good.md")[..],
        [Message::Skills(skills::Message::File(skills::FileMessage::Open(Some(path))))]
            if path == "examples/good.md"
    ));
    let _ = app.update(file(skills::FileMessage::Open(Some(
        "examples/good.md".into(),
    ))));
    assert!(shows(&app, "Reading the file…"));
    let _ = app.update(file(skills::FileMessage::Opened(
        "examples/good.md".into(),
        Ok(good_example()),
    )));
    draw_sized(
        &app,
        "skills-file",
        &[
            "Files",
            "SKILL.md",
            "examples/good.md",
            "examples/bad.md",
            "Delete file",
            "Save file",
        ],
        Size::new(1440.0, 900.0),
    );
    // SKILL.md is open again from the list, as it was.
    assert!(matches!(
        &clicked(&app, "SKILL.md")[..],
        [Message::Skills(skills::Message::File(
            skills::FileMessage::Open(None)
        ))]
    ));
    let _ = app.update(file(skills::FileMessage::Open(None)));
    assert!(shows(&app, "Markdown · the text after the frontmatter"));
    assert!(!shows(&app, "Save file"));
}

#[test]
fn a_file_that_is_not_text_is_listed_with_its_size_and_cannot_be_edited() {
    let mut app = skills_app();
    let _ = app.update(file(skills::FileMessage::Open(Some("logo.png".into()))));
    let binary =
        serde_json::from_value(json!({"path": "logo.png", "size": 4200, "binary": true})).unwrap();
    let _ = app.update(file(skills::FileMessage::Opened(
        "logo.png".into(),
        Ok(binary),
    )));
    assert!(shows(&app, "Not text, or too large to edit (4.2 KB)."));
    assert!(clicked(&app, "Save file").is_empty());
    assert!(matches!(
        &clicked(&app, "Delete file")[..],
        [Message::Skills(skills::Message::File(
            skills::FileMessage::AskDelete
        ))]
    ));
}

#[test]
fn saving_a_file_sends_its_path_and_text_and_says_the_commit() {
    let settings = settings();
    let mut skills = skills::Skills::default();
    let mut update = |message| skills.update(message, Some(&settings)).units();
    update(skills::Message::Select("house-style".into()));
    update(skills::Message::Opened(
        "house-style".into(),
        Ok(house_style()),
    ));
    assert_eq!(
        update(skills::Message::File(skills::FileMessage::Open(Some(
            "examples/good.md".into()
        )))),
        1,
        "the file is read"
    );
    update(skills::Message::File(skills::FileMessage::Opened(
        "examples/good.md".into(),
        Ok(good_example()),
    )));
    assert_eq!(
        skills.save_file_request().unwrap(),
        (
            "examples/good.md".to_string(),
            "We ship on Fridays.\nThe tests must pass.\n".to_string()
        )
    );
    let mut update = |message| skills.update(message, Some(&settings)).units();
    assert_eq!(update(skills::Message::File(skills::FileMessage::Save)), 1);

    // A new file is named first; a path that leaves the skill, or SKILL.md, is said instead of sent.
    let _ = skills.update(
        skills::Message::File(skills::FileMessage::NewFile),
        Some(&settings),
    );
    for (path, problem) in [
        ("", "Enter the file's path"),
        ("../agents/Coder.toml", "Enter the file's path"),
        ("SKILL.md", "SKILL.md is saved with the skill."),
    ] {
        let mut update = |message| skills.update(message, Some(&settings)).units();
        update(skills::Message::File(skills::FileMessage::PathEdited(
            path.into(),
        )));
        assert_eq!(update(skills::Message::File(skills::FileMessage::Save)), 0);
        assert!(skills.save_file_request().unwrap_err().starts_with(problem));
    }
    let mut update = |message| skills.update(message, Some(&settings)).units();
    update(skills::Message::File(skills::FileMessage::PathEdited(
        " scripts/fill.py ".into(),
    )));
    assert_eq!(update(skills::Message::File(skills::FileMessage::Save)), 1);
    assert_eq!(skills.save_file_request().unwrap().0, "scripts/fill.py");
}

#[test]
fn a_new_file_is_named_saved_as_a_commit_and_deleted_after_asking() {
    let mut app = skills_app();
    assert!(matches!(
        &clicked(&app, "New file")[..],
        [Message::Skills(skills::Message::File(
            skills::FileMessage::NewFile
        ))]
    ));
    let _ = app.update(file(skills::FileMessage::NewFile));
    assert!(shows(&app, "Path in the skill, such as scripts/fill.py"));
    assert!(!shows(&app, "Delete file"));
    let _ = app.update(file(skills::FileMessage::PathEdited(
        "scripts/fill.py".into(),
    )));
    let mut saved = house_style();
    saved.files.push("scripts/fill.py".into());
    saved.commit = Some("f11e5a1".into());
    let _ = app.update(file(skills::FileMessage::Saved(Ok(saved))));
    assert!(shows(&app, "Committed: f11e5a1"));
    assert!(shows(&app, "Delete file"));
    click(&mut app, "Delete file");
    assert!(shows(&app, "Delete scripts/fill.py?"));
    assert!(matches!(
        &clicked(&app, "Delete the file")[..],
        [Message::Skills(skills::Message::File(
            skills::FileMessage::Delete
        ))]
    ));
    let mut deleted = house_style();
    deleted.commit = Some("de1e7ed".into());
    let _ = app.update(file(skills::FileMessage::Deleted(Ok(deleted))));
    assert!(shows(&app, "Committed: de1e7ed"));
    assert!(!shows(&app, "scripts/fill.py"));
    assert!(shows(&app, "Markdown · the text after the frontmatter"));
}

#[test]
fn the_built_in_skill_is_shown_and_cannot_be_changed() {
    let mut app = skills_app();
    let built_in = client::SkillDetail {
        name: "moebius".into(),
        description: "Build on Moebius.".into(),
        text: "# Moebius\n".into(),
        files: vec!["guide/agents.md".into()],
        commit: None,
        source: None,
    };
    let _ = app.update(Message::Skills(skills::Message::Select("moebius".into())));
    let _ = app.update(Message::Skills(skills::Message::Opened(
        "moebius".into(),
        Ok(built_in),
    )));
    assert!(shows(&app, "Built in · read only"));
    assert!(shows(&app, "guide/agents.md"));
    let mut ui = simulator(&app);
    assert!(ui.find("Save").is_err() && ui.find("Delete").is_err());
    assert!(ui.find("New file").is_err());
    drop(ui);
    // A built-in skill's file is read and not changed.
    let _ = app.update(file(skills::FileMessage::Open(Some(
        "guide/agents.md".into(),
    ))));
    let page =
        serde_json::from_value(json!({"path": "guide/agents.md", "size": 9, "text": "# Agents\n"}))
            .unwrap();
    let _ = app.update(file(skills::FileMessage::Opened(
        "guide/agents.md".into(),
        Ok(page),
    )));
    assert!(shows(&app, "Read only"));
    assert!(!shows(&app, "Markdown · the text after the frontmatter"));
    let mut ui = simulator(&app);
    assert!(ui.find("Save file").is_err() && ui.find("Delete file").is_err());
}

/// A skill Moebius copied from Matt Pocock's skills, as the server lists and reads it.
const SOURCE: &str = r#"{"url": "https://github.com/mattpocock/skills", "commit": "59db609eb1e18aaf59b63284d0e52255aff0c63c"}"#;

#[test]
fn a_skill_copied_from_upstream_shows_its_source_and_can_be_updated_from_it() {
    let mut app = skills_app();
    let mut listing = skill_listing();
    listing.push(
        serde_json::from_value(json!({
            "name": "tdd", "description": "Test-driven development.", "built_in": true,
            "source": serde_json::from_str::<serde_json::Value>(SOURCE).unwrap()
        }))
        .unwrap(),
    );
    let _ = app.update(Message::Skills(skills::Message::Listed(Ok(listing))));
    let detail = serde_json::from_value(json!({
        "name": "tdd", "description": "Test-driven development.", "text": "# Test-Driven Development\n",
        "files": ["tests.md", "mocking.md", "LICENSE"],
        "source": serde_json::from_str::<serde_json::Value>(SOURCE).unwrap()
    }))
    .unwrap();
    let _ = app.update(Message::Skills(skills::Message::Select("tdd".into())));
    let _ = app.update(Message::Skills(skills::Message::Opened(
        "tdd".into(),
        Ok(detail),
    )));
    draw_sized(
        &app,
        "skills-upstream",
        &[
            "Built in · read only",
            "Update from upstream",
            "Copied from https://github.com/mattpocock/skills at commit 59db609eb1e1",
            "tests.md",
        ],
        Size::new(1440.0, 900.0),
    );
    assert!(matches!(
        &clicked(&app, "Update from upstream")[..],
        [Message::Skills(skills::Message::UpdateFromUpstream)]
    ));
    // The request is the skills import of the source, for this skill, by name and with the person's consent.
    let source = serde_json::from_str(SOURCE).unwrap();
    let asked = skills::update_request("tdd", &source);
    assert!(asked.contains("git clone --depth 1 https://github.com/mattpocock/skills import"));
    assert!(asked.contains("import only the skill tdd"));
    assert!(asked.contains("Replacing the built-in copy is what the person asked for"));
    assert!(asked.contains("commit 59db609eb1e18aaf59b63284d0e52255aff0c63c"));
    // A skill without a source has no such button.
    let _ = app.update(Message::Skills(skills::Message::Select("moebius".into())));
    assert!(!shows(&app, "Update from upstream"));
}

#[test]
fn a_server_without_a_harness_repository_offers_to_set_one_up_at_the_top() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Skills));
    let _ = app.update(Message::Skills(skills::Message::Listed(Ok(vec![]))));
    draw_sized(
        &app,
        "skills-no-harness",
        &[
            "Skills",
            harness_setup::NEEDED,
            "Set up",
            "Import",
            "New skill",
        ],
        Size::new(1440.0, 900.0),
    );
    // The buttons that need a repository do nothing, and "Set up" sets it up.
    assert!(clicked(&app, "New skill").is_empty());
    assert!(clicked(&app, "Import").is_empty());
    assert!(matches!(
        &clicked(&app, "Set up")[..],
        [Message::Harness(harness_setup::Message::SetUp)]
    ));

    let _ = app.update(Message::Select(Page::McpServers));
    // Before the server has said whether it has a repository, nothing is offered.
    assert!(!shows(&app, "Set up"));
    let _ = app.update(Message::Mcp(mcp_servers::Message::Harness(Ok(vec![]))));
    assert!(shows(&app, harness_setup::NEEDED));
    assert!(clicked(&app, "New server").is_empty());
    assert!(matches!(
        &clicked(&app, "Set up")[..],
        [Message::Harness(harness_setup::Message::SetUp)]
    ));

    // While the server sets up, and then restarts, the button says so and does nothing; a failure is told.
    let _ = app.update(Message::Harness(harness_setup::Message::Answered(Err(
        ClientError::Server {
            status: 409,
            message: "harness exists and is not empty".into(),
        },
    ))));
    assert!(shows(&app, "harness exists and is not empty"));
    let _ = app.update(Message::Harness(harness_setup::Message::Answered(Ok(
        client::SavedSettings {
            restart_needed: true,
            restart_keys: vec!["harness.url".into()],
        },
    ))));
    assert!(shows(&app, "Restarting…"));
    assert!(clicked(&app, "Restarting…").is_empty());
}

fn import(message: import::Message) -> Message {
    Message::Skills(skills::Message::Import(message))
}

#[test]
fn the_import_panel_takes_a_zip_or_a_link_and_a_note() {
    let mut app = skills_app();
    assert!(matches!(
        &clicked(&app, "Import")[..],
        [Message::Skills(skills::Message::Import(
            import::Message::Show(true)
        ))]
    ));
    let _ = app.update(import(import::Message::Show(true)));
    assert!(shows(&app, "No file chosen."));
    assert!(matches!(
        &clicked(&app, "Choose file…")[..],
        [Message::Skills(skills::Message::Import(
            import::Message::Choose
        ))]
    ));
    assert!(clicked(&app, "Ask the agent").is_empty(), "no source yet");
    let _ = app.update(import(import::Message::FromZip(false)));
    let _ = app.update(import(import::Message::LinkEdited(
        "https://github.com/acme/skills".into(),
    )));
    let _ = app.update(import(import::Message::NoteEdited(
        "only the pdf skill; it needs PDF_KEY".into(),
    )));
    draw_sized(
        &app,
        "skills-import",
        &[
            "Import",
            "From a zip",
            "From a link",
            "A git repository or a page",
            "Note (optional)",
            "Ask the agent",
            "Cancel",
        ],
        Size::new(1440.0, 900.0),
    );
    assert!(matches!(
        &clicked(&app, "Ask the agent")[..],
        [Message::Skills(skills::Message::Import(
            import::Message::Ask
        ))]
    ));
    // The chat the server started opens in the floating window, over the page, and the panel closes.
    let _ = app.update(import(import::Message::Asked(Ok(moebius_chat(9)))));
    assert_eq!(app.page(), Page::Skills);
    assert!(shows(&app, "Open in Chat"));
    assert!(!shows(&app, "From a link"));
}

#[test]
fn asking_to_import_from_a_link_starts_a_chat_that_names_the_link() {
    let settings = settings();
    let mut skills = skills::Skills::default();
    let mut update = |message| {
        skills
            .update(skills::Message::Import(message), Some(&settings))
            .units()
    };
    update(import::Message::Show(true));
    update(import::Message::FromZip(false));
    assert_eq!(update(import::Message::Ask), 0, "nothing to import yet");
    update(import::Message::LinkEdited(
        " https://github.com/acme/skills ".into(),
    ));
    assert_eq!(update(import::Message::Ask), 1);
    let asked = skills.import().request().unwrap();
    assert!(asked.contains("git clone --depth 1 https://github.com/acme/skills import"));
    assert!(asked.contains("PUT /v1/skills/<name>"));
}

#[test]
fn a_zip_is_sent_and_the_agent_is_given_its_path_on_the_server() {
    let settings = settings();
    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("skills.zip");
    std::fs::write(&zip, b"PK").unwrap();
    let mut skills = skills::Skills::default();
    let mut update = |message| {
        skills
            .update(skills::Message::Import(message), Some(&settings))
            .units()
    };
    update(import::Message::Show(true));
    assert_eq!(
        update(import::Message::Chosen(None)),
        0,
        "the dialog was closed"
    );
    assert_eq!(
        update(import::Message::Chosen(Some(zip))),
        1,
        "the file is sent"
    );
    assert!(skills.import().request().is_none(), "not on the server yet");
    let mut update = |message| {
        skills
            .update(skills::Message::Import(message), Some(&settings))
            .units()
    };
    update(import::Message::Uploaded(
        "skills.zip".into(),
        Ok(client::Upload {
            id: "ab12".into(),
            path: "/srv/moebius/uploads/ab12.zip".into(),
        }),
    ));
    assert_eq!(update(import::Message::Ask), 1);
    let asked = skills.import().request().unwrap();
    assert!(asked.contains("unzip -q /srv/moebius/uploads/ab12.zip -d import"));
}

#[test]
fn a_refused_upload_is_said_in_red_in_the_panel() {
    let mut app = skills_app();
    let _ = app.update(import(import::Message::Show(true)));
    let _ = app.update(import(import::Message::Chosen(Some(
        "/tmp/x/big.zip".into(),
    ))));
    assert!(shows(&app, "Sending big.zip…"));
    let _ = app.update(import(import::Message::Uploaded(
        "big.zip".into(),
        Err("The file is larger than the server takes.".into()),
    )));
    assert!(shows(&app, "The file is larger than the server takes."));
    assert!(shows(&app, "No file chosen."));
}

#[test]
fn import_is_disabled_without_a_harness_repository() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Skills));
    let _ = app.update(Message::Skills(skills::Message::Listed(Ok(vec![]))));
    assert!(clicked(&app, "Import").is_empty());
    assert!(shows(&app, harness_setup::NEEDED));
    let _ = app.update(Message::Select(Page::McpServers));
    let _ = app.update(Message::Mcp(mcp_servers::Message::Harness(Ok(vec![]))));
    assert!(clicked(&app, "Import").is_empty());
    let app = mcp_app();
    assert!(matches!(
        &clicked(&app, "Import")[..],
        [Message::Mcp(mcp_servers::Message::Import(
            import::Message::Show(true)
        ))]
    ));
}

#[test]
fn saving_a_skill_sends_the_edited_fields_and_a_wrong_field_is_said_instead() {
    let settings = settings();
    let mut skills = skills::Skills::default();
    let mut update = |message| skills.update(message, Some(&settings)).units();
    assert_eq!(update(skills::Message::Select("house-style".into())), 1);
    update(skills::Message::Opened(
        "house-style".into(),
        Ok(house_style()),
    ));
    update(skills::Message::DescriptionEdited(" Write well. ".into()));
    assert_eq!(update(skills::Message::Save), 1);
    let (name, description, text) = skills.save_request().unwrap();
    assert_eq!(
        (name.as_str(), description.as_str(), text.as_str()),
        (
            "house-style",
            "Write well.",
            "# House style\n\nShort sentences. No jargon."
        )
    );
    let mut update = |message| skills.update(message, Some(&settings)).units();
    update(skills::Message::NewSkill);
    update(skills::Message::NameEdited("bad name".into()));
    assert_eq!(update(skills::Message::Save), 0);
    assert!(skills.save_request().is_err());
}

#[test]
fn a_saved_skill_says_its_commit_and_deleting_asks_first() {
    let mut app = skills_app();
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Skills(skills::Message::Save)]
    ));
    let mut saved = house_style();
    saved.commit = Some("a1b2c3d".into());
    let _ = app.update(Message::Skills(skills::Message::Saved(Ok(saved))));
    assert!(shows(&app, "Committed: a1b2c3d"));
    let named = ClientError::Server {
        status: 409,
        message: "the skill house-style is named by the agents Coder".into(),
    };
    let _ = app.update(Message::Skills(skills::Message::Saved(Err(named))));
    assert!(shows(
        &app,
        "the skill house-style is named by the agents Coder"
    ));
    assert!(!shows(&app, "Committed: a1b2c3d"));
    click(&mut app, "Delete");
    assert!(shows(&app, "Delete house-style?"));
    assert!(matches!(
        &clicked(&app, "Delete skill")[..],
        [Message::Skills(skills::Message::Delete)]
    ));
    assert!(matches!(
        &clicked(&app, "Keep")[..],
        [Message::Skills(skills::Message::Keep)]
    ));
}

fn docs_search() -> client::McpServerDetail {
    serde_json::from_value(json!({
        "name": "profiler", "command": "profiler-mcp", "args": ["--root", "/srv/profiles"],
        "env": {"API_KEY": "${PROFILER_KEY}", "REGION": "eu"}
    }))
    .unwrap()
}

/// The MCP servers page of a server with a harness repository, with the server `profiler` open.
fn mcp_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::McpServers));
    let mcp = |message| Message::Mcp(message);
    let _ = app.update(mcp(mcp_servers::Message::Listed(Ok(mcp_listing()))));
    let _ = app.update(mcp(mcp_servers::Message::Harness(Ok(skill_listing()))));
    let _ = app.update(mcp(mcp_servers::Message::Select("profiler".into())));
    let _ = app.update(mcp(mcp_servers::Message::Opened(
        "profiler".into(),
        Ok(docs_search()),
    )));
    app
}

#[test]
fn the_mcp_servers_page_lists_the_servers_and_edits_one() {
    draw_sized(
        &mcp_app(),
        "mcp-servers",
        &[
            "MCP servers",
            "New server",
            "docs-search",
            "remote",
            "profiler",
            "stdio",
            "leaky",
            "${OPENAI_API_KEY} is not in mcp_env",
            "Delete",
            "Save",
            "Kind",
            "Command",
            "profiler-mcp",
            "Arguments, one per line",
            "Environment",
            "Values may be ${NAME}",
            "API_KEY",
            "${PROFILER_KEY}",
            "Add variable",
        ],
        Size::new(1440.0, 900.0),
    );
}

#[test]
fn saving_an_mcp_server_sends_a_command_or_a_url_and_a_wrong_field_is_said_instead() {
    let settings = settings();
    let mut servers = mcp_servers::McpServers::default();
    let mut update = |message| servers.update(message, Some(&settings)).units();
    assert_eq!(update(mcp_servers::Message::Select("profiler".into())), 1);
    update(mcp_servers::Message::Opened(
        "profiler".into(),
        Ok(docs_search()),
    ));
    let (name, sent) = servers.save_request().unwrap();
    assert_eq!(name, "profiler");
    assert_eq!(
        serde_json::to_value(&sent).unwrap(),
        json!({"command": "profiler-mcp", "args": ["--root", "/srv/profiles"], "url": null,
               "env": {"API_KEY": "${PROFILER_KEY}", "REGION": "eu"}, "headers": {}})
    );

    // A URL server takes headers, and the environment of the command is left out.
    let mut update = |message| servers.update(message, Some(&settings)).units();
    update(mcp_servers::Message::KindChosen("URL".into()));
    update(mcp_servers::Message::UrlEdited(
        " https://x.test/mcp ".into(),
    ));
    update(mcp_servers::Message::RowNameEdited(
        0,
        "Authorization".into(),
    ));
    update(mcp_servers::Message::RowValueEdited(
        0,
        "Bearer ${T}".into(),
    ));
    update(mcp_servers::Message::RemoveRow(1));
    update(mcp_servers::Message::AddRow);
    assert_eq!(update(mcp_servers::Message::Save), 1);
    let (_, sent) = servers.save_request().unwrap();
    assert_eq!(
        serde_json::to_value(&sent).unwrap(),
        json!({"command": null, "args": [], "url": "https://x.test/mcp",
               "env": {}, "headers": {"Authorization": "Bearer ${T}"}})
    );

    let mut update = |message| servers.update(message, Some(&settings)).units();
    update(mcp_servers::Message::UrlEdited(" ".into()));
    assert_eq!(update(mcp_servers::Message::Save), 0);
    assert!(servers.save_request().is_err());
}

#[test]
fn an_mcp_server_save_shows_its_commit_and_the_servers_refusal() {
    let mut app = mcp_app();
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Mcp(mcp_servers::Message::Save)]
    ));
    let mut saved = docs_search();
    saved.commit = Some("c0ffee1".into());
    let _ = app.update(Message::Mcp(mcp_servers::Message::Saved(Ok(saved))));
    assert!(shows(&app, "Committed: c0ffee1"));
    let refused = ClientError::Server {
        status: 422,
        message: "${OPENAI_API_KEY} is not in mcp_env of [harness] in moebius.toml".into(),
    };
    let _ = app.update(Message::Mcp(mcp_servers::Message::Saved(Err(refused))));
    assert!(shows(
        &app,
        "${OPENAI_API_KEY} is not in mcp_env of [harness] in moebius.toml"
    ));
    click(&mut app, "Delete");
    assert!(matches!(
        &clicked(&app, "Delete server")[..],
        [Message::Mcp(mcp_servers::Message::Delete)]
    ));
}
