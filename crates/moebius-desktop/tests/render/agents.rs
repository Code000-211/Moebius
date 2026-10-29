//! The Agents page: the agents listed and edited, the Moebius agent's built-in part, agents kept in `moebius.toml`,
//! saving and deleting, the model picked from the upstreams' list, and the proposals that change an agent.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::ClientError;
use moebius_desktop::screens::agents;
use moebius_desktop::screens::harness_setup;
use serde_json::json;

use crate::fixtures::{clicked, draw_sized, overview, settings, shows, simulator_sized};
use crate::improve::proposal_listing;
use crate::setup::{available_models, claude_opus, settings_answer};

pub fn agent_listing() -> Vec<moebius_desktop::client::Agent> {
    serde_json::from_value(json!([
        {"name": "Moebius", "model": "claude-fake", "protocol": "anthropic", "built_in": true},
        {"name": "Coder", "model": "qwen-fake", "protocol": "openai", "built_in": false},
        {"name": "Typo", "model": "", "protocol": "openai", "built_in": false,
         "error": "agents/Typo.toml: unknown field `modle`"}
    ]))
    .unwrap()
}

pub fn skill_listing() -> Vec<moebius_desktop::client::Skill> {
    serde_json::from_value(json!([
        {"name": "moebius", "description": "Build on Moebius.", "built_in": true},
        {"name": "radiance-kernels", "description": "How the kernels are written.", "built_in": false},
        {"name": "house-style", "description": "Write plainly.", "built_in": false}
    ]))
    .unwrap()
}

pub fn mcp_listing() -> Vec<moebius_desktop::client::McpServer> {
    serde_json::from_value(json!([
        {"name": "docs-search", "kind": "remote"},
        {"name": "profiler", "kind": "stdio"},
        {"name": "leaky", "kind": "unknown", "error": "${OPENAI_API_KEY} is not in mcp_env"}
    ]))
    .unwrap()
}

fn coder() -> moebius_desktop::client::AgentDetail {
    serde_json::from_value(json!({
        "name": "Coder",
        "settings": {"model": "qwen-fake", "protocol": "openai", "thinking": "high",
                     "token_budget": 1_500_000, "skills": ["radiance-kernels"],
                     "extensions": ["extensions/log.ts"], "mcp": ["docs-search"]},
        "instructions": "# Coder\nYou work on one card at a time, in its own worktree.\n\n- Run the tests before you say you are done.\n"
    }))
    .unwrap()
}

/// The Agents page of a server with a harness repository: three agents and the Coder open in the editor.
pub fn agents_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Agents));
    let agents = |message| Message::Agents(message);
    let _ = app.update(agents(agents::Message::Listed(Ok(agent_listing()))));
    let _ = app.update(agents(agents::Message::SkillsListed(Ok(skill_listing()))));
    let _ = app.update(agents(agents::Message::McpListed(Ok(mcp_listing()))));
    let _ = app.update(agents(agents::Message::Select("Coder".into())));
    let _ = app.update(agents(agents::Message::Opened("Coder".into(), Ok(coder()))));
    app
}

/// The agents screen drawn: compare `tests/snapshots/agents-tiny-skia.png` with `docs/design/agents.png`.
#[test]
fn the_agents_screen_lists_the_agents_and_edits_one_like_the_mockup() {
    draw_sized(
        &agents_app(),
        "agents",
        &[
            "Agents",
            "New agent",
            "Moebius",
            "Built in",
            "claude-fake · anthropic",
            "Coder",
            "qwen-fake · openai",
            "Typo",
            "Broken",
            "agents/Typo.toml: unknown field `modle`",
            "Save",
            "Delete",
            "Model",
            "Protocol",
            "Thinking",
            "Token budget per card",
            "Instructions",
            "Skills",
            "1 of 2",
            "radiance-kernels",
            "How the kernels are written.",
            "house-style",
            "MCP servers",
            "docs-search",
            "remote",
            "leaky",
            "${OPENAI_API_KEY} is not in mcp_env",
            "Extensions",
            "extensions/log.ts",
            "Add extension",
            "From the Retro",
            "No proposal waiting for a decision changes this agent.",
        ],
        Size::new(1440.0, 1300.0),
    );
}

