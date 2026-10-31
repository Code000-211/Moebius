//! How an editor tab is drawn: the file open as highlighted text to edit, or what stands in for a file that cannot
//! be edited, with the path over it and the status line under it.

use iced::advanced::text::highlighter::Format;
use iced::highlighter;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space, text, text_editor};
use iced::{Alignment, Element, Length};

use crate::app::Message as AppMessage;
use crate::widgets::inline_edit::editor_style;
use crate::widgets::page;
use crate::{theme, widgets::transcript};

use super::{Body, Message, Project, TEXT_LIMIT};

/// The language the highlighter is given for a file: its extension, or its name when it has none.
fn token(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name
        .rsplit_once('.')
        .map_or(name, |(_, extension)| extension);
    transcript::language(extension).to_string()
}

/// What the language label of an editor says of a file.
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
        .padding(theme::FIELD_PADDING[1])
        .height(Length::Fill)
        .highlight_with::<highlighter::Highlighter>(
            highlighter::Settings {
                theme: crate::theme::HIGHLIGHT,
                token: token(path),
            },
            |highlight, _| Format {
                color: highlight.color(),
                font: None,
            },
        )
        .style(editor_style)
        .into()
}

impl Project {
    pub(super) fn editor_view(&self, repository: i64) -> Element<'_, AppMessage> {
        let Some(file) = self.shown() else {
            return container(page::body("No file open."))
                .padding(theme::BOX_PADDING)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| page::box_style())
                .into();
        };
        let label = text(transcript_label(&file.path))
            .size(theme::LABEL)
            .font(theme::MONO)
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
            Body::Reading => page::body("Reading the file…"),
            Body::Binary => page::body(format!(
                "{} is binary ({} bytes), not editable here.",
                file.name(),
                file.size
            )),
            Body::TooLarge => page::body(format!(
                "{} is over {TEXT_LIMIT} ({} bytes), not editable here.",
                file.name(),
                file.size
            )),
            Body::Failed(error) => text(error.as_str())
                .size(theme::CONTEXT)
                .font(theme::MONO)
                .color(theme::RED_TEXT)
                .into(),
        };
        let mut status = row![].spacing(theme::SPACE_14).align_y(Alignment::Center);
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
                        .font(theme::MONO)
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
            Some(("Committed to the default branch.", theme::ONLINE_TEXT))
        } else {
            None
        };
        if let Some((line, color)) = notice {
            status = status.push(
                text(line)
                    .size(theme::CONTEXT)
                    .font(theme::MONO)
                    .color(color)
                    .wrapping(Wrapping::WordOrGlyph),
            );
        }
        container(column![path_line, main, status].spacing(theme::SPACE_12))
            .padding(theme::BOX_PADDING)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| page::box_style())
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
