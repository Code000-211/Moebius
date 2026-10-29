//! Finding in a project: "Go to file" (Ctrl+P), a fuzzy match over the paths of the default branch whose best match
//! opens in the Editor, and "Search in files" (Ctrl+Shift+F), a full-text search of the default branch run by the
//! server (`GET /v1/repositories/{id}/search`) whose hits are listed per file and open the file at their line. The
//! panel shows over the Files and Editor tabs. `fuzzy` ranks the paths; this module holds the state, how a message
//! changes it, what the floating chat is told of it and how it is drawn.

use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Border, Element, Length, Padding, Task, keyboard};

use super::{Project, fuzzy};
use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, PathList, SearchResult};
use crate::screens::connect::input_style;
use crate::screens::live::say;
use crate::search::Direction;
use crate::settings::Settings;
use crate::theme;
use crate::widgets::icons::Icon;
use crate::widgets::page;

/// The id of the field of either panel, which opening a panel focuses.
pub const FIELD: &str = "find-field";

/// How many paths "Go to file" lists.
const PATHS_SHOWN: usize = 10;

/// How many hits a search asks for.
const HIT_LIMIT: usize = 200;

/// How many hits the floating chat is told of.
const HITS_TOLD: usize = 8;

#[derive(Debug, Clone)]
pub enum Message {
    /// Ctrl+P: "Go to file" opens.
    Go,
    /// Ctrl+Shift+F: "Search in files" opens.
    Search,
    /// Escape, or the cross of the panel.
    Close,
    /// The arrows moved the selection.
    Move(Direction),
    /// The field was edited.
    Query(String),
    /// The regular expression option was switched.
    Regex(bool),
    /// Enter: "Go to file" opens the selected path; a search runs, or when it already ran opens the selected hit.
    Submit,
    /// A path or a hit was clicked, by its number in the list.
    Pick(usize),
    /// The server listed the paths.
    Paths(Result<PathList, ClientError>),
    /// The server answered the search that was asked as number `n`.
    Found(u64, Result<SearchResult, ClientError>),
}

/// What a search has come to.
#[derive(Debug)]
pub(super) enum Hits {
    /// Nothing was asked yet.
    Idle,
    Searching,
    Found(SearchResult),
    Failed(String),
}

/// The panel that is open.
#[derive(Debug)]
pub(super) enum Find {
    Go {
        query: String,
        selected: usize,
    },
    Search {
        query: String,
        regex: bool,
        selected: usize,
        /// The text and option the hits are for, which tell whether Enter searches again or opens a hit.
        asked: Option<(String, bool)>,
        /// The number of the last search asked, so that an answer to an older one is dropped.
        number: u64,
        hits: Hits,
    },
}

/// Where the file in the Editor was opened from, which the floating chat is told.
#[derive(Debug)]
pub(super) struct Origin {
    pub path: String,
    pub text: String,
}

/// The keys of finding: Ctrl+P (Cmd+P on macOS), Ctrl+Shift+F, Escape, and the arrows when no field took them. The
/// app ignores them where there is no project.
pub fn key(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<AppMessage> {
    let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };
    let message = match key.as_ref() {
        keyboard::Key::Character(c) if modifiers.command() && c.eq_ignore_ascii_case("p") => {
            if modifiers.shift() {
                return None;
            }
            Message::Go
        }
        keyboard::Key::Character(c) if modifiers.command() && c.eq_ignore_ascii_case("f") => {
            if !modifiers.shift() {
                return None;
            }
            Message::Search
        }
        keyboard::Key::Named(keyboard::key::Named::Escape) => Message::Close,
        keyboard::Key::Named(keyboard::key::Named::ArrowUp)
            if status == iced::event::Status::Ignored =>
        {
            Message::Move(Direction::Up)
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowDown)
            if status == iced::event::Status::Ignored =>
        {
            Message::Move(Direction::Down)
        }
        _ => return None,
    };
    Some(AppMessage::Find(message))
}

/// `selected` moved one step in `direction` within `count` entries.
fn step(selected: usize, direction: Direction, count: usize) -> usize {
    match direction {
        Direction::Up => selected.saturating_sub(1),
        Direction::Down => (selected + 1).min(count.saturating_sub(1)),
    }
}

impl Project {
    /// The paths "Go to file" lists for `query`, best first.
    fn matching(&self, query: &str) -> Vec<&str> {
        fuzzy::rank(
            query,
            &self.known_paths,
            PATHS_SHOWN,
            &self.recent,
            self.current.as_deref(),
        )
    }

