//! The Decision model screen: what the server has recorded of each kind of decision, the training-set exports a person
//! started, and (later) the shadowing of a trained model. "Export training set" is `POST /v1/exports`; the files stay on
//! the server, so each kind shows the route that downloads its file and the command that writes them elsewhere.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length, Padding, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Export, KindCounts};
use crate::page;
use crate::settings::Settings;
use crate::theme;

#[derive(Debug, Clone)]
pub enum Message {
    Kinds(Result<Vec<KindCounts>, ClientError>),
    Exports(Result<Vec<Export>, ClientError>),
    /// "Export training set" was pressed.
    Export,
    Exported(Result<Export, ClientError>),
}

#[derive(Default)]
pub struct DecisionModel {
    kinds: Vec<KindCounts>,
    exports: Vec<Export>,
    /// Whether an export is being written.
    exporting: bool,
    /// Why the last export was refused.
    failed: Option<String>,
}

impl DecisionModel {
    /// Reads the counts and the exports again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let (kinds, exports) = (Client::new(settings.clone()), Client::new(settings.clone()));
        Task::batch([
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
        )
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Kinds(Ok(kinds)) => self.kinds = kinds,
            Message::Exports(Ok(exports)) => self.exports = exports,
            Message::Kinds(Err(_)) | Message::Exports(Err(_)) => {}
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
                "{judgements} judgements recorded in {} kinds; they leave the server only in an export you start.",
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
            .push(page::boxed(
                "Shadowing",
                page::body("No decision model is configured."),
            ));
        column![
            header,
            scrollable(container(content).padding(Padding::default().right(12)))
                .height(Length::Fill)
        ]
        .spacing(theme::PAGE_GAP)
        .width(Length::Fill)
        .into()
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
            .spacing(10),
            page::rule(),
        ]
        .spacing(8);
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
                    .spacing(10),
                )
                .push(page::rule());
        }
        page::boxed(
            "Kinds of decision",
            column![
                table,
                page::body(
                    "The files stay on the server. To write them to a directory yourself, run:"
                ),
                mono("moebius export --out <directory>".to_string(), theme::TEXT),
            ]
            .spacing(10),
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
            .spacing(10),
            page::rule(),
        ]
        .spacing(8);
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
                    .spacing(10),
                )
                .push(page::rule());
        }
        page::boxed("Exports", list)
    }
}

const KINDS: [u16; 6] = [3, 2, 2, 2, 2, 7];
const EXPORTS: [u16; 5] = [1, 3, 2, 2, 8];

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
