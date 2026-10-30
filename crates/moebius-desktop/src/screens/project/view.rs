//! How a project's page is drawn: the header, the tabs (`strip`), the tab shown and the Files sidebar on its right
//! with the tree of the default branch.

use iced::font::Weight;
use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Alignment, Border, Element, Length, Padding};

use crate::app::Message as AppMessage;
use crate::client::{Repository, TreeEntry};
use crate::theme;
use crate::widgets::icons::Icon;
use crate::widgets::page;

use super::{Listing, Message, Project, Tab};

/// A line of the tree that is not an entry: reading, failed or empty.
fn note_row<'a>(depth: usize, line: &'a str, color: iced::Color) -> Element<'a, AppMessage> {
    container(
        text(line)
            .size(theme::CONTEXT)
            .font(theme::MONO)
            .color(color),
    )
    .padding(Padding {
        top: theme::SPACE_4,
        bottom: theme::SPACE_4,
        left: theme::SPACE_10 + theme::TREE_INDENT * depth as f32 + theme::ICON + theme::SPACE_8,
        right: theme::SPACE_10,
    })
    .into()
}

/// A directory or a file of the tree, indented by its depth.
fn entry_row<'a>(
    repository: i64,
    entry: &'a TreeEntry,
    depth: usize,
    open: bool,
) -> Element<'a, AppMessage> {
    let dir = entry.kind == "dir";
    let chevron: Element<'a, AppMessage> = if dir {
        let icon = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        icon.view(theme::TEXT_MUTED, theme::ICON_SMALL).into()
    } else {
        space().width(theme::ICON_SMALL).into()
    };
    let icon = if dir { Icon::Folder } else { Icon::File };
    let mut line = row![
        chevron,
        icon.view(theme::TEXT_SECONDARY, theme::ICON),
        text(entry.name.as_str())
            .size(theme::CONTEXT)
            .font(if dir {
                theme::manrope(Weight::Semibold)
            } else {
                theme::MANROPE
            })
            .color(theme::TEXT),
        space::horizontal(),
    ]
    .spacing(theme::SPACE_8)
    .align_y(Alignment::Center);
    if !dir {
        line = line.push(
            text(size(entry.size))
                .size(theme::LABEL)
                .font(theme::MONO)
                .color(theme::TEXT_MUTED),
        );
    }
    let message = if dir {
        Message::Toggle(entry.path.clone())
    } else {
        Message::Open(entry.path.clone())
    };
    container(
        button(line)
            .padding(Padding {
                top: theme::SPACE_4,
                bottom: theme::SPACE_4,
                left: theme::SPACE_10 + theme::TREE_INDENT * depth as f32,
                right: theme::SPACE_10,
            })
            .width(Length::Fill)
            .on_press(AppMessage::Project(repository, message))
            .style(|_, status| button::Style {
                background: (status == button::Status::Hovered).then(|| theme::LANE.into()),
                border: Border {
                    radius: theme::RADIUS_SMALL.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
    )
    .id(format!("tree-{}", entry.path))
    .into()
}

/// A size in bytes as the tree writes it: `512 B`, `4.2 KB`, `1.3 MB`.
pub(crate) fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1_000_000 => format!("{:.1} MB", b as f64 / 1_000_000.0),
        b if b >= 1_000 => format!("{:.1} KB", b as f64 / 1_000.0),
        b => format!("{b} B"),
    }
}