    /// The hits of the search shown, flattened in the order they are listed.
    fn hit_list(&self) -> Vec<(&str, u64)> {
        match &self.find {
            Some(Find::Search {
                hits: Hits::Found(found),
                ..
            }) => found
                .files
                .iter()
                .flat_map(|file| file.hits.iter().map(|hit| (file.path.as_str(), hit.line)))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub(super) fn find_update(
        &mut self,
        repository: i64,
        message: Message,
        settings: &Settings,
    ) -> Task<AppMessage> {
        match message {
            Message::Go | Message::Search => {
                let go = matches!(message, Message::Go);
                // Over the board there are no files to find; the Files tab shows.
                let listing = if self.tab == super::Tab::Board {
                    self.tab = super::Tab::Files;
                    self.list(repository, "", settings)
                } else {
                    Task::none()
                };
                // The text typed before stays, so that Ctrl+P again selects it.
                let query = match self.find.take() {
                    Some(Find::Go { query, .. } | Find::Search { query, .. }) => query,
                    None => String::new(),
                };
                let focus = iced::widget::operation::focus(FIELD);
                if go {
                    self.find = Some(Find::Go { query, selected: 0 });
                    let client = Client::new(settings.clone());
                    let read = Task::perform(
                        async move { client.paths(repository).await },
                        move |paths| {
                            AppMessage::Project(
                                repository,
                                super::Message::Find(Message::Paths(paths)),
                            )
                        },
                    );
                    return Task::batch([listing, focus, read]);
                }
                self.find = Some(Find::Search {
                    query,
                    regex: false,
                    selected: 0,
                    asked: None,
                    number: 0,
                    hits: Hits::Idle,
                });
                return Task::batch([listing, focus]);
            }
            Message::Close => self.find = None,
            Message::Paths(Ok(list)) => self.known_paths = list.paths,
            Message::Paths(Err(_)) => {}
            Message::Move(direction) => {
                let count = match &self.find {
                    Some(Find::Go { query, .. }) => self.matching(query).len(),
                    Some(Find::Search { .. }) => self.hit_list().len(),
                    None => return Task::none(),
                };
                if let Some(Find::Go { selected, .. } | Find::Search { selected, .. }) =
                    &mut self.find
                {
                    *selected = step(*selected, direction, count);
                }
            }
            Message::Query(text) => {
                if let Some(
                    Find::Go { query, selected }
                    | Find::Search {
                        query, selected, ..
                    },
                ) = &mut self.find
                {
                    *query = text;
                    *selected = 0;
                }
            }
            Message::Regex(on) => {
                if let Some(Find::Search { regex, .. }) = &mut self.find {
                    *regex = on;
                }
            }
            Message::Submit => return self.submit(repository, settings),
            Message::Pick(index) => return self.pick(repository, index, settings),
            Message::Found(number, result) => {
                if let Some(Find::Search {
                    number: asked,
                    hits,
                    selected,
                    ..
                }) = &mut self.find
                    && *asked == number
                {
                    *selected = 0;
                    *hits = match result {
                        Ok(found) => Hits::Found(found),
                        Err(e) => Hits::Failed(say(&e)),
                    };
                }
            }
        }
        Task::none()
    }

    /// Enter: opens the selected path, runs the search, or opens the selected hit of the search that ran.
    fn submit(&mut self, repository: i64, settings: &Settings) -> Task<AppMessage> {
        match &mut self.find {
            Some(Find::Go { query, selected }) => {
                let (query, selected) = (query.clone(), *selected);
                self.pick_path(repository, &query, selected, settings)
            }
            Some(Find::Search {
                query,
                regex,
                asked,
                number,
                hits,
                selected,
            }) => {
                if query.trim().is_empty() {
                    return Task::none();
                }
                let now = (query.clone(), *regex);
                if asked.as_ref() == Some(&now) && matches!(hits, Hits::Found(_)) {
                    let index = *selected;
                    return self.pick(repository, index, settings);
                }
                *number += 1;
                *asked = Some(now.clone());
                *hits = Hits::Searching;
                *selected = 0;
                let (client, n) = (Client::new(settings.clone()), *number);
                Task::perform(
                    async move { client.search(repository, &now.0, now.1, HIT_LIMIT).await },
                    move |found| {
                        AppMessage::Project(
                            repository,
                            super::Message::Find(Message::Found(n, found)),
                        )
                    },
                )
            }
            None => Task::none(),
        }
    }

    fn pick_path(
        &mut self,
        repository: i64,
        query: &str,
        index: usize,
        settings: &Settings,
    ) -> Task<AppMessage> {
        let Some(path) = self.matching(query).get(index).map(|path| path.to_string()) else {
            return Task::none();
        };
        let text = match query.trim() {
            "" => "Go to file".to_string(),
            typed => format!("Go to file, typed \"{typed}\""),
        };
        self.find = None;
        let task = self.open(repository, &path, None, settings);
        self.origin = Some(Origin { path, text });
        task
    }

    /// A path or a hit was chosen, by its number in the list shown.
    fn pick(&mut self, repository: i64, index: usize, settings: &Settings) -> Task<AppMessage> {
        match &self.find {
            Some(Find::Go { query, .. }) => {
                let query = query.clone();
                self.pick_path(repository, &query, index, settings)
            }
            Some(Find::Search { asked, regex, .. }) => {
                let Some((path, line)) = self
                    .hit_list()
                    .get(index)
                    .map(|(path, line)| (path.to_string(), *line))
                else {
                    return Task::none();
                };
                let typed = asked.as_ref().map_or("", |(query, _)| query.as_str());
                let text = format!(
                    "Search in files for \"{typed}\"{}, hit at line {line}",
                    if *regex { " (regular expression)" } else { "" }
                );
                let task = self.open(repository, &path, Some(line as usize), settings);
                self.origin = Some(Origin { path, text });
                // The panel stays, so that the next hit is a click away.
                task
            }
            None => Task::none(),
        }
    }

    /// What the floating chat is told of finding: the panel that is open and, in the Editor, where the file came from.
    pub(super) fn find_context(&self) -> Vec<String> {
        let mut lines = Vec::new();
        match &self.find {
            Some(Find::Go { query, .. }) => {
                lines.push(format!("\"Go to file\" is open; typed: \"{query}\""));
                let matching = self.matching(query);
                if !matching.is_empty() {
                    lines.push(format!("Best matches: {}", matching.join(", ")));
                }
            }
            Some(Find::Search {
                query,
                regex,
                asked,
                hits,
                ..
            }) => {
                lines.push(format!(
                    "\"Search in files\" is open; typed: \"{query}\"{}",
                    if *regex { " (regular expression)" } else { "" }
                ));
                match (asked, hits) {
                    (Some((asked, _)), Hits::Found(found)) => {
                        lines.push(format!(
                            "The search for \"{asked}\" found {} hits in {} files{}:",
                            found.total,
                            found.files.len(),
                            if found.truncated {
                                " (more were cut)"
                            } else {
                                ""
                            }
                        ));
                        lines.extend(
                            found
                                .files
                                .iter()
                                .flat_map(|file| file.hits.iter().map(move |hit| (file, hit)))
                                .take(HITS_TOLD)
                                .map(|(file, hit)| {
                                    format!("  {}:{}: {}", file.path, hit.line, hit.text.trim())
                                }),
                        );
                    }
                    (Some((asked, _)), Hits::Failed(error)) => {
                        lines.push(format!("The search for \"{asked}\" failed: {error}"));
                    }
                    _ => {}
                }
            }
            None => {}
        }
        if self.tab == super::Tab::Editor
            && let Some(origin) = &self.origin
            && self.current.as_deref() == Some(origin.path.as_str())
        {
            lines.push(format!("The open file came from: {}", origin.text));
        }
        lines
    }

    /// The panel over the Files or Editor tab, when one is open.
    pub(super) fn find_view(&self, repository: i64) -> Option<Element<'_, AppMessage>> {
        let message = move |message| AppMessage::Project(repository, super::Message::Find(message));
        let (title, hint) = match self.find.as_ref()? {
            Find::Go { .. } => ("Go to file", "Type part of a path"),
            Find::Search { .. } => ("Search in files", "Text to find, then Enter"),
        };
        let (query, selected) = match self.find.as_ref()? {
            Find::Go { query, selected }
            | Find::Search {
                query, selected, ..
            } => (query.as_str(), *selected),
        };
        let field = text_input(hint, query)
            .id(FIELD)
            .on_input(move |text| message(Message::Query(text)))
            .on_submit(message(Message::Submit))
            .size(theme::BODY)
            .padding(theme::CONTROL_PADDING)
            .style(input_style);
        let close = button(Icon::Close.view(theme::TEXT_MUTED, theme::ICON_SMALL))
            .padding(theme::SPACE_2)
            .on_press(message(Message::Close))
            .style(|_, _| button::Style::default());
        let heading = row![
            text(title)
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
            space::horizontal(),
            close,
        ]
        .align_y(iced::Alignment::Center);
        let mut input = row![field]
            .spacing(theme::SPACE_8)
            .align_y(iced::Alignment::Center);
        let body: Element<'_, AppMessage> = match self.find.as_ref()? {
            Find::Go { .. } => self.go_rows(&message, selected, query),
            Find::Search { regex, hits, .. } => {
                input = input
                    .push(page::chip(
                        "Regex".into(),
                        *regex,
                        false,
                        message(Message::Regex(!regex)),
                    ))
                    .push(page::white_button("Search", Some(message(Message::Submit))));
                self.hit_rows(&message, selected, hits)
            }
        };
        Some(
            container(column![heading, input, body].spacing(theme::SPACE_8))
                .padding(theme::SPACE_10)
                .width(Length::Fill)
                .style(|_| page::box_style())
                .into(),
        )
    }

