//! The sidebar on every screen: the app name with the connection, the search field, the Inbox, Chat, Fleet and
//! Automations, and the groups Projects, Build, Improve and Setup. It collapses to a rail of icons. Every number in it
//! comes from the server.

use iced::font::Weight;
use iced::widget::{button, column, container, row, scrollable, space, text, text_input, tooltip};
use iced::{Border, Center, Color, Element, Length, Padding};

use crate::app::{Connection, Message, Page};
use crate::client::Snapshot;
use crate::screens::usage::short;
use crate::search;
use crate::theme;
use crate::widgets::icons::{self, Icon};

/// The groups below the Inbox and what each holds.
const BUILD: [(Page, Icon); 4] = [
    (Page::Agents, Icon::Agents),
    (Page::Skills, Icon::Skills),
    (Page::McpServers, Icon::McpServers),
    (Page::Rules, Icon::Rules),
];
const IMPROVE: [(Page, Icon); 4] = [
    (Page::Retro, Icon::Retro),
    (Page::Experiments, Icon::Experiments),
    (Page::DecisionModel, Icon::DecisionModel),
    (Page::Usage, Icon::Usage),
];
const SETUP: [(Page, Icon); 5] = [
    (Page::Models, Icon::Models),
    (Page::Plugins, Icon::Plugins),
    (Page::Terminal, Icon::Terminal),
    (Page::Ports, Icon::Ports),
    (Page::Settings, Icon::Settings),
];

/// What leads an item: an icon, or the dot of a board.
enum Lead {
    Icon(Icon),
    Dot(Color),
}

/// What stands at the right of an item.
enum Trailing {
    Nothing,
    Text(String),
    Badge(usize),
}

/// The server the app is connected to and the ones it can switch to, shown under the app name when there is more than
/// one server or the connection is over SSH.
#[derive(Debug, Clone, Default)]
pub struct ServerMenu {
    /// The name of the server in use; none when the header names nothing.
    pub name: Option<String>,
    /// The servers remembered, by name, in order.
    pub servers: Vec<String>,
    pub open: bool,
}

/// What the sidebar tells besides the boards.
pub struct Counts {
    pub chats_unread: usize,
    /// How many runs and chats are working.
    pub working: usize,
    /// The tokens used today and the day's budget, once the usage is read.
    pub tokens_today: Option<(i64, Option<i64>)>,
    /// Whether a step of Get started is missing, which makes the page listed.
    pub setup_incomplete: bool,
    pub menu: ServerMenu,
}

pub fn view<'a>(
    snapshot: &'a Snapshot,
    connection: Connection,
    page: &Page,
    counts: Counts,
    collapsed: bool,
    search: &str,
) -> Element<'a, Message> {
    let Counts {
        chats_unread,
        working,
        tokens_today,
        setup_incomplete,
        menu,
    } = counts;
    let inbox = item(
        Lead::Icon(Icon::Inbox),
        "Inbox",
        Trailing::Badge(snapshot.inbox.len()),
        Page::Inbox,
        page,
        collapsed,
    );
    let chat = item(
        Lead::Icon(Icon::Chat),
        Page::Chat.title(),
        if chats_unread > 0 {
            Trailing::Badge(chats_unread)
        } else {
            Trailing::Nothing
        },
        Page::Chat,
        page,
        collapsed,
    );
    let fleet = item(
        Lead::Icon(Icon::Fleet),
        Page::Fleet.title(),
        if working > 0 {
            Trailing::Text(format!("{working} live"))
        } else {
            Trailing::Nothing
        },
        Page::Fleet,
        page,
        collapsed,
    );
    let automations = item(
        Lead::Icon(Icon::Automations),
        Page::Automations.title(),
        Trailing::Nothing,
        Page::Automations,
        page,
        collapsed,
    );
    let boards = snapshot.boards.iter().map(|board| {
        let repository = &board.repository;
        let live = board.live_runs();
        let (dot, trailing) = if live > 0 {
            (theme::TEAL, format!("{live} live"))
        } else {
            (theme::LINE, "Idle".to_string())
        };
        item(
            Lead::Dot(dot),
            &repository.name,
            Trailing::Text(trailing),
            Page::Board(repository.id),
            page,
            collapsed,
        )
    });
    let listed = |name: &'a str, pages: &[(Page, Icon)]| {
        group(
            name,
            page,
            pages
                .iter()
                .map(|(target, icon)| {
                    item(
                        Lead::Icon(*icon),
                        target.title(),
                        Trailing::Nothing,
                        *target,
                        page,
                        collapsed,
                    )
                })
                .collect(),
            collapsed,
        )
    };
    // Get started leads the Setup group while a step is missing.
    let setup: Vec<(Page, Icon)> = setup_incomplete
        .then_some((Page::Start, Icon::Start))
        .into_iter()
        .chain(SETUP)
        .collect();
    let groups = column![
        group(None, page, vec![inbox, chat, fleet, automations], collapsed),
        divider(),
        group_with(
            Some("Projects"),
            Some(Page::Repositories),
            page,
            Some(add_board(page)),
            boards.collect(),
            collapsed,
        ),
        divider(),
        listed("Build", &BUILD),
        divider(),
        listed("Improve", &IMPROVE),
        divider(),
        listed("Setup", &setup),
    ];
    let width = if collapsed {
        theme::RAIL_WIDTH
    } else {
        theme::SIDEBAR_WIDTH
    };
    let sidebar = container(
        column![
            top(connection, collapsed, search, menu),
            scrollable(groups).height(Length::Fill),
            tokens_meter(tokens_today, collapsed),
        ]
        .height(Length::Fill),
    )
    .width(Length::Fixed(width))
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(theme::SIDEBAR.into()),
        ..container::Style::default()
    });
    row![sidebar, rule_vertical()].into()
}

