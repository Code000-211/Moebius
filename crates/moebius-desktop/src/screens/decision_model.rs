//! The Decision model screen: what the server has recorded of each kind of decision, the training-set exports a person
//! started, and the shadowing of a trained model: how often it chooses what was recorded, per kind, against the share a
//! takeover needs, and the kinds it has taken over, which a person can release. "Export training set" is `POST
//! /v1/exports`; the files stay on the server, so each kind shows the route that downloads its file and the command
//! that writes them elsewhere. A takeover proposal itself is an item of the inbox.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, space, stack, text};
use iced::{Alignment, Element, Length, Padding, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Export, KindCounts, ShadowedKind, Shadowing, Takeover};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::page;

#[derive(Debug, Clone)]
pub enum Message {
    Kinds(Result<Vec<KindCounts>, ClientError>),
    Exports(Result<Vec<Export>, ClientError>),
    /// "Export training set" was pressed.
    Export,
    Exported(Result<Export, ClientError>),
    Shadowing(Result<Shadowing, ClientError>),
    /// "Release" was pressed on a merged takeover.
    Release(i64),
    Released(Result<(), ClientError>),
}

#[derive(Default)]
pub struct DecisionModel {
    kinds: Vec<KindCounts>,
    exports: Vec<Export>,
    /// Whether an export is being written.
    exporting: bool,
    /// Why the last export was refused.
    failed: Option<String>,
    shadowing: Option<Shadowing>,
    /// Why the last release was refused.
    release_failed: Option<String>,
}