    fn go_rows<'a>(
        &'a self,
        message: &impl Fn(Message) -> AppMessage,
        selected: usize,
        query: &str,
    ) -> Element<'a, AppMessage> {
        if self.known_paths.is_empty() {
            return note("Reading the files.", theme::TEXT_MUTED);
        }
        let matching = self.matching(query);
        if matching.is_empty() {
            return note("No file matches.", theme::TEXT_MUTED);
        }
        column(matching.into_iter().enumerate().map(|(index, path)| {
            let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
            let line = row![
                Icon::File.view(theme::TEXT_SECONDARY, theme::ICON),
                text(name.to_string())
                    .size(theme::CONTEXT)
                    .font(theme::manrope(iced::font::Weight::Semibold))
                    .color(theme::TEXT),
                text(dir.to_string())
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
            ]
            .spacing(theme::SPACE_8)
            .align_y(iced::Alignment::Center);
            row_button(
                line.into(),
                index == selected,
                message(Message::Pick(index)),
            )
            .id(format!("go-{path}"))
            .into()
        }))
        .spacing(theme::SPACE_1)
        .into()
    }

    fn hit_rows<'a>(
        &'a self,
        message: &impl Fn(Message) -> AppMessage,
        selected: usize,
        hits: &'a Hits,
    ) -> Element<'a, AppMessage> {
        let found = match hits {
            Hits::Idle => {
                return note(
                    "Case is ignored. Regex takes an extended expression.",
                    theme::TEXT_MUTED,
                );
            }
            Hits::Searching => return note("Searching.", theme::TEXT_MUTED),
            Hits::Failed(error) => return note(error, theme::RED_TEXT),
            Hits::Found(found) if found.files.is_empty() => {
                return note("No hits.", theme::TEXT_MUTED);
            }
            Hits::Found(found) => found,
        };
        let mut rows = column![].spacing(theme::SPACE_1);
        let mut index = 0;
        for file in &found.files {
            rows = rows.push(
                container(
                    row![
                        Icon::File.view(theme::TEXT_SECONDARY, theme::ICON),
                        text(file.path.as_str())
                            .size(theme::CONTEXT)
                            .font(theme::manrope(iced::font::Weight::Semibold))
                            .color(theme::TEXT),
                        text(file.hits.len().to_string())
                            .size(theme::LABEL)
                            .font(theme::MONO)
                            .color(theme::TEXT_MUTED),
                    ]
                    .spacing(theme::SPACE_8)
                    .align_y(iced::Alignment::Center),
                )
                .padding(Padding {
                    top: theme::SPACE_6,
                    bottom: theme::SPACE_2,
                    left: theme::SPACE_10,
                    right: theme::SPACE_10,
                }),
            );
            for hit in &file.hits {
                let line = row![
                    text(hit.line.to_string())
                        .size(theme::CODE)
                        .font(theme::MONO)
                        .color(theme::TEXT_MUTED)
                        .width(Length::Fixed(theme::MARKER_COLUMN * 2.0))
                        .align_x(iced::alignment::Horizontal::Right),
                    text(hit.text.trim_end())
                        .size(theme::CODE)
                        .font(theme::MONO)
                        .color(theme::TEXT)
                        .wrapping(iced::widget::text::Wrapping::None),
                ]
                .spacing(theme::SPACE_10);
                rows = rows.push(
                    row_button(
                        line.into(),
                        index == selected,
                        message(Message::Pick(index)),
                    )
                    .id(format!("hit-{}:{}", file.path, hit.line)),
                );
                index += 1;
            }
        }
        let summary = format!(
            "{} hits in {} files{}",
            found.total,
            found.files.len(),
            if found.truncated {
                ", more were cut"
            } else {
                ""
            }
        );
        column![
            scrollable(rows).height(Length::Fixed(theme::OUTPUT_MAX_HEIGHT)),
            text(summary)
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
        ]
        .spacing(theme::SPACE_6)
        .into()
    }
}

/// One line of a list, selected or not, that sends `message` when it is clicked.
fn row_button<'a>(
    line: Element<'a, AppMessage>,
    selected: bool,
    message: AppMessage,
) -> iced::widget::Container<'a, AppMessage> {
    container(
        button(line)
            .padding(Padding {
                top: theme::SPACE_4,
                bottom: theme::SPACE_4,
                left: theme::SPACE_10,
                right: theme::SPACE_10,
            })
            .width(Length::Fill)
            .on_press(message)
            .style(move |_, status| button::Style {
                background: (selected || status == button::Status::Hovered)
                    .then(|| theme::LANE.into()),
                border: Border {
                    radius: theme::RADIUS_SMALL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    )
}

fn note<'a>(line: &'a str, color: iced::Color) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::CONTEXT)
        .font(theme::MONO)
        .color(color)
        .into()
}