/// The Moebius agent as `GET /v1/agents/Moebius` answers it: its own files and what it brings itself.
fn moebius_agent() -> moebius_desktop::client::AgentDetail {
    serde_json::from_value(json!({
        "name": "Moebius",
        "settings": {"model": "claude-fake", "protocol": "anthropic", "skills": [], "extensions": [], "mcp": []},
        "instructions": "Write scripts in the style of the repository's other scripts.",
        "built_in": {
            "instructions": "You are the Moebius agent. You work for the person in this chat.\n\nKnow before you act. Each kind of task has a skill with its steps; read it first.",
            "skills": [
                {"name": "moebius", "description": "The guide and the exact references of Moebius."},
                {"name": "moebius-operate", "description": "Read the state of a Moebius server and act on it."},
                {"name": "moebius-configure", "description": "Change agents, rules, scripts, settings, skills and MCP servers."}
            ]
        }
    }))
    .unwrap()
}

/// A built-in agent shows what it always has, read only, above what the person adds.
#[test]
fn the_moebius_agent_shows_its_built_in_instructions_and_skills() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Agents));
    let agents = |message| Message::Agents(message);
    let _ = app.update(agents(agents::Message::Listed(Ok(agent_listing()))));
    let _ = app.update(agents(agents::Message::SkillsListed(Ok(skill_listing()))));
    let _ = app.update(agents(agents::Message::McpListed(Ok(mcp_listing()))));
    let _ = app.update(agents(agents::Message::Select("Moebius".into())));
    let _ = app.update(agents(agents::Message::Opened(
        "Moebius".into(),
        Ok(moebius_agent()),
    )));
    draw_sized(
        &app,
        "agents-moebius",
        &[
            "Moebius",
            "Built-in instructions",
            "Read only · Moebius brings them",
            "Built-in skills",
            "Read only · 3 skills",
            "moebius-operate",
            "Read the state of a Moebius server and act on it.",
            "Markdown · added after the built-in instructions",
            "Model",
            "Save",
        ],
        Size::new(1440.0, 1500.0),
    );

    // An agent of the person's has no built-in part.
    let _ = app.update(agents(agents::Message::Select("Coder".into())));
    let _ = app.update(agents(agents::Message::Opened("Coder".into(), Ok(coder()))));
    assert!(!shows(&app, "Built-in instructions"));
}

#[test]
fn a_built_in_agent_without_files_is_edited_from_blank_and_shows_its_built_in_part() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Agents));
    let agents = |message| Message::Agents(message);
    let _ = app.update(agents(agents::Message::Listed(Ok(agent_listing()))));
    let _ = app.update(agents(agents::Message::SkillsListed(Ok(skill_listing()))));
    let _ = app.update(agents(agents::Message::Select("Moebius".into())));
    let mut answer = moebius_agent();
    answer.settings = None;
    answer.instructions = String::new();
    let _ = app.update(agents(agents::Message::Opened(
        "Moebius".into(),
        Ok(answer),
    )));
    assert!(shows(&app, "Built-in skills"));
    // Nothing to delete: the agent has no files.
    assert!(!shows(&app, "Delete"));

    // Without a harness repository the built-in part is read besides moebius.toml.
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Agents));
    let _ = app.update(agents(agents::Message::Listed(Ok(agent_listing()))));
    let _ = app.update(agents(agents::Message::SkillsListed(Ok(vec![]))));
    assert_eq!(
        app.update(agents(agents::Message::Select("Moebius".into())))
            .units(),
        2
    );
    assert_eq!(
        app.update(agents(agents::Message::Select("Coder".into())))
            .units(),
        1
    );
    let _ = app.update(agents(agents::Message::Select("Moebius".into())));
    let _ = app.update(agents(agents::Message::BuiltIn(
        "Moebius".into(),
        Ok(moebius_agent()),
    )));
    let server = settings_answer(json!({}), json!({}));
    let _ = app.update(agents(agents::Message::OpenedFromFile(
        "Moebius".into(),
        Ok(serde_json::from_value(server).unwrap()),
    )));
    assert!(shows(&app, "Built-in instructions"));
    assert!(shows(&app, "moebius-configure"));
}

