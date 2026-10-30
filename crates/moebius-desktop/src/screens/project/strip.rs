//! The tabs at the top of a project's page: Board, the tabs the person opened, the `+` with the menu of new tabs it
//! drops down, and at the right the button that folds the Files sidebar. The panel of "Go to file" or "Search in files" stands under
//! them while it is open.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::theme;
use crate::widgets::dropdown::Dropdown;
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

/// One row of the `+` menu: the whole width is the button; without a message it is drawn muted, with an aside.
fn menu_item<'a>(
    id: &'static str,
    label: &'static str,
    aside: Option<&'static str>,
    message: Option<AppMessage>,
) -> Element<'a, AppMessage> {
    let color = if message.is_some() {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    let mut line = row![
        text(label).size(theme::BODY).color(color),
        space::horizontal()
    ]
    .align_y(Alignment::Center);
    if let Some(aside) = aside {
        line = line.push(
            text(aside)
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
        );
    }
    container(
        button(line)
            .width(Length::Fill)
            .padding(theme::CONTROL_PADDING)
            .on_press_maybe(message)
            .style(|_, status| button::Style {
                background: (status == button::Status::Hovered).then(|| theme::HEAD_PILL.into()),
                border: Border {
                    radius: theme::RADIUS_SMALL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    )
    .id(id)
    .into()
}

/// The menu that drops down from the `+`: Editor, Terminal, Agent chat, and the browser, which comes later.
fn menu<'a>(repository: i64) -> Element<'a, AppMessage> {
    let send = |message| Some(AppMessage::Project(repository, message));
    container(
        column![
            menu_item("new-editor", "Editor", None, send(Message::NewEditor)),
            menu_item("new-terminal", "Terminal", None, send(Message::NewTerminal)),
            menu_item("new-chat", "Agent chat", None, send(Message::NewChat)),
            menu_item("new-browser", "Browser", Some("later"), None),
        ]
        .spacing(theme::SPACE_2),
    )
    .width(theme::DROPDOWN_WIDTH)
    .padding(theme::SPACE_4)
    .style(|_| container::Style {
        shadow: theme::shadow(),
        ..page::box_style()
    })
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
    // The menu closes again with a click beside it, as the `+` toggles it.
    let plus = Dropdown::new(
        square(Icon::Plus, "project-new", menu_open, send(Message::Menu)),
        menu(repository),
        menu_open,
        send(Message::Menu),
    );
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
