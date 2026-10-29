//! How a project's page is drawn: the tabs over it, and the Files tab's tree of the default branch.

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
    container(text(line).size(theme::CONTEXT).color(color))
        .padding(Padding {
            top: 6.0,
            bottom: 6.0,
            left: 10.0 + 18.0 * depth as f32 + 22.0,
            right: 10.0,
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
        icon.view(theme::TEXT_MUTED, 14.0).into()
    } else {
        space().width(14).into()
    };
    let icon = if dir { Icon::Folder } else { Icon::File };
    let mut line = row![
        chevron,
        icon.view(theme::TEXT_SECONDARY, 16.0),
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
    .spacing(8)
    .align_y(Alignment::Center);
    if !dir {
        line = line.push(
            text(size(entry.size))
                .size(theme::LABEL)
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
                top: 6.0,
                bottom: 6.0,
                left: 10.0 + 18.0 * depth as f32,
                right: 10.0,
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

/// The tabs Board, Files and Editor at the top of a project's page; the Editor's says how many files are open.
pub fn tabs<'a>(repository: i64, project: Option<&Project>) -> Element<'a, AppMessage> {
    let shown = project.map_or(Tab::Board, Project::tab);
    let open = project.map_or(0, |project| project.files.len());
    let tab = |tab: Tab| -> Element<'a, AppMessage> {
        let selected = tab == shown;
        let label = match (tab, open) {
            (Tab::Editor, n) if n > 0 => format!("Editor · {n}"),
            (tab, _) => tab.title().to_string(),
        };
        let underline = container(space())
            .width(Length::Fill)
            .height(2)
            .style(move |_| container::Style {
                background: selected.then(|| theme::TEAL.into()),
                ..container::Style::default()
            });
        container(
            button(
                column![
                    text(label)
                        .size(theme::CONTEXT)
                        .font(theme::manrope(if selected {
                            Weight::Bold
                        } else {
                            Weight::Semibold
                        }))
                        .color(if selected {
                            theme::TEXT
                        } else {
                            theme::TEXT_SECONDARY
                        }),
                    underline,
                ]
                .spacing(6)
                .width(Length::Shrink),
            )
            .padding(Padding {
                top: 2.0,
                bottom: 0.0,
                left: 2.0,
                right: 2.0,
            })
            .on_press(AppMessage::Project(repository, Message::Tab(tab)))
            .style(|_, _| button::Style::default()),
        )
        .id(format!("project-tab-{}", tab.title()))
        .into()
    };
    column![
        row![tab(Tab::Board), tab(Tab::Files), tab(Tab::Editor)].spacing(22),
        page::rule(),
    ]
    .into()
}

impl Project {
    /// The Files or Editor tab of `repository`'s page, under the header and the tabs the app gives.
    pub fn view<'a>(
        &'a self,
        repository: &'a Repository,
        tabs: Element<'a, AppMessage>,
    ) -> Element<'a, AppMessage> {
        let id = repository.id;
        let (context, actions): (&str, Element<'a, AppMessage>) = match self.tab {
            Tab::Editor => {
                let can_save = self
                    .shown()
                    .is_some_and(|file| file.unsaved() && !file.saving);
                (
                    "A save is a commit on the default branch.",
                    page::primary_button(
                        "Save",
                        can_save.then_some(AppMessage::Project(id, Message::Save)),
                    ),
                )
            }
            _ => (
                "The default branch. A click on a file opens it in the Editor.",
                space().into(),
            ),
        };
        let header = page::header(
            "Projects",
            &repository.name,
            Some(match &repository.github {
                Some(github) => format!("GitHub · {github}"),
                None => "No forge".to_string(),
            }),
            page::context(context),
            actions,
        );
        let body = match self.tab {
            Tab::Editor => self.editor_view(id),
            _ => self.files_view(id),
        };
        column![header, tabs, body]
            .spacing(theme::PAGE_GAP)
            .width(Length::Fill)
            .into()
    }

    fn files_view(&self, repository: i64) -> Element<'_, AppMessage> {
        let rows: Vec<Element<'_, AppMessage>> = match self.dirs.get("") {
            None | Some(Listing::Reading) => {
                vec![note_row(
                    0,
                    "Reading the default branch.",
                    theme::TEXT_MUTED,
                )]
            }
            Some(Listing::Failed(error)) => vec![note_row(0, error.as_str(), theme::RED_TEXT)],
            Some(Listing::Read(entries)) if entries.is_empty() => {
                vec![note_row(
                    0,
                    "The repository has no files.",
                    theme::TEXT_MUTED,
                )]
            }
            Some(Listing::Read(entries)) => self.interleave(repository, entries, 0),
        };
        container(scrollable(column(rows).spacing(1)).height(Length::Fill))
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| page::box_style())
            .into()
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
