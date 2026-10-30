//! The tabs at the top of a project's page: Board, the tabs the person opened, the `+` with its menu of new tabs, and
//! at the right the button that folds the Files sidebar. The panel of "Go to file" or "Search in files" stands under
//! them while it is open.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::theme;
use crate::widgets::icons::Icon;
use crate::widgets::page;
use crate::widgets::tabs::{self, Tab as Cell};

use super::{Message, Project, Tab};

/// The label of a tab: Terminal and Chat are numbered when there are several of them.
fn name(project: &Project, tab: &Tab) -> String {
    let same = |other: &&Tab| std::mem::discriminant(*other) == std::mem::discriminant(tab);
    let count = project.opened.iter().filter(same).count();
    let place = project
        .opened
        .iter()
        .filter(same)
        .position(|other| other == tab);
    match (tab, place) {
        (Tab::Terminal(_) | Tab::Chat(_), Some(place)) if count > 1 => {
            format!("{} {}", tab.title(), place + 1)
        }
        _ => tab.title(),
    }
}

/// The identity of a tab among the widgets, which the tests click by.
fn key(tab: &Tab) -> String {
    match tab {
        Tab::Board => "Board".to_string(),
        Tab::Editor(path) => path.clone(),
        Tab::Terminal(n) => format!("terminal-{n}"),
        Tab::Chat(n) => format!("chat-{n}"),
    }
}

/// A tab of the strip.
fn cell<'a>(repository: i64, project: Option<&'a Project>, tab: &Tab) -> Element<'a, AppMessage> {
    let shown = project.map_or(Tab::Board, |project| project.tab.clone());
    let selected = *tab == shown;
    let send = |message| AppMessage::Project(repository, message);
    let label = match (project, tab) {
        (Some(project), tab) => name(project, tab),
        (None, tab) => tab.title(),
    };
    let unsaved = match (project, tab) {
        (Some(project), Tab::Editor(path)) => project.file(path).is_some_and(|file| file.unsaved()),
        _ => false,
    };
    Cell {
        id: format!("project-tab-{}", key(tab)),
        label: tabs::label(label, selected, matches!(tab, Tab::Editor(_))),
        selected,
        press: send(Message::Tab(tab.clone())),
        close: (*tab != Tab::Board).then(|| {
            let id = match tab {
                Tab::Editor(path) => format!("close-{path}"),
                other => format!("close-{}", key(other)),
            };
            (id, send(Message::Close(tab.clone())))
        }),
        mark: unsaved.then(tabs::unsaved_dot),
        tip: match tab {
            Tab::Editor(path) => Some(path.clone()),
            _ => None,
        },
    }
    .view()
}

/// A small square button with an icon, as the `+` and the fold button are.
fn square<'a>(
    icon: Icon,
    id: &'static str,
    active: bool,
    message: AppMessage,
) -> Element<'a, AppMessage> {
    container(
        button(container(icon.view(theme::TEXT_SECONDARY, theme::ICON)).center(Length::Fill))
            .width(theme::SQUARE_BUTTON)
            .height(theme::SQUARE_BUTTON)
            .padding(0)
            .on_press(message)
            .style(move |_, status| button::Style {
                background: (active || status == button::Status::Hovered)
                    .then(|| theme::TRACK.into()),
                border: Border {
                    radius: theme::RADIUS_CONTROL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    )
    .id(id)
    .into()
}

/// The items of the `+` menu, in the strip after the `+`: Editor, Terminal, Agent chat, and the browser, which comes
/// later.
fn menu<'a>(repository: i64) -> Element<'a, AppMessage> {
    let item = |id: &'static str, label: &'static str, message: Message| {
        container(page::white_button(
            label,
            Some(AppMessage::Project(repository, message)),
        ))
        .id(id)
    };
    row![
        item("new-editor", "Editor", Message::NewEditor),
        item("new-terminal", "Terminal", Message::NewTerminal),
        item("new-chat", "Agent chat", Message::NewChat),
        container(
            row![
                page::white_button("Browser", None::<AppMessage>),
                text("later")
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
            ]
            .spacing(theme::SPACE_6)
            .align_y(Alignment::Center),
        )
        .id("new-browser"),
    ]
    .spacing(theme::SPACE_6)
    .align_y(Alignment::Center)
    .into()
}

/// The tabs of a project's page, and under them the find panel when one is open.
pub fn tabs<'a>(repository: i64, project: Option<&'a Project>) -> Element<'a, AppMessage> {
    let send = |message| AppMessage::Project(repository, message);
    let menu_open = project.is_some_and(|project| project.menu);
    let folded = project.is_some_and(|project| project.files_folded);
    let mut strip = row![cell(repository, project, &Tab::Board)]
        .spacing(theme::SPACE_20)
        .align_y(Alignment::Start);
    for tab in project.map_or(&[][..], |project| project.opened()) {
        strip = strip.push(cell(repository, project, tab));
    }
    let mut plus = row![square(
        Icon::Plus,
        "project-new",
        menu_open,
        send(Message::Menu)
    )]
    .spacing(theme::SPACE_10)
    .align_y(Alignment::Center);
    if menu_open {
        plus = plus.push(menu(repository));
    }
    let bar = row![
        strip,
        plus,
        space::horizontal(),
        square(
            Icon::PanelRight,
            "toggle-files",
            !folded,
            send(Message::ToggleFiles)
        ),
    ]
    .spacing(theme::SPACE_12)
    .align_y(Alignment::Start);
    let mut page = column![column![bar, page::rule()]].spacing(theme::PAGE_GAP);
    if let Some(panel) = project.and_then(|project| project.find_view(repository)) {
        page = page.push(panel);
    }
    page.into()
}