/// The tokens used today at the foot: with the day's budget a number of both and a thin bar, without one the tokens
/// alone. Nothing stands there until the usage is read.
fn tokens_meter<'a>(
    tokens_today: Option<(i64, Option<i64>)>,
    collapsed: bool,
) -> Element<'a, Message> {
    let Some((tokens, budget)) = tokens_today else {
        return space().height(theme::SPACE_8).into();
    };
    let shown = match budget {
        Some(budget) => format!("{} / {}", short(tokens), short(budget)),
        None => short(tokens),
    };
    let mut meter = column![].spacing(theme::SPACE_6);
    if !collapsed {
        meter = meter.push(
            row![
                text("Tokens today")
                    .size(theme::BODY)
                    .color(theme::SIDEBAR_TEXT),
                space::horizontal(),
                text(shown)
                    .size(theme::GROUP_LABEL)
                    .font(theme::MONO)
                    .color(theme::SIDEBAR_TEXT),
            ]
            .align_y(Center),
        );
    }
    if let Some(budget) = budget.filter(|budget| *budget > 0) {
        let used = (tokens.max(0) * 1000 / budget).clamp(0, 1000) as u16;
        let part = |portion: u16, color: Color| {
            container(space())
                .width(Length::FillPortion(portion))
                .height(theme::BAR)
                .style(move |_| container::Style {
                    background: Some(color.into()),
                    border: Border {
                        radius: theme::RADIUS_SEGMENT.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                })
        };
        let mut bar = row![].width(Length::Fill);
        if used > 0 {
            bar = bar.push(part(used, theme::TEAL));
        }
        if used < 1000 {
            bar = bar.push(part(1000 - used, theme::TRACK));
        }
        meter = meter.push(bar);
    }
    column![
        divider(),
        container(meter).width(Length::Fill).padding(Padding {
            top: theme::SPACE_12,
            right: if collapsed {
                theme::SPACE_12
            } else {
                theme::SPACE_16
            },
            bottom: theme::SPACE_12,
            left: if collapsed {
                theme::SPACE_12
            } else {
                theme::SPACE_16
            },
        })
    ]
    .into()
}

fn rule_vertical<'a>() -> Element<'a, Message> {
    container(space().width(0))
        .width(theme::HAIRLINE)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::SIDEBAR_LINE.into()),
            ..container::Style::default()
        })
        .into()
}

fn divider<'a>() -> Element<'a, Message> {
    container(column![].height(theme::HAIRLINE))
        .width(Length::Fill)
        .height(theme::HAIRLINE)
        .style(|_| container::Style {
            background: Some(theme::SIDEBAR_LINE.into()),
            ..container::Style::default()
        })
        .into()
}

