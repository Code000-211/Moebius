//! The Setup pages: Models with its upstreams, agents, teacher and decision model, Plugins, and Settings with every
//! key of `moebius.toml`; what a save sends, the banner of a restart, and a model picked from the list.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client;
use moebius_desktop::screens::setup;
use serde_json::json;

use crate::fixtures::{clicked_sized, draw, draw_sized, overview, pressed, shows, simulator};

/// The answer of `GET /v1/settings`: every key of the reference with its default, and `set` (keys of the file) on top.
pub fn settings_answer(set: serde_json::Value, more: serde_json::Value) -> serde_json::Value {
    let mut values = serde_json::Map::new();
    for (key, default) in [
        ("data_dir", json!(".moebius")),
        ("listen", json!("127.0.0.1:7171")),
        ("token_file", json!("token")),
        ("run_limit", json!(3)),
        ("retro_at", json!("23:30")),
        ("rules_file", json!(null)),
        ("harness.mcp_env", json!([])),
        ("harness.url", json!(null)),
        ("trial_window.start", json!("00:00")),
        ("trial_window.end", json!("06:00")),
        ("regression.size", json!(5)),
        ("regression.gate", json!(true)),
        ("board.tick_ms", json!(2000)),
        ("board.round_limit", json!(5)),
        ("budgets.day_tokens", json!(null)),
        ("checks.timeout_seconds", json!(1800)),
        ("router.listen", json!("127.0.0.1:7172")),
        ("router.upstreams", json!([])),
        ("router.classes", json!({})),
        ("pi_bolt.program", json!("pi-bolt")),
        ("teacher.model", json!(null)),
        ("teacher.protocol", json!("openai")),
        ("teacher.timeout_seconds", json!(20)),
        ("decision_model.url", json!(null)),
        ("decision_model.model", json!(null)),
        ("decision_model.api_key_env", json!(null)),
        ("decision_model.api_key_secret", json!(null)),
        ("decision_model.timeout_seconds", json!(20)),
        ("decision_model.shadow", json!(null)),
        ("decision_model.agreement", json!(0.95)),
        ("decision_model.min_judgements", json!(100)),
        ("github.app_id", json!(null)),
        ("github.private_key_file", json!(null)),
        ("github.api_url", json!("https://api.github.com")),
        ("github.poll_seconds", json!(30)),
        ("gitlab.url", json!("https://gitlab.com")),
        ("gitlab.token_file", json!(null)),
        ("gitlab.token_secret", json!(null)),
        ("gitlab.poll_seconds", json!(30)),
        ("forgejo.url", json!("https://codeberg.org")),
        ("forgejo.token_file", json!(null)),
        ("forgejo.token_secret", json!(null)),
        ("forgejo.poll_seconds", json!(30)),
        ("agents", json!({})),
    ] {
        values.insert(key.into(), json!({"value": default, "default": true}));
    }
    for (key, value) in set.as_object().unwrap() {
        values.insert(key.clone(), json!({"value": value, "default": false}));
    }
    let mut answer = json!({"file": "/srv/moebius/moebius.toml", "values": values, "secrets": [],
                            "restart_needed": false, "restart_keys": []});
    answer
        .as_object_mut()
        .unwrap()
        .extend(more.as_object().unwrap().clone());
    answer
}

/// The settings of a server that is set up as the mockups are.
fn configured() -> serde_json::Value {
    settings_answer(
        json!({
            "data_dir": "/srv/moebius/.moebius", "board.round_limit": 7, "budgets.day_tokens": 20000000,
            "retro_at": "22:30", "harness.url": "https://github.com/acme/harness-configuration",
            "harness.mcp_env": ["DOCS_TOKEN"], "github.app_id": 123456,
            "github.private_key_file": "/srv/moebius/github-app.pem",
            "router.upstreams": [
                {"protocol": "openai", "base_url": "http://127.0.0.1:8080"},
                {"protocol": "anthropic", "base_url": "https://api.anthropic.com", "api_key_secret": "anthropic"},
                {"protocol": "openai", "base_url": "https://api.example.com/v1", "api_key_env": "EXAMPLE_API_KEY"},
            ],
            "router.classes": {
                "strong": [{"model": "claude-opus-fake", "day_tokens": 5000000}, "qwen3.8-flash-next"],
            },
            "teacher.model": "qwen3.8-flash-next",
            "decision_model.url": "http://127.0.0.1:8081/v1", "decision_model.model": "moebius-decider",
            "decision_model.api_key_env": "DECISION_MODEL_KEY",
        }),
        json!({"secrets": ["anthropic"]}),
    )
}

