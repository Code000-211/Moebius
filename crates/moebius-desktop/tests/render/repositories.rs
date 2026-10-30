//! The Projects page: the overview the "Projects" heading of the sidebar opens, every repository with the board it
//! opens, its removal behind a question, and what the screen shows while the server takes one away.

use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::screens::repositories;

use crate::fixtures::{board, card, clicked, draw, repository, shows, snapshot, update_all};

/// Two repositories: one on GitHub with a card, one plain and empty.
fn two_repositories() -> App {
    App::with_snapshot(snapshot(
        vec![
            board(
                repository(1, "radiance", Some("acme/radiance")),
                vec![card(1, 1, "Ready", "Ship it")],
            ),
            board(repository(2, "moebius", None), vec![]),
        ],
        vec![],
    ))
}

#[test]
fn the_projects_heading_opens_the_overview_of_every_repository() {
    let mut app = two_repositories();
    let messages = clicked(&app, "Projects");
    assert!(
        matches!(messages.as_slice(), [Message::Select(Page::Repositories)]),
        "the heading sends {messages:?}"
    );
    update_all(&mut app, messages);
    draw(
        &app,
        "projects",
        &[
            "Every repository Moebius has; open one, or remove it with the work stored for it.",
            "radiance",
            "GitHub · acme/radiance · 1 card",
            "moebius",
            "No forge · 0 cards",
            "Delete",
            "Add a repository",
        ],
    );
}

#[test]
fn removing_a_repository_asks_first_and_keep_leaves_it() {
    let mut app = two_repositories();
    update_all(&mut app, vec![Message::Select(Page::Repositories)]);
    let messages = clicked(&app, "Delete");
    update_all(&mut app, messages);
    assert!(shows(&app, "Delete radiance with its cards and chats?"));
    let messages = clicked(&app, "Keep");
    update_all(&mut app, messages);
    assert!(!shows(&app, "Delete radiance with its cards and chats?"));
    assert!(shows(&app, "Delete"));
}

#[test]
fn the_question_asks_the_server_to_remove_the_repository() {
    let mut app = two_repositories();
    update_all(&mut app, vec![Message::Select(Page::Repositories)]);
    let messages = clicked(&app, "Delete");
    update_all(&mut app, messages);
    let messages = clicked(&app, "Delete");
    assert!(
        matches!(
            messages.as_slice(),
            [Message::Repositories(repositories::Message::Remove(1))]
        ),
        "the confirmation sends {messages:?}"
    );
}
