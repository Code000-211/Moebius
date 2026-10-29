//! The Rules page: the patterns and rules with what they refused, a rule edited or added in its card, a repository's
//! rules, and the chat that changes them.

use iced::Size;
use iced::widget::text_editor;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::ClientError;
use moebius_desktop::screens::rules;
use serde_json::json;

use crate::fixtures::{
    clicked_sized, draw_sized, ids_sized, overview, settings, shows, simulator_sized, update_all,
};
use crate::new_chat::moebius_chat;

/// What `GET /v1/rules` answers for a server with a harness repository, two rules in prose and a repository that adds one.
fn rules_view(editable: bool) -> moebius_desktop::client::RulesView {
    serde_json::from_value(json!({
        "patterns": [
            {"name": "no-destructive-commands", "description": "commands that delete recursively outside the worktree, format a disk, overwrite a device or exhaust the machine are never run"},
            {"name": "no-force-push", "description": "git push --force rewrites history on the remote"},
            {"name": "stay-in-the-worktree", "description": "files are written only inside the run's worktree"},
            {"name": "no-secrets-in-commands", "description": "a command must not contain a secret; read it from the environment instead"}
        ],
        "prose": {
            "source": if editable { json!("harness") } else { json!("file") },
            "editable": editable,
            "text": "Rules for every run.\n\n## leave-ci-alone\nDo not edit files under .github/ or any other CI configuration.\n\n## no-vendoring\nDo not copy dependencies into the tree.\n",
            "rules": [
                {"name": "leave-ci-alone", "text": "Do not edit files under .github/ or any other CI configuration."},
                {"name": "no-vendoring", "text": "Do not copy dependencies into the tree."}
            ],
            "error": null
        },
        "repositories": [{
            "repository": "radiance",
            "repository_id": 1,
            "text": "## keep-migrations\nDo not edit a migration that exists.",
            "rules": [{"name": "keep-migrations", "text": "Do not edit a migration that exists."}],
            "error": null
        }],
        "refusals": {"no-force-push": 3, "leave-ci-alone": 1}
    }))
    .unwrap()
}

pub fn rules_app(editable: bool) -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Rules));
    let _ = app.update(Message::Rules(rules::Message::Loaded(Ok(rules_view(
        editable,
    )))));
    app
}

/// The Rules page drawn: `tests/snapshots/rules-tiny-skia.png`.
#[test]
fn the_rules_page_lists_the_patterns_the_rules_and_what_they_refused() {
    draw_sized(
        &rules_app(true),
        "rules",
        &[
            "Rules",
            "Let the Moebius agent write them",
            "Add a rule",
            "Built-in patterns",
            "no-force-push",
            "git push --force rewrites history on the remote",
            "3 refusals this week",
            "No refusals this week",
            "Rules in prose",
            "leave-ci-alone",
            "1 refusal this week",
            "no-vendoring",
            "radiance",
            "keep-migrations",
            "Saved as a commit on the default branch (.moebius/rules.md).",
        ],
        Size::new(1440.0, 1300.0),
    );
}

#[test]
fn a_server_without_a_rules_file_shows_its_rules_read_only() {
    let app = rules_app(false);
    assert!(shows(&app, "leave-ci-alone"));
    assert!(shows(&app, "Read only: this server has no rules file."));
    assert!(!shows(&app, "Save"));
    let app = rules_app(true);
    assert!(!shows(&app, "Read only: this server has no rules file."));
}

fn rules_typed(text: &str) -> rules::Message {
    rules::Message::TextEdited(text_editor::Action::Edit(text_editor::Edit::Paste(
        std::sync::Arc::new(text.to_string()),
    )))
}

fn rules_click(app: &App, id: &str) -> Vec<Message> {
    ids_sized(app, id, Size::new(1440.0, 1300.0))
}

fn rules_clicked(app: &App, label: &str) -> Vec<Message> {
    clicked_sized(app, label, Size::new(1440.0, 1300.0))
}

/// A rule of the server edited in its card: `tests/snapshots/rules-rule-edit-tiny-skia.png`.
#[test]
fn the_pencil_of_a_rule_grows_its_card_into_an_editor_of_its_name_and_text() {
    let mut app = rules_app(true);
    let messages = rules_click(&app, "edit-rule-no-vendoring");
    assert!(
        matches!(
            &messages[..],
            [Message::Rules(rules::Message::EditRule(rules::Place::Server, Some(name)))]
                if name == "no-vendoring"
        ),
        "{messages:?}"
    );
    update_all(&mut app, messages);
    let _ = app.update(Message::Rules(rules::Message::TextEdited(
        text_editor::Action::Move(text_editor::Motion::DocumentEnd),
    )));
    let _ = app.update(Message::Rules(rules_typed(" Use the package manager.")));
    draw_sized(
        &app,
        "rules-rule-edit",
        &[
            "leave-ci-alone",
            "Save",
            "Cancel",
            "Delete rule",
            "Add a rule",
        ],
        Size::new(1440.0, 1300.0),
    );
    assert!(matches!(
        &rules_clicked(&app, "Save")[..],
        [Message::Rules(rules::Message::SaveRule)]
    ));
    assert!(matches!(
        &rules_clicked(&app, "Cancel")[..],
        [Message::Rules(rules::Message::CancelRule)]
    ));
    let _ = app.update(Message::Rules(rules::Message::CancelRule));
    assert!(!shows(&app, "Delete rule"));
}