fn models_answer() -> serde_json::Value {
    json!({
        "upstreams": [
            {"protocol": "openai", "base_url": "http://127.0.0.1:8080", "api_key_set": false},
            {"protocol": "anthropic", "base_url": "https://api.anthropic.com", "api_key_set": true},
        ],
        "agents": [
            {"name": "Moebius", "model": "claude-fake", "protocol": "anthropic", "built_in": true},
            {"name": "Coder", "model": "qwen3.8-flash-next", "protocol": "openai", "thinking": "medium",
             "token_budget": 2000000, "built_in": false},
            {"name": "Reviewer", "model": "claude-fake", "protocol": "anthropic", "thinking": "high",
             "built_in": false},
        ],
        "teacher": {"model": "qwen3.8-flash-next", "protocol": "openai", "timeout_seconds": 20},
        "decision_model": {"url": "http://127.0.0.1:8081/v1", "model": "moebius-decider",
                           "timeout_seconds": 20, "api_key_set": true},
    })
}

fn setup_messages(models: serde_json::Value, server: serde_json::Value) -> [setup::Message; 3] {
    let plugins = json!({
        "time_limit_seconds": 30,
        "plugins": [
            {"name": "decision-model", "version": "0.1.0", "kinds": ["decider"], "decides": ["merge"]},
            {"name": "guard", "version": "0.1.0", "kinds": ["decider"], "decides": ["tool_call"]},
            {"name": "notes", "version": "0.2.1", "kinds": ["decider", "view"], "decides": []},
            {"name": "allow-reads", "version": "0.1.0", "kinds": ["decider"], "decides": ["start_card"],
             "process": {"program": "/srv/plugins/allow-reads", "state": "running", "restarts": 0, "error": null,
                         "log": "/srv/.moebius/plugins/allow-reads.log"}},
            {"name": "ticket-check", "version": "", "kinds": [], "decides": ["approve_issue"],
             "process": {"program": "python3", "state": "restarting", "restarts": 3,
                         "error": "it exited before its manifest (exit status: 1)",
                         "log": "/srv/.moebius/plugins/ticket-check.log"}},
        ],
    });
    [
        setup::Message::Models(Ok(serde_json::from_value(models).unwrap())),
        setup::Message::Plugins(Ok(serde_json::from_value(plugins).unwrap())),
        setup::Message::Server(Ok(serde_json::from_value(server).unwrap())),
    ]
}

fn setup_with(page: Page, models: serde_json::Value, server: serde_json::Value) -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(page));
    for message in setup_messages(models, server) {
        let _ = app.update(Message::Setup(message));
    }
    app
}

fn setup_app(page: Page) -> App {
    setup_with(page, models_answer(), configured())
}

/// A page of the setup that has been told the same as `setup_app` and is not drawn: for the forms' requests.
fn setup_state(server: serde_json::Value) -> setup::Setup {
    let mut state = setup::Setup::default();
    for message in setup_messages(models_answer(), server) {
        let _ = state.update(message, None);
    }
    state
}

fn edit(state: &mut setup::Setup, key: &'static str, text: &str) {
    let _ = state.update(setup::Message::Edit(key, text.into()), None);
}

#[test]
fn the_models_page_has_the_upstreams_agents_the_teacher_and_the_decision_model_as_forms() {
    draw_sized(
        &setup_app(Page::Models),
        "models",
        &[
            "Models",
            "Upstreams",
            "Protocol",
            "Base URL",
            "Key",
            "Add upstream",
            "Remove",
            "Model classes",
            "Models, first tried first",
            "Add class",
            "Agents",
            "Coder",
            "qwen3.8-flash-next",
            "medium",
            "2.0M tok",
            "Agents are edited on the Agents page.",
            "Teacher",
            "Decision model",
            "Time limit (s)",
            "Turn off",
            "Save",
        ],
        Size::new(1440.0, 1500.0),
    );
}