/// The app name with the connection, the toggle and the search field.
fn top<'a>(
    connection: Connection,
    collapsed: bool,
    search: &str,
    menu: ServerMenu,
) -> Element<'a, Message> {
    let (state, color, text_color) = match connection {
        Connection::Online => ("Online", theme::TEAL, theme::ONLINE_TEXT),
        Connection::Connecting => ("Connecting", theme::TEXT_MUTED, theme::TEXT_MUTED),
        Connection::Offline => ("Offline", theme::AMBER, theme::AMBER),
        Connection::Reconnecting => ("Reconnecting…", theme::AMBER, theme::AMBER),
    };
    // With a server named, its line says the state, and the first row has room for the toggle only.
    let named = menu.name.is_some() && !collapsed;
    let label = if collapsed {
        "Expand the sidebar"
    } else {
        "Collapse the sidebar"
    };
    let toggle = tooltip(
        button(
            container(Icon::Panel.view(theme::TEXT_SECONDARY, theme::ICON_SMALL))
                .center(Length::Fill),
        )
        .width(theme::SQUARE_BUTTON)
        .height(theme::SQUARE_BUTTON)
        .padding(0)
        .on_press(Message::ToggleSidebar)
        .style(|_, _| button::Style {
            background: Some(theme::SURFACE.into()),
            border: Border {
                color: theme::SIDEBAR_LINE,
                width: 1.0,
                radius: theme::RADIUS_SMALL.into(),
            },
            ..button::Style::default()
        }),
        text(label).size(theme::LABEL),
        tooltip::Position::Bottom,
    );
    let mark_row: Element<'a, Message> = if collapsed {
        column![icons::logo(), toggle]
            .spacing(theme::SPACE_10)
            .align_x(Center)
            .into()
    } else {
        let mut head = row![
            icons::logo(),
            text("Moebius")
                .size(theme::APP_NAME)
                .font(theme::manrope(Weight::Bold)),
            space::horizontal(),
        ];
        if !named {
            let (dot, label) = state_parts(state, color, text_color);
            head = head.push(dot).push(label);
        }
        head.push(toggle)
            .spacing(theme::SPACE_8)
            .align_y(Center)
            .padding(Padding::from([0.0, theme::SPACE_4]))
            .into()
    };
    let mark_row: Element<'a, Message> = match (menu.name.clone(), named) {
        (Some(name), true) => column![
            mark_row,
            server_button(name, menu, state, color, text_color)
        ]
        .spacing(theme::SPACE_8)
        .into(),
        _ => mark_row,
    };
    let field = text_input("Search or run…", search)
        .id(search::FIELD)
        .on_input(|text| Message::Search(search::Message::Edited(text)))
        .on_submit(Message::Search(search::Message::Submit))
        .size(theme::CONTEXT)
        .padding(0)
        .style(|_, _| text_input::Style {
            background: Color::TRANSPARENT.into(),
            border: Border::default(),
            icon: theme::TEXT_MUTED,
            placeholder: theme::TEXT_MUTED,
            value: theme::TEXT,
            selection: theme::HEAD_PILL,
        });
    let search: Element<'a, Message> = if collapsed {
        button(container(Icon::Search.view(theme::TEXT_MUTED, theme::ICON)).center_x(Length::Fill))
            .width(Length::Fill)
            .padding([theme::SPACE_8, 0.0])
            .on_press(Message::Search(search::Message::Focus))
            .style(|_, _| button::Style::default())
            .into()
    } else {
        container(
            row![Icon::Search.view(theme::TEXT_MUTED, theme::ICON), field]
                .spacing(theme::SPACE_8)
                .align_y(Center),
        )
        .width(Length::Fill)
        .padding([theme::SPACE_8, theme::SPACE_10])
        .style(|_| container::Style {
            background: Some(theme::SURFACE.into()),
            border: Border {
                color: theme::SIDEBAR_LINE,
                width: 1.0,
                radius: theme::RADIUS_CONTROL.into(),
            },
            ..container::Style::default()
        })
        .into()
    };
    container(column![mark_row, search].spacing(theme::SPACE_12))
        .padding(Padding {
            top: theme::SPACE_12,
            right: theme::SPACE_12,
            bottom: theme::SPACE_8,
            left: theme::SPACE_12,
        })
        .into()
}

/// A dot and the state of the connection.
fn state_line<'a>(state: &'a str, color: Color, text_color: Color) -> Element<'a, Message> {
    let (dot, label) = state_parts(state, color, text_color);
    row![dot, label]
        .spacing(theme::SPACE_8)
        .align_y(Center)
        .into()
}