#[test]
fn a_rule_saved_in_its_card_rewrites_only_its_section_of_the_whole_file() {
    let settings = settings();
    let mut page = rules::Rules::default();
    let update = |page: &mut rules::Rules, message| page.update(message, Some(&settings)).units();
    update(&mut page, rules::Message::Loaded(Ok(rules_view(true))));
    update(
        &mut page,
        rules::Message::EditRule(rules::Place::Server, Some("leave-ci-alone".into())),
    );
    update(
        &mut page,
        rules::Message::TextEdited(text_editor::Action::Move(text_editor::Motion::DocumentEnd)),
    );
    update(&mut page, rules_typed(" Ask first."));
    assert_eq!(
        page.file_to_save(false).unwrap(),
        "Rules for every run.\n\n## leave-ci-alone\nDo not edit files under .github/ or any other CI configuration. Ask first.\n\n## no-vendoring\nDo not copy dependencies into the tree.\n"
    );
    assert_eq!(
        page.file_to_save(true).unwrap(),
        "Rules for every run.\n\n## no-vendoring\nDo not copy dependencies into the tree.\n",
        "Delete drops the section"
    );
    assert_eq!(update(&mut page, rules::Message::SaveRule), 1);
    let refused = ClientError::Server {
        status: 422,
        message: "line 3: the rule \"no-vendoring\" is named twice".into(),
    };
    update(&mut page, rules::Message::RuleSaved(Err(refused)));
    assert!(
        page.file_to_save(false).is_some(),
        "a refusal keeps the card open"
    );
    assert_eq!(
        update(
            &mut page,
            rules::Message::RuleSaved(Ok(Some("a1b2c3d".into())))
        ),
        1,
        "the rules are read again"
    );
    assert!(page.file_to_save(false).is_none());

    // A name with a space cannot be saved.
    update(
        &mut page,
        rules::Message::EditRule(rules::Place::Server, None),
    );
    update(&mut page, rules::Message::NameEdited("two words".into()));
    assert_eq!(update(&mut page, rules::Message::SaveRule), 0);
}

#[test]
fn a_refused_save_stays_red_in_the_card_and_a_saved_one_shows_its_commit() {
    let mut app = rules_app(true);
    let _ = app.update(Message::Rules(rules::Message::EditRule(
        rules::Place::Server,
        Some("no-vendoring".into()),
    )));
    let refused = ClientError::Server {
        status: 422,
        message: "line 4: the rule \"a\" is named twice".into(),
    };
    let _ = app.update(Message::Rules(rules::Message::RuleSaved(Err(refused))));
    assert!(shows(&app, "line 4: the rule \"a\" is named twice"));
    let _ = app.update(Message::Rules(rules::Message::RuleSaved(Ok(Some(
        "a1b2c3d".into(),
    )))));
    assert!(shows(&app, "Committed: a1b2c3d"));
    assert!(!shows(&app, "Delete rule"));
}

/// A new rule in the "New rule" card: `tests/snapshots/rules-editor-tiny-skia.png`.
#[test]
fn the_new_rule_card_adds_a_rule_at_the_end_of_the_file() {
    let mut app = rules_app(true);
    let messages = rules_click(&app, "new-rule");
    assert!(
        matches!(
            &messages[..],
            [Message::Rules(rules::Message::EditRule(
                rules::Place::Server,
                None
            ))]
        ),
        "{messages:?}"
    );
    update_all(&mut app, messages);
    let _ = app.update(Message::Rules(rules::Message::NameEdited(
        "ask-first".into(),
    )));
    let _ = app.update(Message::Rules(rules_typed("Ask before deleting a branch.")));
    draw_sized(
        &app,
        "rules-editor",
        &["ask-first", "Save", "Cancel"],
        Size::new(1440.0, 1300.0),
    );
    assert!(
        !shows(&app, "Delete rule"),
        "a new rule has nothing to delete"
    );

    let settings = settings();
    let mut page = rules::Rules::default();
    let _ = page.update(
        rules::Message::Loaded(Ok(rules_view(true))),
        Some(&settings),
    );
    let _ = page.update(
        rules::Message::EditRule(rules::Place::Server, None),
        Some(&settings),
    );
    let _ = page.update(
        rules::Message::NameEdited("ask-first".into()),
        Some(&settings),
    );
    let _ = page.update(rules_typed("Ask first."), Some(&settings));
    assert!(
        page.file_to_save(false)
            .unwrap()
            .ends_with("into the tree.\n\n## ask-first\nAsk first.\n")
    );
}