impl DecisionModel {
    /// Reads the counts and the exports again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let (kinds, exports) = (Client::new(settings.clone()), Client::new(settings.clone()));
        let shadowing = Client::new(settings.clone());
        Task::batch([
            Task::perform(async move { shadowing.shadowing().await }, |result| {
                AppMessage::DecisionModel(Message::Shadowing(result))
            }),
            Task::perform(async move { kinds.judgement_kinds().await }, |result| {
                AppMessage::DecisionModel(Message::Kinds(result))
            }),
            Task::perform(async move { exports.exports().await }, |result| {
                AppMessage::DecisionModel(Message::Exports(result))
            }),
        ])
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Kinds(Err(ClientError::Unauthorized))
                | Message::Exports(Err(ClientError::Unauthorized))
                | Message::Exported(Err(ClientError::Unauthorized))
                | Message::Shadowing(Err(ClientError::Unauthorized))
                | Message::Released(Err(ClientError::Unauthorized))
        )
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Kinds(Ok(kinds)) => self.kinds = kinds,
            Message::Exports(Ok(exports)) => self.exports = exports,
            Message::Shadowing(Ok(shadowing)) => self.shadowing = Some(shadowing),
            Message::Kinds(Err(_)) | Message::Exports(Err(_)) | Message::Shadowing(Err(_)) => {}
            Message::Release(takeover) => {
                let Some(settings) = settings else {
                    return Task::none();
                };
                self.release_failed = None;
                let client = Client::new(settings.clone());
                return Task::perform(
                    async move { client.release_takeover(takeover).await },
                    |result| AppMessage::DecisionModel(Message::Released(result)),
                );
            }
            Message::Released(Ok(())) => {
                return settings.map_or_else(Task::none, |settings| self.reload(settings));
            }
            Message::Released(Err(e)) => self.release_failed = Some(e.to_string()),
            Message::Export => {
                let Some(settings) = settings.filter(|_| !self.exporting) else {
                    return Task::none();
                };
                self.exporting = true;
                self.failed = None;
                let client = Client::new(settings.clone());
                return Task::perform(async move { client.start_export().await }, |result| {
                    AppMessage::DecisionModel(Message::Exported(result))
                });
            }
            Message::Exported(Ok(_)) => {
                self.exporting = false;
                return settings.map_or_else(Task::none, |settings| self.reload(settings));
            }
            Message::Exported(Err(e)) => {
                self.exporting = false;
                self.failed = Some(e.to_string());
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        let judgements: i64 = self.kinds.iter().map(|k| k.count).sum();
        let header = page::header(
            "Improve",
            "Decision model",
            None,
            page::context(format!(
                "{judgements} judgements in {} kinds; they leave the server only in an export you start.",
                self.kinds.len()
            )),
            page::primary_button(
                "Export training set",
                (!self.exporting).then_some(AppMessage::DecisionModel(Message::Export)),
            ),
        );
        let mut content = column![].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(failed) = &self.failed {
            content = content.push(page::boxed(
                "The export was refused",
                text(failed.as_str())
                    .size(theme::BODY)
                    .color(theme::RED_TEXT),
            ));
        }
        let content = content
            .push(self.kinds_box())
            .push(self.exports_box())
            .push(self.shadowing_box());
        column![
            header,
            scrollable(container(content).padding(Padding::default().right(theme::SCROLL_GUTTER)))
                .height(Length::Fill)
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
    }

    fn shadowing_box(&self) -> Element<'_, AppMessage> {
        let Some(shadowing) = self.shadowing.as_ref().filter(|s| s.configured) else {
            return page::boxed("Shadowing", page::body("No decision model is configured."));
        };
        let needed = shadowing.agreement.unwrap_or(1.0);
        let mut content = column![page::body(format!(
            "{} decides alongside the current decider on judgements held out of training. A kind is proposed in the inbox at {} of them with at least {} agreement.",
            shadowing.model.as_deref().unwrap_or("The model"),
            shadowing.min_judgements.unwrap_or_default(),
            percent(needed),
        ))]
        .spacing(theme::SPACE_8);
        if shadowing.kinds.is_empty() {
            content = content.push(page::body("Nothing has been shadowed yet."));
        } else {
            let mut table = column![
                row![
                    head_cell(SHADOWS[0], "Kind"),
                    head_cell(SHADOWS[1], "Shadows"),
                    head_cell(SHADOWS[2], "Agreement"),
                    head_cell(SHADOWS[3], "Decided by"),
                ]
                .spacing(theme::SPACE_8),
                page::rule(),
            ]
            .spacing(theme::SPACE_8);
            for kind in &shadowing.kinds {
                table = table.push(shadow_row(kind, needed)).push(page::rule());
            }
            content = content.push(table);
        }
        let merged: Vec<&Takeover> = shadowing
            .takeovers
            .iter()
            .filter(|t| t.status == "merged")
            .collect();
        if let Some(failed) = &self.release_failed {
            content = content.push(
                text(failed.as_str())
                    .size(theme::BODY)
                    .color(theme::RED_TEXT),
            );
        }
        for takeover in merged {
            content = content.push(
                row![
                    text(format!(
                        "The decision model decides {}: it agreed on {} of {} judgements.",
                        takeover.kind,
                        percent(takeover.agreement),
                        takeover.count
                    ))
                    .size(theme::BODY)
                    .color(theme::TEXT)
                    .width(Length::Fill),
                    page::white_button(
                        "Release",
                        Some(AppMessage::DecisionModel(Message::Release(takeover.id)))
                    ),
                ]
                .spacing(theme::SPACE_8)
                .align_y(Alignment::Center),
            );
        }
        page::boxed("Shadowing", content)
    }

    /// The route that downloads the file of `kind` in the newest export that has one.
    fn route(&self, kind: &str) -> Option<String> {
        self.exports
            .iter()
            .find(|export| export.kinds.iter().any(|k| k.kind == kind))
            .map(|export| format!("GET /v1/exports/{}/{kind}.jsonl", export.id))
    }

    fn kinds_box(&self) -> Element<'_, AppMessage> {
        if self.kinds.is_empty() {
            return page::boxed(
                "Kinds of decision",
                page::body("Nothing has been decided yet."),
            );
        }
        let mut table = column![
            row![
                head_cell(KINDS[0], "Kind"),
                head_cell(KINDS[1], "Judgements"),
                head_cell(KINDS[2], "With an outcome"),
                head_cell(KINDS[3], "By a person"),
                head_cell(KINDS[4], "Held out"),
                head_cell(KINDS[5], "Its file"),
            ]
            .spacing(theme::SPACE_8),
            page::rule(),
        ]
        .spacing(theme::SPACE_8);
        for kind in &self.kinds {
            let file = match self.route(&kind.kind) {
                Some(route) => mono(route, theme::TEXT_SECONDARY),
                None => text("Not exported yet")
                    .size(theme::BODY)
                    .color(theme::TEXT_MUTED)
                    .into(),
            };
            table = table
                .push(
                    row![
                        cell(
                            KINDS[0],
                            text(kind.kind.as_str())
                                .size(theme::BODY)
                                .font(theme::manrope(Weight::Semibold))
                        ),
                        cell(KINDS[1], plain(kind.count.to_string())),
                        cell(KINDS[2], plain(kind.with_outcome.to_string())),
                        cell(KINDS[3], plain(kind.by_person.to_string())),
                        cell(KINDS[4], plain(kind.held_out.to_string())),
                        cell(KINDS[5], file),
                    ]
                    .spacing(theme::SPACE_8),
                )
                .push(page::rule());
        }
        page::boxed(
            "Kinds of decision",
            column![
                table,
                page::body("The files stay on the server. To write them to a directory, run:"),
                mono("moebius export --out <directory>".to_string(), theme::TEXT),
            ]
            .spacing(theme::SPACE_8),
        )
    }

    fn exports_box(&self) -> Element<'_, AppMessage> {
        if self.exports.is_empty() {
            return page::boxed("Exports", page::body("No export has been made yet."));
        }
        let mut list = column![
            row![
                head_cell(EXPORTS[0], "Export"),
                head_cell(EXPORTS[1], "When"),
                head_cell(EXPORTS[2], "Judgements"),
                head_cell(EXPORTS[3], "Held out"),
                head_cell(EXPORTS[4], "Kinds"),
            ]
            .spacing(theme::SPACE_8),
            page::rule(),
        ]
        .spacing(theme::SPACE_8);
        for export in &self.exports {
            let count: i64 = export.kinds.iter().map(|k| k.count).sum();
            let held_out: i64 = export.kinds.iter().map(|k| k.held_out).sum();
            let kinds: Vec<&str> = export.kinds.iter().map(|k| k.kind.as_str()).collect();
            list = list
                .push(
                    row![
                        cell(
                            EXPORTS[0],
                            text(format!("#{}", export.id))
                                .size(theme::BODY)
                                .font(theme::manrope(Weight::Semibold))
                        ),
                        cell(EXPORTS[1], plain(when(export.at))),
                        cell(EXPORTS[2], plain(count.to_string())),
                        cell(EXPORTS[3], plain(held_out.to_string())),
                        cell(EXPORTS[4], plain(kinds.join(", "))),
                    ]
                    .spacing(theme::SPACE_8),
                )
                .push(page::rule());
        }
        page::boxed("Exports", list)
    }
}