impl Project {
    /// The page of `repository` for a tab other than Board, under the header and the tabs the app gives. `files` is
    /// the Files sidebar, `chat` the chat of a chat tab as the Chat page draws it.
    pub fn view<'a>(
        &'a self,
        repository: &'a Repository,
        tabs: Element<'a, AppMessage>,
        files: Option<Element<'a, AppMessage>>,
        chat: Option<Element<'a, AppMessage>>,
    ) -> Element<'a, AppMessage> {
        let id = repository.id;
        let (context, actions): (&str, Element<'a, AppMessage>) = match &self.tab {
            Tab::Editor(_) => {
                let can_save = self
                    .shown()
                    .is_some_and(|file| file.unsaved() && !file.saving);
                (
                    "Save commits to the default branch.",
                    page::primary_button(
                        "Save",
                        can_save.then_some(AppMessage::Project(id, Message::Save)),
                    ),
                )
            }
            Tab::Terminal(_) => (
                "Shell on the server, in the clone of the repository.",
                space().into(),
            ),
            Tab::Chat(_) => (
                "A chat with an agent working in this project.",
                space().into(),
            ),
            Tab::Board => ("Default branch. Click a file to edit it.", space().into()),
        };
        let header = page::header(
            "Projects",
            &repository.name,
            Some(repository.forge_tag()),
            page::context(context),
            actions,
        );
        let body: Element<'a, AppMessage> = match &self.tab {
            Tab::Editor(_) => self.editor_view(id),
            Tab::Terminal(n) => self.terminal_view(id, *n),
            Tab::Chat(_) => chat.unwrap_or_else(|| {
                container(page::body("Reading the chat…"))
                    .padding(theme::BOX_PADDING)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| page::box_style())
                    .into()
            }),
            Tab::Board => space().into(),
        };
        let mut main = row![body].spacing(theme::PAGE_GAP).height(Length::Fill);
        if let Some(files) = files {
            main = main.push(files);
        }
        column![header, tabs, main]
            .spacing(theme::PAGE_GAP)
            .width(Length::Fill)
            .into()
    }

    /// The tree of the default branch, as the rows of its root.
    fn tree_rows(&self, repository: i64) -> Vec<Element<'_, AppMessage>> {
        match self.dirs.get("") {
            None | Some(Listing::Reading) => {
                vec![note_row(
                    0,
                    "Reading the default branch.",
                    theme::TEXT_MUTED,
                )]
            }
            Some(Listing::Failed(error)) => vec![note_row(0, error.as_str(), theme::RED_TEXT)],
            Some(Listing::Read(entries)) if entries.is_empty() => {
                vec![note_row(0, "No files.", theme::TEXT_MUTED)]
            }
            Some(Listing::Read(entries)) => self.interleave(repository, entries, 0),
        }
    }

    /// The rows of `entries` at `depth`, each open directory followed by its own rows.
    fn interleave<'a>(
        &'a self,
        repository: i64,
        entries: &'a [TreeEntry],
        depth: usize,
    ) -> Vec<Element<'a, AppMessage>> {
        let mut rows = Vec::new();
        for entry in entries {
            let open = self.open_dirs.contains(&entry.path);
            rows.push(entry_row(repository, entry, depth, open));
            if entry.kind != "dir" || !open {
                continue;
            }
            match self.dirs.get(&entry.path) {
                None | Some(Listing::Reading) => {
                    rows.push(note_row(depth + 1, "Reading.", theme::TEXT_MUTED));
                }
                Some(Listing::Failed(error)) => {
                    rows.push(note_row(depth + 1, error.as_str(), theme::RED_TEXT));
                }
                Some(Listing::Read(children)) if children.is_empty() => {
                    rows.push(note_row(depth + 1, "Empty.", theme::TEXT_MUTED));
                }
                Some(Listing::Read(children)) => {
                    rows.extend(self.interleave(repository, children, depth + 1));
                }
            }
        }
        rows
    }
}

/// The Files sidebar on the right of a project's page: the default branch as a tree in a white box, none while it is
/// folded away. A project the person has not opened yet shows the tree as it is being read.
pub fn sidebar<'a>(
    repository: i64,
    project: Option<&'a Project>,
) -> Option<Element<'a, AppMessage>> {
    if project.is_some_and(|project| project.files_folded) {
        return None;
    }
    let rows = match project {
        Some(project) => project.tree_rows(repository),
        None => vec![note_row(
            0,
            "Reading the default branch.",
            theme::TEXT_MUTED,
        )],
    };
    let head = text("Files · default branch")
        .size(theme::LABEL)
        .font(theme::MONO)
        .color(theme::TEXT_MUTED);
    Some(
        container(
            column![
                container(head).padding([0.0, theme::SPACE_10]),
                scrollable(column(rows).spacing(theme::SPACE_1)).height(Length::Fill),
            ]
            .spacing(theme::SPACE_8),
        )
        .id("files-sidebar")
        .padding(theme::SPACE_10)
        .width(theme::FILES_WIDTH)
        .height(Length::Fill)
        .style(|_| page::box_style())
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_size_is_written_short() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(4_200), "4.2 KB");
        assert_eq!(size(1_300_000), "1.3 MB");
    }
}