#[test]
fn deleting_a_rule_asks_first() {
    let mut app = rules_app(true);
    let _ = app.update(Message::Rules(rules::Message::EditRule(
        rules::Place::Server,
        Some("no-vendoring".into()),
    )));
    assert!(matches!(
        &rules_clicked(&app, "Delete rule")[..],
        [Message::Rules(rules::Message::AskDelete)]
    ));
    let _ = app.update(Message::Rules(rules::Message::AskDelete));
    assert!(shows(&app, "Delete this rule?"));
    assert!(matches!(
        &rules_clicked(&app, "Keep it")[..],
        [Message::Rules(rules::Message::KeepRule)]
    ));
    assert!(matches!(
        &rules_clicked(&app, "Delete")[..],
        [Message::Rules(rules::Message::DeleteRule)]
    ));
    assert_eq!(
        app.update(Message::Rules(rules::Message::DeleteRule))
            .units(),
        1
    );
}

/// A repository's rule edited in its card: `tests/snapshots/rules-repository-edit-tiny-skia.png`.
#[test]
fn a_repositorys_rules_are_cards_with_pencils_and_a_new_rule_card() {
    let mut app = rules_app(true);
    assert!(shows(&app, "Add a rule"));
    let messages = rules_click(&app, "edit-rule-1-keep-migrations");
    assert!(
        matches!(
            &messages[..],
            [Message::Rules(rules::Message::EditRule(rules::Place::Repository(1), Some(name)))]
                if name == "keep-migrations"
        ),
        "{messages:?}"
    );
    update_all(&mut app, messages);
    let refused = ClientError::Server {
        status: 422,
        message: ".moebius/rules.md: line 3: the rule \"keep-migrations\" is named twice".into(),
    };
    let _ = app.update(Message::Rules(rules::Message::RuleSaved(Err(refused))));
    draw_sized(
        &app,
        "rules-repository-edit",
        &[
            "radiance",
            ".moebius/rules.md: line 3: the rule \"keep-migrations\" is named twice",
            "Cancel",
        ],
        Size::new(1440.0, 1300.0),
    );
    assert!(
        matches!(
            &rules_click(&app, "new-rule-1")[..],
            [Message::Rules(rules::Message::EditRule(
                rules::Place::Repository(1),
                None
            ))]
        ),
        "the repository has its own New rule card"
    );
}

#[test]
fn a_server_without_a_rules_file_has_no_pencils_and_no_new_rule_card() {
    let app = rules_app(false);
    assert!(!shows(&app, "Let the Moebius agent write them"));
    let mut ui = simulator_sized(&app, Size::new(1440.0, 1300.0));
    assert!(
        ui.click(iced::widget::Id::from("edit-rule-no-vendoring".to_string()))
            .is_err()
    );
    assert!(
        ui.click(iced::widget::Id::from("new-rule".to_string()))
            .is_err()
    );
}

#[test]
fn the_moebius_agent_can_change_the_rules_in_a_chat() {
    let mut app = rules_app(true);
    let size = Size::new(1440.0, 1300.0);
    let clicked_on = |app: &App, label: &str| {
        let mut ui = simulator_sized(app, size);
        ui.click(label).unwrap();
        format!("{:?}", ui.into_messages().collect::<Vec<_>>())
    };
    assert_eq!(
        clicked_on(&app, "Let the Moebius agent write them"),
        "[Rules(ByAgent(true))]"
    );
    let _ = app.update(Message::Rules(rules::Message::ByAgent(true)));
    assert_eq!(clicked_on(&app, "Ask the agent"), "[]", "nothing said");
    let _ = app.update(Message::Rules(rules::Message::WishEdited(
        text_editor::Action::Edit(text_editor::Edit::Paste(std::sync::Arc::new(
            "never edit the lockfile".to_string(),
        ))),
    )));
    assert_eq!(clicked_on(&app, "Ask the agent"), "[Rules(AskAgent)]");
    draw_sized(&app, "rules-by-agent", &["Ask the agent", "Cancel"], size);
    // The chat the server started opens in the floating window, over the rules: not on the Chat page.
    let _ = app.update(Message::Rules(rules::Message::Asked(Ok(moebius_chat(9)))));
    assert_eq!(app.page(), Page::Rules);
    assert!(shows(&app, "Moebius agent"));
    assert!(shows(&app, "Open in Chat"));
}