#[test]
fn the_models_page_says_when_there_is_no_teacher_or_decision_model() {
    let models = json!({"upstreams": [], "agents": [], "teacher": null, "decision_model": null});
    let app = setup_with(Page::Models, models, settings_answer(json!({}), json!({})));
    let mut ui = simulator(&app);
    for text in [
        "No upstream is configured; every model request fails.",
        "The server has no agent.",
        "No teacher is configured; a call is refused while a rule in prose applies.",
        "No decision model is configured.",
        "Turn on",
        "Add upstream",
    ] {
        assert!(ui.find(text).is_ok(), "{text:?} is not on the screen");
    }
}

#[test]
fn the_plugins_page_lists_what_the_host_loaded() {
    draw(
        &setup_app(Page::Plugins),
        "plugins",
        &[
            "Plugins",
            "5 plugins loaded; a call into one is refused after 30 s.",
            "Loaded plugins",
            "decision-model",
            "guard",
            "tool_call",
            "0.2.1",
            "decider, view",
            "No kind of decision",
            "In the server",
            "Running",
            "Restarting",
            "Plugins in their own process",
            "/srv/plugins/allow-reads",
            "3 times",
            "It never stopped",
            "it exited before its manifest (exit status: 1)",
            "/srv/.moebius/plugins/ticket-check.log",
        ],
    );
}

#[test]
fn the_settings_page_has_every_key_in_a_box_with_its_default_and_this_app_with_the_token_masked() {
    let app = setup_app(Page::Settings);
    draw_sized(
        &app,
        "settings",
        &[
            "Settings",
            "The keys of /srv/moebius/moebius.toml. Save writes only what you changed.",
            "Server",
            "Data directory",
            "Desktop API listens on",
            "Default 127.0.0.1:7171",
            "Runs at a time",
            "Default 3",
            "Board and budgets",
            "Board tick (ms)",
            "Default 2000",
            "Coding rounds a card",
            "Checks time limit (s)",
            "Nightly schedule",
            "Retro at (HH:MM)",
            "Trial window starts",
            "Default 00:00",
            "Harness",
            "Repository URL",
            "MCP variables",
            "Pi-Bolt",
            "Program",
            "GitHub",
            "App id",
            "Private key file",
            "Poll every (s)",
            "GitLab",
            "Token file",
            "Token secret",
            "Forgejo",
            "This app",
            "http://127.0.0.1:7171",
            "••••oken",
            "Regression cards",
            "Connect to another server",
            "Save",
        ],
        Size::new(1440.0, 2000.0),
    );
    assert!(simulator(&app).find("not-a-real-token").is_err());
    assert_eq!(
        format!(
            "{:?}",
            clicked_sized(&app, "Connect to another server", Size::new(1440.0, 2000.0))
        ),
        "[ChangeConnection]"
    );
    assert_eq!(
        pressed(&app, "Save").as_deref(),
        Some("[Setup(SaveSettings)]")
    );
}

#[test]
fn a_key_that_needs_a_restart_is_a_banner_with_a_restart_button_that_is_replaced_while_it_restarts()
{
    let server = settings_answer(
        json!({"listen": "127.0.0.1:9999", "teacher.model": "m"}),
        json!({"restart_needed": true, "restart_keys": ["listen", "teacher.model"]}),
    );
    let mut app = setup_with(Page::Settings, models_answer(), server.clone());
    draw_sized(
        &app,
        "settings-restart",
        &[
            "Restart the server to use: listen, teacher.model",
            "Restart",
        ],
        Size::new(1440.0, 1700.0),
    );
    assert_eq!(
        pressed(&app, "Restart").as_deref(),
        Some("[Setup(Restart)]")
    );
    // The banner is on the Models page too.
    let _ = app.update(Message::Select(Page::Models));
    assert!(shows(
        &app,
        "Restart the server to use: listen, teacher.model"
    ));
    let _ = app.update(Message::Setup(setup::Message::Restart));
    assert!(shows(
        &app,
        "Restarting the server. The app connects again by itself."
    ));
    assert_eq!(pressed(&app, "Restart"), None);
    // The server's answer after it is back says nothing is left to restart.
    let _ = app.update(Message::Setup(setup::Message::Server(Ok(
        serde_json::from_value(settings_answer(json!({}), json!({}))).unwrap(),
    ))));
    assert!(!shows(
        &app,
        "Restarting the server. The app connects again by itself."
    ));
    assert!(!shows(
        &app,
        "Restart the server to use: listen, teacher.model"
    ));
}