fn state_parts<'a>(
    state: &'a str,
    color: Color,
    text_color: Color,
) -> (Element<'a, Message>, Element<'a, Message>) {
    (
        container(space().width(theme::DOT).height(theme::DOT))
            .style(move |_| container::Style {
                background: Some(color.into()),
                border: Border {
                    radius: (theme::DOT / 2.0).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into(),
        text(state)
            .size(theme::LABEL)
            .font(theme::MONO)
            .color(text_color)
            .into(),
    )
}

/// The server in use with the state of the connection under its name; a click lists the servers to switch to.
fn server_button<'a>(
    name: String,
    menu: ServerMenu,
    state: &'a str,
    color: Color,
    text_color: Color,
) -> Element<'a, Message> {
    let head = button(
        row![
            column![
                text(name.clone())
                    .size(theme::CODE_SMALL)
                    .font(theme::MONO)
                    .color(theme::TEXT),
                state_line(state, color, text_color),
            ]
            .spacing(theme::SPACE_2),
            space::horizontal(),
            if menu.open {
                Icon::ChevronDown.view(theme::TEXT_MUTED, theme::ICON_SMALL)
            } else {
                Icon::ChevronRight.view(theme::TEXT_MUTED, theme::ICON_SMALL)
            },
        ]
        .align_y(Center),
    )
    .width(Length::Fill)
    .padding(theme::FIELD_PADDING)
    .on_press(Message::ToggleServers)
    .style(|_, status| button::Style {
        background: Some(
            if status == button::Status::Hovered {
                theme::SIDEBAR_HOVER
            } else {
                theme::SURFACE
            }
            .into(),
        ),
        text_color: theme::TEXT,
        border: Border {
            color: theme::SIDEBAR_LINE,
            width: theme::HAIRLINE,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..button::Style::default()
    });
    if !menu.open {
        return head.into();
    }
    let entry = |label: String, current: bool, message: Message| {
        button(
            text(label)
                .size(theme::CODE_SMALL)
                .font(theme::MONO)
                .color(theme::TEXT),
        )
        .width(Length::Fill)
        .padding(theme::FIELD_PADDING)
        .on_press(message)
        .style(move |_, status| button::Style {
            background: (current || status == button::Status::Hovered)
                .then(|| theme::SIDEBAR_HOVER.into()),
            text_color: theme::TEXT,
            border: Border {
                radius: theme::RADIUS_CONTROL.into(),
                ..Border::default()
            },
            ..button::Style::default()
        })
    };
    let mut list = column![head].spacing(theme::SPACE_2);
    for (index, server) in menu.servers.into_iter().enumerate() {
        let current = server == name;
        list = list.push(entry(server, current, Message::SwitchServer(index)));
    }
    list.push(entry(
        "Add a server…".to_string(),
        false,
        Message::ChangeConnection,
    ))
    .into()
}

/// A group of items under a bold name, which a collapsed sidebar does not show.
fn group<'a>(
    name: impl Into<Option<&'a str>>,
    current: &Page,
    items: Vec<Element<'a, Message>>,
    collapsed: bool,
) -> Element<'a, Message> {
    group_with(name, None, current, None, items, collapsed)
}

/// The "+" at the right of the name of Projects, which opens the page that adds a repository.
fn add_board<'a>(current: &Page) -> Element<'a, Message> {
    let selected = *current == Page::AddRepository;
    tooltip(
        button(
            text("+")
                .size(theme::BODY)
                .font(theme::manrope(Weight::Bold)),
        )
        .padding([0.0, theme::SPACE_8])
        .on_press(Message::Select(Page::AddRepository))
        .style(move |_, status| button::Style {
            background: (selected || status == button::Status::Hovered)
                .then(|| theme::SIDEBAR_HOVER.into()),
            text_color: theme::SIDEBAR_GROUP,
            border: Border {
                radius: theme::RADIUS_SMALL.into(),
                ..Border::default()
            },
            ..button::Style::default()
        }),
        text("Add a repository").size(theme::LABEL),
        tooltip::Position::Right,
    )
    .into()
}