const SHADOWS: [u16; 4] = [3, 2, 5, 3];
const KINDS: [u16; 6] = [3, 2, 2, 2, 2, 7];
const EXPORTS: [u16; 5] = [1, 3, 2, 2, 8];

/// A share between 0 and 1 as a percentage, such as `97%` or `96.5%`.
fn percent(share: f64) -> String {
    let percent = (share * 1000.0).round() / 10.0;
    if percent.fract() == 0.0 {
        format!("{percent:.0}%")
    } else {
        format!("{percent:.1}%")
    }
}

/// One kind of the shadowing table: its shadows, its agreement with a thin bar against the share a takeover needs, and
/// who decides it.
fn shadow_row(kind: &ShadowedKind, needed: f64) -> Element<'_, AppMessage> {
    let agreement: Element<AppMessage> = match kind.agreement {
        Some(share) => {
            let enough = share >= needed;
            column![
                text(format!("{} (needs {})", percent(share), percent(needed)))
                    .size(theme::BODY)
                    .color(if enough {
                        theme::TEAL
                    } else {
                        theme::TEXT_SECONDARY
                    }),
                bar(share, needed),
            ]
            .spacing(theme::SPACE_6)
            .into()
        }
        None => plain("No shadow yet".to_string()).into(),
    };
    let mut shadows = vec![kind.shadows.to_string()];
    if kind.failed > 0 {
        shadows.push(format!("{} failed", kind.failed));
    }
    if kind.dropped > 0 {
        shadows.push(format!("{} dropped", kind.dropped));
    }
    row![
        cell(
            SHADOWS[0],
            text(kind.kind.as_str())
                .size(theme::BODY)
                .font(theme::manrope(Weight::Semibold))
        ),
        cell(SHADOWS[1], plain(shadows.join(", "))),
        cell(SHADOWS[2], agreement),
        cell(
            SHADOWS[3],
            plain(
                if kind.routed {
                    "The decision model"
                } else {
                    "As before"
                }
                .to_string()
            )
        ),
    ]
    .spacing(theme::SPACE_8)
    .into()
}

