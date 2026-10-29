//! The sidebar and what opens from it: the repositories and the inbox count, the connect screen, the page that adds
//! a repository from the plus, and the search field with its list of matches.

use iced::{Settings, Size};
use iced_test::Simulator;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{self, ClientError, Snapshot};
use moebius_desktop::screens::add_repository;
use moebius_desktop::screens::agents;
use moebius_desktop::screens::chat;
use moebius_desktop::search;
use moebius_desktop::theme;
use serde_json::json;

use crate::agents::agent_listing;
use crate::chat::chat_listing;
use crate::fixtures::{
    board, card, clicked, draw, overview, pressed, radiance, repository, settings, shows,
    simulator, snapshot, update_all,
};

#[test]
fn the_expanded_sidebar_lists_the_repositories_and_the_board_is_named() {
    let app = App::with_snapshot(overview());
    draw(
        &app,
        "shell-expanded",
        &[
            "Online",
            "Inbox",
            "6",
            "Projects",
            "radiance",
            "3 live",
            "moebius",
            "1 live",
            "dotfiles",
            "Idle",
            "Build",
            "Agents",
            "Skills",
            "MCP servers",
            "Rules",
            "Improve",
            "Retro",
            "Experiments",
            "Decision model",
            "Usage",
            "Setup",
            "Models",
            "Plugins",
            "Settings",
            "GitHub · Code000-211/Radiance",
            "3 runs working · 1 change waits for you · default workflow",
        ],
    );
}

#[test]
fn the_collapsed_sidebar_marks_the_inbox_in_the_alert_colour() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::ToggleSidebar);
    draw(&app, "shell-collapsed", &["GitHub · Code000-211/Radiance"]);
}

#[test]
fn the_sidebar_says_online_when_the_stream_opens_before_the_saved_server_answers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("desktop.toml");
    let saved = settings();
    moebius_desktop::settings::save(&path, &saved).unwrap();
    let (mut app, _) = App::start(Some(path));
    let _ = app.update(Message::Stream(client::StreamEvent::Connected));
    let _ = app.update(Message::Checked(Ok((saved, overview()))));
    assert!(shows(&app, "Online"));
    assert!(!shows(&app, "Connecting"));
}

#[test]
fn an_app_without_repositories_opens_on_the_inbox() {
    let app = App::with_snapshot(Snapshot::default());
    assert_eq!(app.page(), Page::Inbox);
}

#[test]
fn the_connect_screen_says_why_the_connection_failed() {
    let (mut app, _) = App::start(None);
    let _ = app.update(Message::Checked(Err(ClientError::Unauthorized)));
    draw(
        &app,
        "connect-token-refused",
        &[
            "Connect to a server",
            "The token is not accepted.",
            "Connect",
        ],
    );
    let address = "http://10.0.0.5:7171".to_string();
    let _ = app.update(Message::Checked(Err(ClientError::Unreachable(address))));
    let mut ui = Simulator::with_size(
        Settings {
            fonts: theme::fonts(),
            ..Settings::default()
        },
        Size::new(1440.0, 900.0),
        app.view(),
    );
    assert!(ui.find("Cannot reach http://10.0.0.5:7171.").is_ok());
}

fn add_repository_app() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::AddRepository));
    app
}

fn typed(app: &mut App, messages: Vec<add_repository::Message>) {
    for message in messages {
        let _ = app.update(Message::AddRepository(message));
    }
}

#[test]
fn the_plus_of_the_sidebar_opens_the_page_that_adds_a_repository() {
    let app = App::with_snapshot(overview());
    assert_eq!(
        pressed(&app, "+").as_deref(),
        Some("[Select(AddRepository)]")
    );
    let app = add_repository_app();
    for text in ["Add a repository", "Name", "URL or path", "Forge", "None"] {
        assert!(shows(&app, text), "{text}");
    }
    assert!(!shows(&app, "Trusted authors"));
}

