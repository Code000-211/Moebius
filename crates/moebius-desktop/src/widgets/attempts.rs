//! The attempts of a card that was tried several times at once, one line each: its run, the model it was started with,
//! and how it fared (which passed, which was picked), with the reason the picked one was kept and, under one that
//! failed, the first line of why. The live view's panel and the card's page both draw it.

use iced::widget::text::Wrapping;
use iced::widget::{column, row, space, text};
use iced::{Alignment, Element, Length};

use crate::client::Attempt;
use crate::theme;
use crate::widgets::page::{Tone, tag};

/// The most characters of the first line of why an attempt failed.
const WHY: usize = 120;

/// How the attempt stands, as the label and tone of its tag: picked, passed, failed, or what its run is doing.
fn standing(attempt: &Attempt) -> (&'static str, Tone) {
    match (attempt.picked, attempt.result.as_deref()) {
        (true, _) => ("picked", Tone::Teal),
        (false, Some("passed")) => ("passed", Tone::Plain),
        (false, Some(_)) => ("failed", Tone::Red),
        (false, None) => match attempt.status.as_str() {
            "starting" | "running" => ("running", Tone::Plain),
            "succeeded" => ("judging", Tone::Plain),
            _ => ("failed", Tone::Red),
        },
    }
}

/// The first line of `output`, cut to a length that fits.
fn first_line(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    Some(match line.char_indices().nth(WHY) {
        Some((end, _)) => format!("{}…", line[..end].trim_end()),
        None => line.to_string(),
    })
}

/// The attempts, one block each.
pub fn list<'a, Message: 'a>(attempts: &'a [Attempt]) -> Element<'a, Message> {
    column(attempts.iter().map(entry))
        .spacing(theme::SPACE_8)
        .width(Length::Fill)
        .into()
}

fn entry<'a, Message: 'a>(attempt: &'a Attempt) -> Element<'a, Message> {
    let (label, tone) = standing(attempt);
    let mut head = row![
        text(format!("run {}", attempt.run))
            .size(theme::LABEL)
            .font(theme::MONO)
            .color(theme::TEXT)
    ]
    .spacing(theme::SPACE_6)
    .align_y(Alignment::Center);
    if let Some(model) = &attempt.model {
        head = head.push(
            text(model.as_str())
                .size(theme::LABEL)
                .font(theme::MONO)
                .wrapping(Wrapping::None)
                .color(theme::TEXT_MUTED),
        );
    }
    let head = head
        .push(space().width(Length::Fill))
        .push(tag(label.to_string(), tone));
    let mut block = column![head].spacing(theme::SPACE_4);
    let why = match (attempt.picked, &attempt.reason) {
        (true, Some(reason)) if !reason.trim().is_empty() => {
            Some((reason.clone(), theme::TEXT_SECONDARY))
        }
        (false, _) if tone_is_red(tone) => {
            first_line(&attempt.output).map(|line| (line, theme::TEXT_MUTED))
        }
        _ => None,
    };
    if let Some((line, color)) = why {
        block = block.push(
            text(line)
                .size(theme::CONTEXT)
                .wrapping(Wrapping::WordOrGlyph)
                .color(color),
        );
    }
    block.into()
}

fn tone_is_red(tone: Tone) -> bool {
    matches!(tone, Tone::Red)
}