#[test]
fn a_save_says_saved_or_the_servers_message() {
    let mut app = setup_app(Page::Settings);
    assert!(!shows(&app, "Saved"));
    let saved =
        serde_json::from_value(json!({"restart_needed": false, "restart_keys": []})).unwrap();
    let _ = app.update(Message::Setup(setup::Message::Saved(Ok(Some(saved)))));
    assert!(shows(&app, "Saved"));
    let refused = client::ClientError::Server {
        status: 422,
        message: "run_limit must be at least 1".into(),
    };
    let _ = app.update(Message::Setup(setup::Message::Saved(Err(refused))));
    assert!(shows(&app, "run_limit must be at least 1"));
    // Nothing is sent when nothing was changed.
    let _ = app.update(Message::Setup(setup::Message::SaveSettings));
    assert!(shows(&app, "Nothing changed, so nothing was saved."));
    // A wrong number is said, not sent.
    let _ = app.update(Message::Setup(setup::Message::Edit(
        "run_limit",
        "many".into(),
    )));
    let _ = app.update(Message::Setup(setup::Message::SaveSettings));
    assert!(shows(&app, "Runs at a time is a whole number."));
}

#[test]
fn saving_the_settings_sends_exactly_the_keys_that_were_changed() {
    let mut state = setup_state(configured());
    assert_eq!(state.save_request(false), Ok(setup::Plan::default()));
    edit(&mut state, "board.round_limit", "9");
    edit(&mut state, "budgets.day_tokens", "5000");
    edit(&mut state, "listen", " 127.0.0.1:9000 ");
    edit(&mut state, "harness.mcp_env", "DOCS_TOKEN, OTHER");
    // A key put back to what the file says is not sent.
    edit(&mut state, "retro_at", "21:00");
    edit(&mut state, "retro_at", "22:30");
    let plan = state.save_request(false).unwrap();
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"board.round_limit": 9, "budgets.day_tokens": 5000, "listen": "127.0.0.1:9000",
               "harness.mcp_env": ["DOCS_TOKEN", "OTHER"]})
    );
    assert!(plan.remove.is_empty() && plan.store.is_empty());

    // A blank input takes a key out of the file, and a table whose inputs are all blank goes as a whole.
    let mut state = setup_state(configured());
    edit(&mut state, "budgets.day_tokens", "");
    edit(&mut state, "board.round_limit", "");
    edit(&mut state, "github.app_id", "");
    edit(&mut state, "github.private_key_file", "");
    let plan = state.save_request(false).unwrap();
    assert!(plan.set.is_empty());
    assert_eq!(
        plan.remove,
        ["board.round_limit", "budgets.day_tokens", "github"]
    );
    // One blank key of a table keeps the others, and the server says what is missing.
    let mut state = setup_state(configured());
    edit(&mut state, "github.app_id", "");
    assert_eq!(state.save_request(false).unwrap().remove, ["github.app_id"]);

    let mut state = setup_state(configured());
    edit(&mut state, "board.tick_ms", "fast");
    assert_eq!(
        state.save_request(false),
        Err("Board tick (ms) is a whole number.".into())
    );
}