/// The page with the forge GitHub and a refusal: compare `tests/snapshots/add-repository-tiny-skia.png`.
#[test]
fn the_add_page_asks_for_the_github_repository_and_the_trusted_authors_and_shows_the_refusal() {
    let mut app = add_repository_app();
    typed(
        &mut app,
        vec![
            add_repository::Message::NameEdited("widgets".into()),
            add_repository::Message::UrlEdited("https://github.com/acme/widgets.git".into()),
            add_repository::Message::ForgeChosen(add_repository::Forge::GitHub),
            add_repository::Message::GithubEdited("acme/widgets".into()),
            add_repository::Message::TrustedEdited("leon, mona".into()),
            add_repository::Message::Answered(Err(ClientError::Server {
                status: 422,
                message: "the repository name \"widgets\" is taken".into(),
            })),
        ],
    );
    draw(
        &app,
        "add-repository",
        &[
            "Add a repository",
            "GitHub repository",
            "Trusted authors",
            "GitHub logins, comma separated. Their issues need no approval.",
            "Add",
            "the server answered 422: the repository name \"widgets\" is taken",
        ],
    );
}

#[test]
fn adding_needs_a_name_and_a_url_and_with_github_the_repository_and_sends_what_the_api_takes() {
    let mut app = add_repository_app();
    let add = |app: &App| pressed(app, "Add") == Some("[AddRepository(Add)]".into());
    assert!(!add(&app));
    typed(
        &mut app,
        vec![
            add_repository::Message::NameEdited("demo".into()),
            add_repository::Message::UrlEdited("/srv/git/demo".into()),
        ],
    );
    assert!(add(&app));
    typed(
        &mut app,
        vec![add_repository::Message::ForgeChosen(
            add_repository::Forge::GitHub,
        )],
    );
    assert!(!add(&app), "GitHub needs owner/name");
    typed(
        &mut app,
        vec![
            add_repository::Message::GithubEdited("acme/demo".into()),
            add_repository::Message::TrustedEdited(" leon, ,mona ".into()),
        ],
    );
    assert!(add(&app));

    let mut form = add_repository::AddRepository::default();
    for message in [
        add_repository::Message::NameEdited("demo".into()),
        add_repository::Message::UrlEdited("/srv/git/demo".into()),
    ] {
        let _ = form.update(message, None);
    }
    assert_eq!(
        serde_json::to_value(form.request().unwrap()).unwrap(),
        json!({"name": "demo", "url": "/srv/git/demo", "forge": "none"})
    );
    for message in [
        add_repository::Message::ForgeChosen(add_repository::Forge::GitHub),
        add_repository::Message::GithubEdited("acme/demo".into()),
        add_repository::Message::TrustedEdited(" leon, ,mona ".into()),
    ] {
        let _ = form.update(message, None);
    }
    assert_eq!(
        serde_json::to_value(form.request().unwrap()).unwrap(),
        json!({"name": "demo", "url": "/srv/git/demo", "forge": "github", "github": "acme/demo",
               "trusted": ["leon", "mona"]})
    );

    for message in [
        add_repository::Message::ForgeChosen(add_repository::Forge::GitLab),
        add_repository::Message::ProjectEdited(" acme/platform/demo ".into()),
    ] {
        let _ = form.update(message, None);
    }
    assert_eq!(
        serde_json::to_value(form.request().unwrap()).unwrap(),
        json!({"name": "demo", "url": "/srv/git/demo", "forge": "gitlab", "project": "acme/platform/demo",
               "trusted": ["leon", "mona"]})
    );

    for message in [
        add_repository::Message::ForgeChosen(add_repository::Forge::Forgejo),
        add_repository::Message::ProjectEdited(" acme/demo ".into()),
    ] {
        let _ = form.update(message, None);
    }
    assert_eq!(
        serde_json::to_value(form.request().unwrap()).unwrap(),
        json!({"name": "demo", "url": "/srv/git/demo", "forge": "forgejo", "project": "acme/demo",
               "trusted": ["leon", "mona"]})
    );
}

/// The page with the forge GitLab: compare `tests/snapshots/add-repository-gitlab-tiny-skia.png`.
#[test]
fn the_add_page_asks_for_the_gitlab_project_and_the_trusted_usernames() {
    let mut app = add_repository_app();
    typed(
        &mut app,
        vec![
            add_repository::Message::NameEdited("widgets".into()),
            add_repository::Message::UrlEdited(
                "https://gitlab.com/acme/platform/widgets.git".into(),
            ),
            add_repository::Message::ForgeChosen(add_repository::Forge::GitLab),
        ],
    );
    let add = |app: &App| pressed(app, "Add") == Some("[AddRepository(Add)]".into());
    assert!(!add(&app), "GitLab needs the project's path");
    typed(
        &mut app,
        vec![
            add_repository::Message::ProjectEdited("acme/platform/widgets".into()),
            add_repository::Message::TrustedEdited("leon".into()),
        ],
    );
    assert!(add(&app));
    assert!(!shows(&app, "GitHub repository"));
    draw(
        &app,
        "add-repository-gitlab",
        &[
            "GitLab",
            "GitLab project",
            "Trusted authors",
            "GitLab usernames, comma separated. Their issues need no approval.",
        ],
    );
}

