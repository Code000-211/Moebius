//! Going back: the link back to the page an editor or a card was opened from, Alt+Left, the mouse's back button and
//! Escape, and the sidebar that starts the way back again.

use iced::widget::text_editor;
use moebius_desktop::app::{Message, Page};
use moebius_desktop::screens::project;
use moebius_desktop::search;
use moebius_desktop::widgets::script_editor;

use crate::board::{HOLD, script, script_app};
use crate::fixtures::{clicked, radiance_app, shows};
use crate::project::project;

/// The pencil opens the script's editor under a link back to the board; going back shows the board's tab, and nothing
/// is left to go back to.
#[test]
fn the_script_editor_goes_back_to_the_board_it_was_opened_from() {
    let mut app = script_app("repository");
    assert!(shows(&app, "\u{2190} Back to radiance"));
    assert!(matches!(
        &clicked(&app, "\u{2190} Back to radiance")[..],
        [Message::Back]
    ));
    let _ = app.update(Message::Back);
    assert_eq!(app.page(), Page::Board(1));
    assert!(!shows(&app, "Use the built-in script"));
    assert!(shows(&app, "Retry model downloads on HTTP 5xx"));
    assert!(!shows(&app, "\u{2190} Back to radiance"));
}

/// A card opened from another tab of its project goes back to that tab, and a card opened from the Inbox to the Inbox.
#[test]
fn going_back_from_a_card_restores_the_tab_or_page_it_was_opened_from() {
    let mut app = radiance_app();
    let _ = app.update(project(project::Message::Tab(project::Tab::Editor)));
    let _ = app.update(Message::OpenCard(48));
    assert!(shows(&app, "\u{2190} Back to Editor of radiance"));
    let _ = app.update(Message::Back);
    assert!(shows(&app, "No file is open. Open one from Files."));

    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::OpenCard(48));
    assert!(shows(&app, "MTP drafter: cap the candidate tree depth"));
    assert!(shows(&app, "\u{2190} Back to Inbox"));
    let _ = app.update(Message::Back);
    assert_eq!(app.page(), Page::Inbox);

    // The breadcrumb back to the board is a way back too: nothing is left to go back to after it.
    let _ = app.update(project(project::Message::Tab(project::Tab::Board)));
    let _ = app.update(Message::Select(Page::Board(1)));
    let _ = app.update(Message::OpenCard(48));
    let _ = app.update(Message::CloseCard);
    assert!(!shows(&app, "\u{2190} Back to #48"));
    assert!(!shows(&app, "\u{2190} Back to radiance"));
}

/// Alt+Left and the mouse's back button go back; Escape in an editor goes back when nothing in it is unsaved.
#[test]
fn alt_left_the_mouse_and_escape_in_a_saved_editor_go_back() {
    let window = iced::window::Id::unique();
    let alt_left = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft),
        modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::ALT,
        text: None,
        repeat: false,
    });
    let ignored = iced::event::Status::Ignored;
    assert!(matches!(
        moebius_desktop::app::back_key(alt_left.clone(), ignored, window),
        Some(Message::Back)
    ));
    // A text field that moves by words with Alt+Left keeps it.
    let captured = iced::event::Status::Captured;
    assert!(moebius_desktop::app::back_key(alt_left, captured, window).is_none());
    let mouse_back =
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Back));
    assert!(matches!(
        moebius_desktop::app::back_key(mouse_back, ignored, window),
        Some(Message::Back)
    ));

    // Escape is the search's key; with the search closed it goes back from the editor.
    let mut app = script_app("repository");
    let _ = app.update(Message::Search(search::Message::Close));
    assert!(!shows(&app, "Use the built-in script"));
    assert_eq!(app.page(), Page::Board(1));
}

/// Escape keeps an editor with unsaved text open, and after a save it goes back.
#[test]
fn escape_keeps_an_editor_with_unsaved_text() {
    let mut app = script_app("repository");
    let _ = app.update(Message::Script(script_editor::Message::Text(
        text_editor::Action::Edit(text_editor::Edit::Insert('x')),
    )));
    let _ = app.update(Message::Search(search::Message::Close));
    assert!(shows(&app, "Use the built-in script"));
    let _ = app.update(Message::Script(script_editor::Message::Saved(Ok(script(
        "repository",
        HOLD,
    )))));
    let _ = app.update(Message::Search(search::Message::Close));
    assert!(!shows(&app, "Use the built-in script"));
}

/// A pick of the sidebar starts the way back again: the page it opens has no link back.
#[test]
fn a_pick_of_the_sidebar_forgets_the_way_back() {
    let mut app = script_app("repository");
    let _ = app.update(Message::Select(Page::Inbox));
    assert!(!shows(&app, "\u{2190} Back to radiance"));
    let _ = app.update(Message::Select(Page::Board(1)));
    assert!(!shows(&app, "\u{2190} Back to Inbox"));
    let _ = app.update(Message::Back);
    assert_eq!(app.page(), Page::Board(1));
}

/// A link on a page to another page goes back to it, and a search result too.
#[test]
fn a_link_or_a_search_result_to_another_page_goes_back() {
    let mut app = radiance_app();
    let _ = app.update(Message::Select(Page::Inbox));
    let _ = app.update(Message::Visit(Page::Models));
    assert!(shows(&app, "\u{2190} Back to Inbox"));
    let _ = app.update(Message::Search(search::Message::Edited("#48".into())));
    let _ = app.update(Message::Search(search::Message::Submit));
    assert!(shows(&app, "\u{2190} Back to Models"));
    let _ = app.update(Message::Back);
    assert_eq!(app.page(), Page::Models);
    let _ = app.update(Message::Back);
    assert_eq!(app.page(), Page::Inbox);
}