#[test]
fn saving_the_models_sends_the_changed_rows_the_typed_key_and_the_tables_turned_off() {
    let mut state = setup_state(configured());
    let models = |state: &setup::Setup| state.save_request(true);
    assert_eq!(models(&state), Ok(setup::Plan::default()));

    // An upstream with a typed key is a row at the end, and the key is stored under a new name.
    let _ = state.update(setup::Message::AddUpstream, None);
    let _ = state.update(
        setup::Message::UpstreamUrl(3, "https://api.new.example".into()),
        None,
    );
    let _ = state.update(
        setup::Message::UpstreamProtocol(3, "anthropic".into()),
        None,
    );
    let _ = state.update(
        setup::Message::KeyMode(setup::Owner::Upstream(3), setup::KeyMode::Typed),
        None,
    );
    assert_eq!(
        models(&state),
        Err("Type the key of the upstream 4.".into())
    );
    let _ = state.update(
        setup::Message::KeyTyped(setup::Owner::Upstream(3), "sk-typed-secret".into()),
        None,
    );
    let plan = models(&state).unwrap();
    assert_eq!(
        plan.store,
        [("upstream-1".to_string(), "sk-typed-secret".to_string())]
    );
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"router.upstreams.3": {"protocol": "anthropic", "base_url": "https://api.new.example",
                                      "api_key_secret": "upstream-1"}})
    );
    assert!(plan.remove.is_empty());

    // A key stored before is replaced under its name, and nothing else of the row is sent again.
    let mut state = setup_state(configured());
    let _ = state.update(
        setup::Message::KeyTyped(setup::Owner::Upstream(1), "sk-new".into()),
        None,
    );
    let plan = models(&state).unwrap();
    assert_eq!(
        plan.store,
        [("anthropic".to_string(), "sk-new".to_string())]
    );
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"router.upstreams.1": {"protocol": "anthropic", "base_url": "https://api.anthropic.com",
                                      "api_key_secret": "anthropic"}})
    );

    // A removed row is removed by its place, the rows after it keep what they have, and a changed one is set at the
    // place it has then.
    let mut state = setup_state(configured());
    let _ = state.update(setup::Message::RemoveUpstream(0), None);
    let _ = state.update(
        setup::Message::KeyEnv(setup::Owner::Upstream(1), "NEW_KEY".into()),
        None,
    );
    let _ = state.update(
        setup::Message::KeyMode(setup::Owner::Upstream(1), setup::KeyMode::Environment),
        None,
    );
    let plan = models(&state).unwrap();
    assert_eq!(plan.remove, ["router.upstreams.0"]);
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"router.upstreams.1": {"protocol": "openai", "base_url": "https://api.example.com/v1",
                                      "api_key_env": "NEW_KEY"}})
    );
    assert!(plan.store.is_empty());

    // The teacher and the decision model are tables that are removed when they are turned off, and the decision
    // model's key can change from the environment to a typed one.
    let mut state = setup_state(configured());
    let _ = state.update(setup::Message::ToggleTeacher, None);
    edit(&mut state, "decision_model.timeout_seconds", "9");
    let _ = state.update(
        setup::Message::KeyMode(setup::Owner::DecisionModel, setup::KeyMode::Typed),
        None,
    );
    let _ = state.update(
        setup::Message::KeyTyped(setup::Owner::DecisionModel, "dm-key".into()),
        None,
    );
    let plan = models(&state).unwrap();
    assert_eq!(plan.remove, ["teacher", "decision_model.api_key_env"]);
    assert_eq!(
        plan.store,
        [("decision-model".to_string(), "dm-key".to_string())]
    );
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"decision_model.timeout_seconds": 9, "decision_model.api_key_secret": "decision-model"})
    );

    // A table that was off and is turned on is sent whole, and needs its model.
    let mut state = setup_state(settings_answer(json!({}), json!({})));
    let _ = state.update(setup::Message::ToggleTeacher, None);
    assert_eq!(
        models(&state),
        Err("Enter the model of the teacher.".into())
    );
    edit(&mut state, "teacher.model", "judge");
    assert_eq!(
        serde_json::Value::Object(models(&state).unwrap().set),
        json!({"teacher.model": "judge", "teacher.protocol": "openai"})
    );
}

