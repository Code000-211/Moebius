//! How the Editor tab is drawn: the tabs of the open files, and the file open as highlighted text to edit, or what
//! stands in for a file that cannot be edited.

use iced::advanced::text::highlighter::Format;
use iced::font::Weight;
use iced::highlighter;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, space, text, text_editor};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::widgets::icons::Icon;
use crate::widgets::page;
use crate::{theme, widgets::transcript};

use super::{Body, Message, OpenFile, Project, TEXT_LIMIT};

/// The language the highlighter is given for a file: its extension, or its name when it has none.
fn token(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name
        .rsplit_once('.')
        .map_or(name, |(_, extension)| extension);
    transcript::language(extension).to_string()
}

/// What the language label of the Editor says of a file.
fn transcript_label(path: &str) -> String {
    match token(path).as_str() {
        "rs" => "Rust".to_string(),
        "lua" => "Lua".to_string(),
        "py" => "Python".to_string(),
        "js" => "JavaScript".to_string(),
        "md" => "Markdown".to_string(),
        "sh" => "Shell".to_string(),
        "ini" => "TOML".to_string(),
        other => other.to_uppercase(),
    }
}

/// The tab of an open file: its name, a dot while it is unsaved, and the cross that closes it.
fn file_tab<'a>(repository: i64, file: &'a OpenFile, current: bool) -> Element<'a, AppMessage> {
    let mut label = row![
        text(file.name())
            .size(theme::BREADCRUMB)
            .font(theme::MONO)
            .color(if current {
                theme::TEXT
            } else {
                theme::TEXT_SECONDARY
            })
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if file.unsaved() {
        label = label.push(
            container(space().width(theme::DOT).height(theme::DOT)).style(|_| container::Style {
                background: Some(theme::AMBER.into()),
                border: Border {
                    radius: (theme::DOT / 2.0).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
        );
    }
    let close = container(
        button(Icon::Close.view(theme::TEXT_MUTED, 12.0))
            .padding(2)
            .on_press(AppMessage::Project(
                repository,
                Message::Close(file.path.clone()),
            ))
            .style(|_, status| button::Style {
                background: (status == button::Status::Hovered).then(|| theme::TRACK.into()),
                border: Border {
                    radius: theme::RADIUS_SMALL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    )
    .id(format!("close-{}", file.path));
    let open = button(label)
        .padding(0)
        .on_press(AppMessage::Project(
            repository,
            Message::Show(file.path.clone()),
        ))
        .style(|_, _| button::Style::default());
    container(row![open, close].spacing(8).align_y(Alignment::Center))
        .padding([6, 10])
        .style(move |_| container::Style {
            background: Some(if current { theme::SURFACE } else { theme::LANE }.into()),
            border: Border {
                color: if current {
                    theme::TEAL_BORDER
                } else {
                    theme::LINE
                },
                width: 1.0,
                radius: theme::RADIUS_CONTROL.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// The file in the code font, highlighted by its extension.
fn code_editor<'a>(
    repository: i64,
    path: &str,
    content: &'a text_editor::Content,
) -> Element<'a, AppMessage> {
    text_editor(content)
        .on_action(move |action| AppMessage::Project(repository, Message::Edit(action)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding(14)
        .height(Length::Fill)
        .highlight_with::<highlighter::Highlighter>(
            highlighter::Settings {
                theme: highlighter::Theme::InspiredGitHub,
                token: token(path),
            },
            |highlight, _| Format {
                color: highlight.color(),
                font: None,
            },
        )
        .style(|_, status| text_editor::Style {
            background: theme::LANE.into(),
            border: Border {
                color: match status {
                    text_editor::Status::Focused { .. } => theme::TEAL,
                    _ => theme::LINE,
                },
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            placeholder: theme::TEXT_MUTED,
            value: theme::TEXT,
            selection: theme::TEAL_FILL,
        })
        .into()
}

impl Project {
    pub(super) fn editor_view(&self, repository: i64) -> Element<'_, AppMessage> {
        let Some(file) = self.shown() else {
            return container(page::body("No file is open. Open one from Files."))
                .padding(20)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| page::box_style())
                .into();
        };
        let tabs = row(self.files.iter().map(|open| {
            file_tab(
                repository,
                open,
                Some(open.path.as_str()) == self.current.as_deref(),
            )
        }))
        .spacing(6)
        .wrap();
        let label = text(transcript_label(&file.path))
            .size(theme::LABEL)
            .color(theme::TEXT_MUTED);
        let path_line = row![
            text(file.path.as_str())
                .size(theme::CODE)
                .font(theme::MONO)
                .color(theme::TEXT_SECONDARY),
            space::horizontal(),
            label,
        ]
        .align_y(Alignment::Center);
        let main: Element<'_, AppMessage> = match &file.body {
            Body::Text { content, .. } => code_editor(repository, &file.path, content),
            Body::Reading => page::body("Reading the file."),
            Body::Binary => page::body(format!(
                "{} is a binary file ({} bytes); it is not edited here.",
                file.name(),
                file.size
            )),
            Body::TooLarge => page::body(format!(
                "{} is larger than {TEXT_LIMIT} ({} bytes); it is not edited here.",
                file.name(),
                file.size
            )),
            Body::Failed(error) => text(error.as_str())
                .size(theme::CONTEXT)
                .color(theme::RED_TEXT)
                .into(),
        };
        let mut status = row![].spacing(14).align_y(Alignment::Center);
        if let Some((line, column)) = self.cursor() {
            status = status.push(
                text(format!("Ln {line}, Col {column}"))
                    .size(theme::LABEL)
                    .font(theme::MONO)
                    .color(theme::TEXT_MUTED),
            );
        }
        let notice = if self.closing.as_deref() == Some(file.path.as_str()) {
            status = status
                .push(
                    text(format!("Close {} without saving?", file.name()))
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY),
                )
                .push(page::white_button(
                    "Keep",
                    Some(AppMessage::Project(repository, Message::Keep)),
                ))
                .push(page::white_button(
                    "Close without saving",
                    Some(AppMessage::Project(
                        repository,
                        Message::Discard(file.path.clone()),
                    )),
                ));
            None
        } else if let Some(error) = &file.error {
            Some((error.as_str(), theme::RED_TEXT))
        } else if file.saving {
            Some(("Saving.", theme::TEXT_MUTED))
        } else if file.unsaved() {
            Some(("Unsaved changes.", theme::AMBER_TEXT))
        } else if file.committed {
            Some((
                "Saved as a commit on the default branch.",
                theme::ONLINE_TEXT,
            ))
        } else {
            None
        };
        if let Some((line, color)) = notice {
            status = status.push(
                text(line)
                    .size(theme::CONTEXT)
                    .font(theme::manrope(Weight::Semibold))
                    .color(color)
                    .wrapping(Wrapping::WordOrGlyph),
            );
        }
        column![
            tabs,
            container(column![path_line, main, status].spacing(12))
                .padding(20)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| page::box_style()),
        ]
        .spacing(10)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_file_is_highlighted_by_its_extension_or_its_name() {
        assert_eq!(token("src/main.rs"), "rs");
        assert_eq!(token(".moebius/ready.luau"), "lua");
        assert_eq!(token("Cargo.toml"), "ini");
        assert_eq!(token("Makefile"), "Makefile");
        assert_eq!(transcript_label("a/b.rs"), "Rust");
    }
}