/// The page with the forge Forgejo: compare `tests/snapshots/add-repository-forgejo-tiny-skia.png`.
#[test]
fn the_add_page_asks_for_the_forgejo_repository_and_the_trusted_logins() {
    let mut app = add_repository_app();
    typed(
        &mut app,
        vec![
            add_repository::Message::NameEdited("widgets".into()),
            add_repository::Message::UrlEdited("https://codeberg.org/acme/widgets.git".into()),
            add_repository::Message::ForgeChosen(add_repository::Forge::Forgejo),
        ],
    );
    let add = |app: &App| pressed(app, "Add") == Some("[AddRepository(Add)]".into());
    assert!(!add(&app), "Forgejo needs owner/name");
    typed(
        &mut app,
        vec![
            add_repository::Message::ProjectEdited("acme/widgets".into()),
            add_repository::Message::TrustedEdited("leon".into()),
        ],
    );
    assert!(add(&app));
    assert!(!shows(&app, "GitLab project"));
    draw(
        &app,
        "add-repository-forgejo",
        &[
            "Forgejo",
            "Forgejo repository",
            "Trusted authors",
            "Forgejo logins, comma separated. Their issues need no approval.",
        ],
    );
}

/// Two boards, chats and agents, with `text` typed into the search field.
fn searching(text: &str) -> App {
    let moebius = board(
        repository(2, "moebius", None),
        vec![
            card(60, 2, "Backlog", "Retry the usage report"),
            card(61, 2, "Ready", "Write the retro page"),
        ],
    );
    let mut app = App::with_snapshot(snapshot(vec![radiance("Ready"), moebius], vec![]));
    let _ = app.update(Message::Chat(chat::Message::Listed(Ok(chat_listing(
        "waiting",
    )))));
    let _ = app.update(Message::Agents(agents::Message::Listed(
        Ok(agent_listing()),
    )));
    let _ = app.update(Message::Search(search::Message::Edited(text.into())));
    app
}