pub fn available_models() -> client::Available {
    serde_json::from_value(json!({
        "models": [
            {"id": "claude-opus-fake", "protocol": "anthropic", "upstream": "https://api.anthropic.com"},
            {"id": "qwen3.8-flash-next", "protocol": "openai", "upstream": "http://127.0.0.1:8080"},
        ],
        "errors": [{"upstream": "https://api.example.com/v1", "message": "401"}],
        "classes": [{"name": "strong", "models": ["claude-opus-fake", "qwen3.8-flash-next"],
                     "protocols": ["anthropic", "openai"]}],
    }))
    .unwrap()
}

pub fn claude_opus() -> client::AvailableModel {
    available_models().models.remove(0)
}

#[test]
fn a_model_picked_from_the_list_fills_the_models_page_and_the_errors_show_once() {
    let mut app = setup_app(Page::Models);
    let _ = app.update(Message::Setup(setup::Message::Available(Ok(
        available_models(),
    ))));
    let _ = app.update(Message::Setup(setup::Message::PickModel(
        "teacher.model",
        claude_opus(),
    )));
    draw_sized(
        &app,
        "models-picked",
        &["Refresh", "Could not ask https://api.example.com/v1: 401"],
        Size::new(1440.0, 1500.0),
    );
}

#[test]
fn picking_a_model_sets_the_field_and_the_protocol_and_a_typed_model_is_kept() {
    let mut state = setup_state(configured());
    let _ = state.update(setup::Message::Available(Ok(available_models())), None);
    let _ = state.update(
        setup::Message::PickModel("teacher.model", claude_opus()),
        None,
    );
    let plan = state.save_request(true).unwrap();
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"teacher.model": "claude-opus-fake", "teacher.protocol": "anthropic"})
    );

    // The decision model has no protocol to follow.
    let _ = state.update(
        setup::Message::PickModel("decision_model.model", claude_opus()),
        None,
    );
    let plan = state.save_request(true).unwrap();
    assert_eq!(plan.set["decision_model.model"], "claude-opus-fake");
    assert!(!plan.set.contains_key("decision_model.protocol"));

    // A model no upstream lists can be typed, after "Another model…".
    let _ = state.update(setup::Message::TypeModel("teacher.model"), None);
    edit(&mut state, "teacher.model", "my-own-model");
    let plan = state.save_request(true).unwrap();
    assert_eq!(plan.set["teacher.model"], "my-own-model");
}

#[test]
fn saving_the_models_sends_the_classes_whole_and_keeps_a_models_tokens_for_the_day() {
    let mut state = setup_state(configured());
    // A class's models are edited as names separated by commas; the table is written in place of the file's.
    let _ = state.update(
        setup::Message::ClassModels(0, "qwen3.8-flash-next, claude-opus-fake".into()),
        None,
    );
    let _ = state.update(setup::Message::AddClass, None);
    assert_eq!(
        state.save_request(true),
        Err("Enter the name of each model class.".into())
    );
    let _ = state.update(setup::Message::ClassName(1, "fast".into()), None);
    assert_eq!(
        state.save_request(true),
        Err("Enter the models of the class fast.".into())
    );
    let _ = state.update(setup::Message::ClassModels(1, " small ,".into()), None);
    let plan = state.save_request(true).unwrap();
    assert_eq!(plan.remove, ["router.classes"]);
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"router.classes": {
            "strong": ["qwen3.8-flash-next", {"model": "claude-opus-fake", "day_tokens": 5000000}],
            "fast": ["small"],
        }})
    );
    let _ = state.update(setup::Message::ClassName(1, "strong".into()), None);
    assert_eq!(
        state.save_request(true),
        Err("There are two classes named strong.".into())
    );

    // Without classes the table is taken out of the file.
    let mut state = setup_state(configured());
    let _ = state.update(setup::Message::RemoveClass(0), None);
    let plan = state.save_request(true).unwrap();
    assert_eq!(plan.remove, ["router.classes"]);
    assert!(plan.set.is_empty());
}

#[test]
fn the_teachers_model_field_offers_the_classes_first() {
    let available = available_models();
    let choices = available.choices();
    let ids: Vec<&str> = choices.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["strong", "claude-opus-fake", "qwen3.8-flash-next"]);
    assert!(choices[0].class);
    // Its models are served in two protocols, so picking it leaves the teacher's protocol as it is.
    let mut state = setup_state(configured());
    let _ = state.update(setup::Message::Available(Ok(available)), None);
    let _ = state.update(
        setup::Message::PickModel("teacher.model", choices[0].clone()),
        None,
    );
    let plan = state.save_request(true).unwrap();
    assert_eq!(
        serde_json::Value::Object(plan.set),
        json!({"teacher.model": "strong"})
    );
}