#[test]
fn a_server_without_a_harness_repository_edits_its_agents_in_moebius_toml() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Agents));
    let _ = app.update(Message::Agents(agents::Message::Listed(
        Ok(agent_listing()),
    )));
    let _ = app.update(Message::Agents(agents::Message::SkillsListed(Ok(vec![]))));
    assert!(!shows(&app, "Agents are set in moebius.toml"));
    assert!(shows(
        &app,
        "What each run works with: its model, instructions, skills and extensions. An edit is saved in moebius.toml."
    ));
    assert!(shows(&app, harness_setup::NEEDED));
    assert!(matches!(
        &clicked(&app, "Set up")[..],
        [Message::Harness(harness_setup::Message::SetUp)]
    ));
    assert_eq!(
        format!("{:?}", clicked(&app, "Coder")),
        "[Agents(Select(\"Coder\"))]"
    );
    assert_eq!(
        format!("{:?}", clicked(&app, "New agent")),
        "[Agents(NewAgent)]"
    );
    // The agent is read from the settings, and has skill paths instead of the repository's skills and MCP servers.
    let server = settings_answer(
        json!({"agents": {"Coder": {"model": "local", "instructions": "Be brief.", "skills": ["skills/house"],
                                    "extensions": ["extensions/log.ts"], "token_budget": 5000}}}),
        json!({}),
    );
    let _ = app.update(Message::Agents(agents::Message::Select("Coder".into())));
    let _ = app.update(Message::Agents(agents::Message::OpenedFromFile(
        "Coder".into(),
        Ok(serde_json::from_value(server).unwrap()),
    )));
    for text in [
        "Directories, from moebius.toml",
        "skills/house",
        "extensions/log.ts",
        "Add skill",
    ] {
        assert!(shows(&app, text), "{text}");
    }
    assert!(!shows(&app, "No server in mcp.toml of the repository."));
}

#[test]
fn saving_sends_the_edited_settings_and_a_wrong_field_is_said_instead() {
    let settings = settings();
    let mut agents = agents::Agents::default();
    let mut update = |message| agents.update(message, Some(&settings)).units();
    update(agents::Message::Opened("Coder".into(), Ok(coder())));
    // Nothing is open before an agent is chosen.
    assert_eq!(update(agents::Message::Select("Coder".into())), 1);
    update(agents::Message::Opened("Coder".into(), Ok(coder())));
    update(agents::Message::ModelEdited("better-model".into()));
    update(agents::Message::ProtocolChosen("anthropic".into()));
    update(agents::Message::HarnessChosen("claude-code".into()));
    update(agents::Message::ThinkingChosen("Not set".into()));
    update(agents::Message::BudgetEdited("2000".into()));
    update(agents::Message::ToggleSkill("house-style".into()));
    update(agents::Message::ToggleSkill("radiance-kernels".into()));
    update(agents::Message::ToggleMcp("docs-search".into()));
    update(agents::Message::ToggleMcp("profiler".into()));
    update(agents::Message::ExtensionEdited(
        " extensions/two.ts ".into(),
    ));
    update(agents::Message::AddExtension);
    update(agents::Message::RemoveExtension("extensions/log.ts".into()));
    assert_eq!(update(agents::Message::Save), 1);
    let (name, sent, instructions) = agents.save_request().unwrap();
    assert_eq!(name, "Coder");
    assert_eq!(
        serde_json::to_value(&sent).unwrap(),
        json!({"model": "better-model", "protocol": "anthropic", "token_budget": 2000, "harness": "claude-code",
               "skills": ["house-style"], "extensions": ["extensions/two.ts"], "mcp": ["profiler"]})
    );
    assert_eq!(
        instructions,
        "# Coder\nYou work on one card at a time, in its own worktree.\n\n- Run the tests before you say you are done."
    );

    let mut update = |message| agents.update(message, Some(&settings)).units();
    update(agents::Message::BudgetEdited("lots".into()));
    assert_eq!(update(agents::Message::Save), 0);
    assert!(agents.save_request().is_err());
}

#[test]
fn the_save_button_sends_the_save_message_and_the_answer_is_shown() {
    let mut app = agents_app();
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Agents(agents::Message::Save)]
    ));
    let mut saved = coder();
    saved.commit = Some("a1b2c3d".into());
    let _ = app.update(Message::Agents(agents::Message::Saved(Ok(saved))));
    assert!(shows(&app, "Saved as a commit: a1b2c3d"));
    let invalid = ClientError::Server {
        status: 422,
        message: "the skill \"nope\" is not in skills/".into(),
    };
    let _ = app.update(Message::Agents(agents::Message::Saved(Err(invalid))));
    assert!(shows(&app, "the skill \"nope\" is not in skills/"));
    assert!(!shows(&app, "Saved as a commit: a1b2c3d"));
}