fn key_pressed(key: iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> iced::Event {
    iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

/// The list of matches stands under the field, over the board: compare `tests/snapshots/search-tiny-skia.png`.
#[test]
fn the_list_of_matches_is_drawn_under_the_field_over_the_board() {
    let app = searching("retr");
    draw(
        &app,
        "search",
        &[
            "Cards",
            "#50 Retry model downloads on HTTP 5xx",
            "#60 Retry the usage report",
            "moebius · Backlog",
            "Pages",
            "Retro",
            "Ask the Moebius agent: retr",
        ],
    );
}

/// Typing a title of another board and pressing Enter opens that card, and the field is empty again.
#[test]
fn enter_opens_a_card_of_another_board_and_clears_the_field() {
    let mut app = searching("usage report");
    assert!(shows(&app, "#60 Retry the usage report"));
    assert!(matches!(app.page(), Page::Board(1)));
    let mut ui = simulator(&app);
    ui.click("usage report").unwrap();
    ui.tap_key(iced::keyboard::key::Named::Enter);
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(matches!(
        &messages[..],
        [Message::Search(search::Message::Submit)]
    ));
    update_all(&mut app, messages);
    assert!(matches!(app.page(), Page::Board(2)));
    assert!(shows(&app, "Retry the usage report"));
    assert!(!shows(&app, "Ask the Moebius agent: usage report"));
    assert!(shows(&app, "Search or run…"));
}

/// `#48` finds the card by its number, and a click on the entry opens its live view because its run is working.
#[test]
fn a_number_finds_the_card_and_a_click_opens_its_live_view() {
    let mut app = searching("#48");
    let label = "#48 MTP drafter: cap the candidate tree depth";
    assert!(shows(&app, label));
    assert!(shows(&app, "radiance · In progress"));
    let messages = clicked(&app, label);
    assert!(matches!(
        &messages[..],
        [Message::Search(search::Message::Pick(0))]
    ));
    update_all(&mut app, messages);
    assert!(shows(&app, "This run"));
}

/// The arrows move the selection; a chat, an agent and a page open what they are.
#[test]
fn the_arrows_move_the_selection_and_chats_agents_and_pages_open() {
    // Entries for "co": the agent Coder, then the pages with "co" in their title, then the ask entry.
    let mut app = searching("coder");
    let _ = app.update(Message::Search(search::Message::Move(
        search::Direction::Down,
    )));
    let _ = app.update(Message::Search(search::Message::Move(
        search::Direction::Up,
    )));
    let _ = app.update(Message::Search(search::Message::Submit));
    assert!(matches!(app.page(), Page::Agents));

    let mut app = searching("ready.luau");
    let _ = app.update(Message::Search(search::Message::Submit));
    assert!(matches!(app.page(), Page::Chat));

    let mut app = searching("usage");
    // The card of the moebius board, the chat of the same words in no title, then the page.
    for _ in 0..1 {
        let _ = app.update(Message::Search(search::Message::Move(
            search::Direction::Down,
        )));
    }
    let _ = app.update(Message::Search(search::Message::Submit));
    assert!(matches!(app.page(), Page::Usage));
}

/// The ask entry sends the first message to the Moebius agent, and the chat opens when the server has started it.
#[test]
fn the_ask_entry_starts_a_chat_and_a_failure_keeps_the_text() {
    let mut app = searching("how do I add a rule");
    let label = "Ask the Moebius agent: how do I add a rule";
    let messages = clicked(&app, label);
    assert!(matches!(
        &messages[..],
        [Message::Search(search::Message::Pick(_))]
    ));
    let mut sent = 0;
    for message in messages {
        sent += app.update(message).units();
    }
    assert_eq!(sent, 1, "one request starts the chat");
    assert!(!shows(&app, label), "the field is cleared");

    let run = serde_json::from_value(json!({"id": 8, "agent": "Moebius", "purpose": "chat",
                                            "status": "running"}))
    .unwrap();
    let _ = app.update(Message::Search(search::Message::Started(
        "how do I add a rule".into(),
        Ok(run),
    )));
    assert!(matches!(app.page(), Page::Chat));

    let mut app = searching("how");
    let _ = app.update(Message::Search(search::Message::Submit));
    let _ = app.update(Message::Search(search::Message::Started(
        "how".into(),
        Err(ClientError::Unreachable("http://x".into())),
    )));
    assert!(shows(&app, "Ask the Moebius agent: how"));
    assert!(shows(&app, "cannot reach http://x"));
}

/// Escape and an empty field close the list; Ctrl+K and the arrows are what the keys say.
#[test]
fn escape_or_clearing_the_field_closes_the_list() {
    let mut app = searching("usage");
    assert!(shows(&app, "Ask the Moebius agent: usage"));
    let escape = key_pressed(
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        iced::keyboard::Modifiers::empty(),
    );
    let message = search::key(
        escape,
        iced::event::Status::Ignored,
        iced::window::Id::unique(),
    );
    assert!(matches!(
        message,
        Some(Message::Search(search::Message::Close))
    ));
    let _ = app.update(message.unwrap());
    assert!(!shows(&app, "Ask the Moebius agent: usage"));

    let mut app = searching("usage");
    let _ = app.update(Message::Search(search::Message::Edited(String::new())));
    assert!(!shows(&app, "Ask the Moebius agent: "));

    let ctrl_k = key_pressed(
        iced::keyboard::Key::Character("k".into()),
        iced::keyboard::Modifiers::COMMAND,
    );
    assert!(matches!(
        search::key(
            ctrl_k,
            iced::event::Status::Captured,
            iced::window::Id::unique()
        ),
        Some(Message::Search(search::Message::Focus))
    ));
}

/// In the collapsed sidebar the search icon opens the sidebar, whose field shows.
#[test]
fn the_collapsed_sidebars_search_opens_the_sidebar() {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::ToggleSidebar);
    assert!(!shows(&app, "Search or run…"));
    let _ = app.update(Message::Search(search::Message::Focus));
    assert!(shows(&app, "Search or run…"));
}
