//! The sidebar on every screen: the app name with the connection, the search field, the Inbox, Chat and Fleet, and the groups Boards,
//! Build, Improve and Setup. It collapses to a rail of icons. Every number in it comes from the server.

use iced::font::Weight;
use iced::widget::{button, column, container, row, scrollable, space, text, text_input, tooltip};
use iced::{Border, Center, Color, Element, Length, Padding};

use crate::app::{Connection, Message, Page};
use crate::client::Snapshot;
use crate::icons::{self, Icon};
use crate::search;
use crate::theme;
use crate::usage::short;

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
const SETUP: [(Page, Icon); 4] = [
    (Page::Models, Icon::Models),
    (Page::Plugins, Icon::Plugins),
    (Page::Terminal, Icon::Terminal),
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

/// What the sidebar tells besides the boards.
pub struct Counts {
    pub chats_unread: usize,
    /// How many runs and chats are working.
    pub working: usize,
    /// The tokens used today and the day's budget, once the usage is read.
    pub tokens_today: Option<(i64, Option<i64>)>,
    /// Whether a step of Get started is missing, which makes the page listed.
    pub setup_incomplete: bool,
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
        group(None, vec![inbox, chat, fleet], collapsed),
        divider(),
        group_with(
            Some("Boards"),
            Some(add_board(page)),
            boards.collect(),
            collapsed
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
            top(connection, collapsed, search),
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
        return space().height(8).into();
    };
    let shown = match budget {
        Some(budget) => format!("{} / {}", short(tokens), short(budget)),
        None => short(tokens),
    };
    let mut meter = column![].spacing(6);
    if !collapsed {
        meter = meter.push(
            row![
                text("Tokens today").size(13).color(theme::SIDEBAR_TEXT),
                space::horizontal(),
                text(shown)
                    .size(theme::GROUP_LABEL)
                    .font(theme::manrope(Weight::Semibold))
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
                .height(5)
                .style(move |_| container::Style {
                    background: Some(color.into()),
                    border: Border {
                        radius: 2.5.into(),
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
            top: 14.0,
            right: if collapsed { 12.0 } else { 20.0 },
            bottom: 18.0,
            left: if collapsed { 12.0 } else { 20.0 },
        })
    ]
    .into()
}

fn rule_vertical<'a>() -> Element<'a, Message> {
    container(space().width(0))
        .width(1)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::SIDEBAR_LINE.into()),
            ..container::Style::default()
        })
        .into()
}

fn divider<'a>() -> Element<'a, Message> {
    container(column![].height(1))
        .width(Length::Fill)
        .height(1)
        .style(|_| container::Style {
            background: Some(theme::SIDEBAR_LINE.into()),
            ..container::Style::default()
        })
        .into()
}

/// The app name with the connection, the toggle and the search field.
fn top<'a>(connection: Connection, collapsed: bool, search: &str) -> Element<'a, Message> {
    let (state, color, text_color) = match connection {
        Connection::Online => ("Online", theme::TEAL, theme::ONLINE_TEXT),
        Connection::Connecting => ("Connecting", theme::TEXT_MUTED, theme::TEXT_MUTED),
        Connection::Offline => ("Offline", theme::AMBER, theme::AMBER),
    };
    let label = if collapsed {
        "Expand the sidebar"
    } else {
        "Collapse the sidebar"
    };
    let toggle = tooltip(
        button(container(Icon::Panel.view(theme::TEXT_SECONDARY, 14.0)).center(Length::Fill))
            .width(26)
            .height(26)
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
            .spacing(10)
            .align_x(Center)
            .into()
    } else {
        row![
            icons::logo(),
            text("Moebius")
                .size(theme::APP_NAME)
                .font(theme::manrope(Weight::Bold)),
            space::horizontal(),
            container(space().width(theme::DOT).height(theme::DOT)).style(move |_| {
                container::Style {
                    background: Some(color.into()),
                    border: Border {
                        radius: (theme::DOT / 2.0).into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                }
            }),
            text(state).size(theme::LABEL).color(text_color),
            toggle,
        ]
        .spacing(9)
        .align_y(Center)
        .padding(Padding::from([0.0, 4.0]))
        .into()
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
            selection: theme::TEAL_BORDER,
        });
    let search: Element<'a, Message> = if collapsed {
        button(container(Icon::Search.view(theme::TEXT_MUTED, theme::ICON)).center_x(Length::Fill))
            .width(Length::Fill)
            .padding([8, 0])
            .on_press(Message::Search(search::Message::Focus))
            .style(|_, _| button::Style::default())
            .into()
    } else {
        container(
            row![Icon::Search.view(theme::TEXT_MUTED, theme::ICON), field]
                .spacing(8)
                .align_y(Center),
        )
        .width(Length::Fill)
        .padding([8, 10])
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
    container(column![mark_row, search].spacing(12))
        .padding(Padding {
            top: 16.0,
            right: 12.0,
            bottom: 8.0,
            left: 12.0,
        })
        .into()
}

/// A group of items under a bold name, which a collapsed sidebar does not show.
fn group<'a>(
    name: impl Into<Option<&'a str>>,
    items: Vec<Element<'a, Message>>,
    collapsed: bool,
) -> Element<'a, Message> {
    group_with(name, None, items, collapsed)
}

/// The "+" at the right of the name of Boards, which opens the page that adds a repository.
fn add_board<'a>(current: &Page) -> Element<'a, Message> {
    let selected = *current == Page::AddRepository;
    tooltip(
        button(
            text("+")
                .size(theme::BODY)
                .font(theme::manrope(Weight::Bold)),
        )
        .padding([0, 7])
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

/// A group with something at the right of its name.
fn group_with<'a>(
    name: impl Into<Option<&'a str>>,
    action: Option<Element<'a, Message>>,
    items: Vec<Element<'a, Message>>,
    collapsed: bool,
) -> Element<'a, Message> {
    let mut content = column![].spacing(1);
    if let (Some(name), false) = (name.into(), collapsed) {
        let mut heading = row![
            text(name)
                .size(theme::GROUP_LABEL)
                .font(theme::manrope(Weight::Bold))
                .color(theme::SIDEBAR_GROUP),
            space::horizontal(),
        ]
        .align_y(Center);
        if let Some(action) = action {
            heading = heading.push(action);
        }
        content = content.push(container(heading).padding(Padding {
            top: 2.0,
            right: 4.0,
            bottom: 6.0,
            left: 10.0,
        }));
    }
    container(content.extend(items)).padding(10).into()
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
            .font(theme::manrope(Weight::Semibold))
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
        .spacing(10)
        .align_y(Center);
        match trailing {
            Trailing::Nothing | Trailing::Badge(0) => {}
            Trailing::Text(shown) => {
                line = line.push(text(shown).size(theme::LABEL).color(theme::TEXT_MUTED));
            }
            Trailing::Badge(shown) => line = line.push(count(shown)),
        }
        line.into()
    };
    let entry = button(content)
        .width(Length::Fill)
        .padding(if collapsed { [7, 0] } else { [7, 10] })
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