#[test]
fn a_new_agent_asks_for_a_name_and_deleting_asks_first() {
    let mut app = agents_app();
    for message in clicked(&app, "New agent") {
        assert!(matches!(
            message,
            Message::Agents(agents::Message::NewAgent)
        ));
        let _ = app.update(message);
    }
    assert!(shows(&app, "Name of the new agent"));
    assert!(!shows(&app, "Delete"));
    let _ = app.update(Message::Agents(agents::Message::NameEdited(
        "Strong".into(),
    )));
    let _ = app.update(Message::Agents(agents::Message::ModelEdited("big".into())));
    assert!(matches!(
        &clicked(&app, "Save")[..],
        [Message::Agents(agents::Message::Save)]
    ));

    let mut app = agents_app();
    assert!(matches!(
        &clicked(&app, "Delete")[..],
        [Message::Agents(agents::Message::AskDelete)]
    ));
    let _ = app.update(Message::Agents(agents::Message::AskDelete));
    assert!(shows(&app, "Delete Coder?"));
    assert!(matches!(
        &clicked(&app, "Delete agent")[..],
        [Message::Agents(agents::Message::Delete)]
    ));
    let _ = app.update(Message::Agents(agents::Message::KeepAgent));
    assert!(!shows(&app, "Delete Coder?"));
}

#[test]
fn the_agents_screen_offers_the_open_proposals_that_change_the_agent() {
    let mut app = agents_app();
    let _ = app.update(Message::Agents(agents::Message::ProposalsListed(Ok(
        proposal_listing(),
    ))));
    assert!(shows(&app, "Tell the Coder to run the tests first"));
    assert!(shows(&app, "proposal/2026-10-28-small-commits"));
    assert!(shows(&app, "Positive"));
    // The proposal that changes no agent is not offered here, nor one whose trials found it worse.
    assert!(!shows(&app, "Allow reading the docs directory"));
    assert!(!shows(&app, "Ask the Coder to write plainly"));
}

#[test]
fn the_agent_editor_picks_a_model_from_the_list_and_still_takes_a_typed_one() {
    let settings = settings();
    let mut agents = agents::Agents::default();
    let mut update = |message| agents.update(message, Some(&settings)).units();
    update(agents::Message::Available(Ok(available_models())));
    update(agents::Message::Select("Coder".into()));
    update(agents::Message::Opened("Coder".into(), Ok(coder())));
    update(agents::Message::ModelPicked(claude_opus()));
    let (_, sent, _) = agents.save_request().unwrap();
    assert_eq!(sent.model, "claude-opus-fake");
    assert_eq!(sent.protocol, "anthropic");

    let mut update = |message| agents.update(message, Some(&settings)).units();
    update(agents::Message::ModelEdited("my-own-model".into()));
    let (_, sent, _) = agents.save_request().unwrap();
    assert_eq!(sent.model, "my-own-model");
    assert_eq!(sent.protocol, "anthropic");
}

#[test]
fn the_agents_page_offers_a_refresh_and_says_which_upstream_could_not_be_asked() {
    let mut app = agents_app();
    assert!(!shows(&app, "Refresh"));
    let _ = app.update(Message::Agents(agents::Message::Available(Ok(
        available_models(),
    ))));
    assert!(shows(&app, "Refresh"));
    assert!(shows(&app, "Could not ask https://api.example.com/v1: 401"));
}

#[test]
fn the_model_field_is_a_dropdown_whose_last_entry_turns_it_into_an_input() {
    let mut app = agents_app();
    let _ = app.update(Message::Agents(agents::Message::Available(Ok(
        available_models(),
    ))));
    let size = Size::new(1440.0, 1400.0);
    draw_sized(&app, "agents-model-list", &["Model", "Refresh"], size);
    // A dropdown shows the model as its choice, not as the text of an input.
    assert!(simulator_sized(&app, size).find("qwen-fake").is_err());

    let _ = app.update(Message::Agents(agents::Message::TypeModel));
    let mut ui = simulator_sized(&app, size);
    assert!(matches!(
        ui.find("qwen-fake"),
        Ok(iced_test::selector::Text::Input { .. })
    ));
}