/// A thin bar filled up to `share`, with a mark at `needed`.
fn bar<'a>(share: f64, needed: f64) -> Element<'a, AppMessage> {
    let parts = |share: f64| (share.clamp(0.0, 1.0) * 1000.0).round() as u16;
    let (filled, mark) = (parts(share), parts(needed));
    let segment = |portion: u16, color: iced::Color| {
        container(space())
            .width(Length::FillPortion(portion.max(1)))
            .height(theme::BAR_HEIGHT)
            .style(move |_| container::Style {
                background: Some(color.into()),
                ..container::Style::default()
            })
    };
    let track = row![
        segment(
            filled,
            if share >= needed {
                theme::TEAL
            } else {
                theme::AMBER
            }
        ),
        segment(1000 - filled, theme::TRACK),
    ];
    let threshold = row![
        space().width(Length::FillPortion(mark.saturating_sub(1).max(1))),
        container(space())
            .width(theme::BAR_MARK)
            .height(theme::BAR_HEIGHT)
            .style(|_| container::Style {
                background: Some(theme::TEXT.into()),
                ..container::Style::default()
            }),
        space().width(Length::FillPortion(1000 - mark.min(999))),
    ];
    stack![track, threshold].width(Length::Fill).into()
}

fn cell<'a>(portion: u16, content: impl Into<Element<'a, AppMessage>>) -> Element<'a, AppMessage> {
    container(content.into())
        .width(Length::FillPortion(portion))
        .into()
}

fn head_cell(portion: u16, label: &'static str) -> Element<'static, AppMessage> {
    cell(
        portion,
        text(label)
            .size(theme::LABEL)
            .font(theme::manrope(Weight::Semibold))
            .color(theme::TEXT_MUTED),
    )
}

fn plain<'a>(line: String) -> iced::widget::Text<'a> {
    text(line)
        .size(theme::BODY)
        .color(theme::TEXT_SECONDARY)
        .wrapping(Wrapping::WordOrGlyph)
}

fn mono<'a>(line: String, color: iced::Color) -> Element<'a, AppMessage> {
    text(line)
        .size(theme::CODE_SMALL)
        .font(theme::MONO)
        .color(color)
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

/// `at` (Unix seconds) as a date and time in UTC, such as `2026-10-28 10:15 UTC`.
fn when(at: i64) -> String {
    let (days, seconds) = (at.div_euclid(86_400), at.rem_euclid(86_400));
    // Days since 1970-01-01 to a civil date.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year}-{month:02}-{day:02} {:02}:{:02} UTC",
        seconds / 3600,
        seconds % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::when;

    #[test]
    fn a_time_is_shown_as_a_date_in_utc() {
        assert_eq!(when(0), "1970-01-01 00:00 UTC");
        assert_eq!(when(951_782_400 + 86_399), "2000-02-29 23:59 UTC");
        assert_eq!(when(1_793_182_500), "2026-10-28 10:15 UTC");
    }
}