/// The answers of a server whose second upstream logs in with OAuth by a device code.
fn oauth_answers() -> (serde_json::Value, serde_json::Value) {
    let mut models = models_answer();
    models["upstreams"][1] = json!({
        "protocol": "anthropic", "base_url": "https://api.example.com", "api_key_set": false,
        "oauth": {"flow": "device", "logged_in": false,
                  "warning": "Using a subscription through a third-party tool may break the provider's terms of service."},
    });
    let server = settings_answer(
        json!({"router.upstreams": [
            {"protocol": "openai", "base_url": "http://127.0.0.1:8080"},
            {"protocol": "anthropic", "base_url": "https://api.example.com",
             "oauth": {"flow": "device", "client_id": "moebius", "token_url": "https://login.example.com/token",
                       "device_url": "https://login.example.com/device", "secret": "example-login"}},
        ]}),
        json!({}),
    );
    (models, server)
}

fn login(state: &str, message: &str) -> client::Login {
    serde_json::from_value(json!({
        "state": state, "flow": "device", "user_code": "WDJB-MJHT",
        "verification_uri": "https://login.example.com/device",
        "verification_uri_complete": "https://login.example.com/device?code=WDJB-MJHT",
        "authorize_url": null, "message": message,
    }))
    .unwrap()
}

#[test]
fn an_upstream_that_logs_in_with_oauth_asks_to_confirm_the_warning_then_shows_the_code() {
    let (models, server) = oauth_answers();
    let mut app = setup_with(Page::Models, models, server);
    for text in [
        "Logs in with OAuth: a code you enter at the provider. Not logged in.",
        "Using a subscription through a third-party tool may break the provider's terms of service.",
    ] {
        assert!(shows(&app, text), "{text:?} is not on the screen");
    }
    assert!(pressed(&app, "Log in").unwrap().contains("LogIn(1)"));
    let _ = app.update(Message::Setup(setup::Message::LogIn(1)));
    assert!(
        pressed(&app, "I understand, log in")
            .unwrap()
            .contains("ConfirmLogIn(1)")
    );
    let _ = app.update(Message::Setup(setup::Message::CancelLogIn(1)));
    assert!(!shows(&app, "I understand, log in"));

    let _ = app.update(Message::Setup(setup::Message::LogIn(1)));
    let _ = app.update(Message::Setup(setup::Message::LoginStarted(
        1,
        Ok(login("waiting", "Open the page and enter the code.")),
    )));
    draw_sized(
        &app,
        "models-oauth-login",
        &[
            "WDJB-MJHT",
            "Enter this code at https://login.example.com/device",
            "Open the page",
            "Waiting for the login…",
        ],
        Size::new(1440.0, 1500.0),
    );
    assert!(
        pressed(&app, "Open the page")
            .unwrap()
            .contains("https://login.example.com/device?code=WDJB-MJHT")
    );

    let _ = app.update(Message::Setup(setup::Message::LoginAnswer(
        1,
        Ok(login(
            "logged_in",
            "Logged in: the upstream lists 2 models.",
        )),
    )));
    assert!(shows(&app, "Logged in: the upstream lists 2 models."));
    assert!(!shows(&app, "Waiting for the login…"));
}

#[test]
fn saving_an_upstream_that_logs_in_with_oauth_keeps_its_oauth_table() {
    let (_, server) = oauth_answers();
    let mut state = setup_state(server);
    let _ = state.update(
        setup::Message::UpstreamUrl(1, "https://api2.example.com".into()),
        None,
    );
    let plan = state.save_request(true).unwrap();
    assert_eq!(
        plan.set["router.upstreams.1"]["oauth"]["device_url"],
        "https://login.example.com/device"
    );
    assert!(
        plan.set["router.upstreams.1"]
            .get("api_key_secret")
            .is_none()
    );
}