/// A group with something at the right of its name; a group whose name opens a page passes it as `target`.
fn group_with<'a>(
    name: impl Into<Option<&'a str>>,
    target: Option<Page>,
    current: &Page,
    action: Option<Element<'a, Message>>,
    items: Vec<Element<'a, Message>>,
    collapsed: bool,
) -> Element<'a, Message> {
    let mut content = column![].spacing(theme::SPACE_1);
    if let (Some(name), false) = (name.into(), collapsed) {
        let label: Element<'a, Message> = match target {
            Some(target) => {
                let selected = *current == target;
                button(
                    text(name)
                        .size(theme::GROUP_LABEL)
                        .font(theme::manrope(Weight::Bold)),
                )
                .padding(0.0)
                .on_press(Message::Select(target))
                .style(move |_, status| button::Style {
                    background: None,
                    text_color: if selected || status == button::Status::Hovered {
                        theme::TEXT
                    } else {
                        theme::SIDEBAR_GROUP
                    },
                    border: Border::default(),
                    ..button::Style::default()
                })
                .into()
            }
            None => text(name)
                .size(theme::GROUP_LABEL)
                .font(theme::manrope(Weight::Bold))
                .color(theme::SIDEBAR_GROUP)
                .into(),
        };
        let mut heading = row![label, space::horizontal()].align_y(Center);
        if let Some(action) = action {
            heading = heading.push(action);
        }
        content = content.push(container(heading).padding(Padding {
            top: theme::SPACE_2,
            right: theme::SPACE_4,
            bottom: theme::SPACE_6,
            left: theme::SPACE_10,
        }));
    }
    container(content.extend(items))
        .padding(theme::SPACE_10)
        .into()
}

/// One item: it opens `target`, and is marked when that is the page shown.
fn item<'a>(
    lead: Lead,
    label: &str,
    trailing: Trailing,
    target: Page,
    current: &Page,
    collapsed: bool,
) -> Element<'a, Message> {
    let selected = *current == target;
    // Something new there turns the icon to the alert colour.
    let new = matches!(trailing, Trailing::Badge(count) if count > 0);
    let icon_color = if new {
        theme::AMBER
    } else if selected {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    let lead: Element<'a, Message> = match lead {
        Lead::Icon(icon) => icon.view(icon_color, theme::ICON).into(),
        Lead::Dot(color) => container(space().width(theme::DOT).height(theme::DOT))
            .style(move |_| container::Style {
                background: Some(color.into()),
                border: Border {
                    radius: (theme::DOT / 2.0).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into(),
    };
    let count = |count: usize| {
        text(count.to_string())
            .size(theme::LABEL)
            .font(theme::MONO)
            .color(theme::AMBER)
    };
    let weight = if selected {
        Weight::Semibold
    } else {
        Weight::Normal
    };
    let content: Element<'a, Message> = if collapsed {
        container(lead).center_x(Length::Fill).into()
    } else {
        let mut line = row![
            lead,
            text(label.to_string())
                .size(theme::BODY)
                .font(theme::manrope(weight)),
            space::horizontal(),
        ]
        .spacing(theme::SPACE_10)
        .align_y(Center);
        match trailing {
            Trailing::Nothing | Trailing::Badge(0) => {}
            Trailing::Text(shown) => {
                line = line.push(
                    text(shown)
                        .size(theme::LABEL)
                        .font(theme::MONO)
                        .color(theme::TEXT_MUTED),
                );
            }
            Trailing::Badge(shown) => line = line.push(count(shown)),
        }
        line.into()
    };
    let entry = button(content)
        .width(Length::Fill)
        .padding(if collapsed {
            [theme::SPACE_6, 0.0]
        } else {
            [theme::SPACE_6, theme::SPACE_10]
        })
        .on_press(Message::Select(target))
        .style(move |_, status| {
            let background = if selected {
                Some(theme::SIDEBAR_CURRENT)
            } else if status == button::Status::Hovered {
                Some(theme::SIDEBAR_HOVER)
            } else {
                None
            };
            button::Style {
                background: background.map(Into::into),
                text_color: if selected {
                    theme::TEXT
                } else {
                    theme::SIDEBAR_TEXT
                },
                border: Border {
                    radius: theme::RADIUS_ITEM.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        });
    if collapsed {
        tooltip(
            entry,
            text(label.to_string()).size(theme::LABEL),
            tooltip::Position::Right,
        )
        .into()
    } else {
        entry.into()
    }
}
