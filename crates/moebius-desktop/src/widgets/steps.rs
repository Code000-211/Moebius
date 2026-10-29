//! What an agent did between two of its answers, drawn as a light-blue box of one line per step: its thoughts and its
//! tool calls, each folded to a line a click opens to the whole of it (the arguments, the diff, the full result). The
//! chat draws it for every run of steps in its conversation; a screen with small tiles draws it `compact`.

use std::collections::HashSet;

use iced::font::{Style, Weight};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Alignment, Border, Element, Font, Length};

use crate::app::Message as AppMessage;
use crate::widgets::conversation::{Entry, Execution, Input, ToolCall};
use crate::widgets::icons::Icon;
use crate::widgets::page::{Tone, tag};
use crate::{theme, widgets::transcript};

/// The most characters of a folded line, which the box clips further when it is narrower.
const LINE: usize = 140;
/// The same in a compact box, which shows only the last steps and has no unfolding.
const COMPACT_LINE: usize = 44;
const COMPACT_STEPS: usize = 4;
/// The height at which a box that is not compact starts to scroll.
const MAX_HEIGHT: f32 = 380.0;

/// A time as a person reads it: `42s`, `7m 17s`, `1h 02m`.
pub fn elapsed(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3600, seconds / 60 % 60),
    }
}

fn shorten(line: &str, limit: usize) -> String {
    match line.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", line[..end].trim_end()),
        None => line.to_string(),
    }
}

fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn italic() -> Font {
    Font {
        style: Style::Italic,
        ..theme::MANROPE
    }
}

/// The box of the steps `group`, which stand at `first` and after in the conversation (a step is known by its index
/// there, and `unfolded` holds the open ones; `toggle` makes the message of a click on a step). `working` says the
/// agent is still at it, and `seconds` how long it has been. A `compact` box has no clicks: it shows the header and
/// the last steps, one line each.
pub fn steps_box<'a>(
    group: &'a [Entry],
    first: usize,
    working: bool,
    seconds: i64,
    unfolded: &HashSet<usize>,
    toggle: fn(usize) -> AppMessage,
    compact: bool,
) -> Element<'a, AppMessage> {
    let calls = group
        .iter()
        .filter(|entry| matches!(entry, Entry::Tool(_)))
        .count();
    let counted = match calls {
        1 => "1 tool call".to_string(),
        n => format!("{n} tool calls"),
    };
    let time = elapsed(seconds);
    let note = |line: String| text(line).size(theme::LABEL).color(theme::TEXT_SECONDARY);
    let (left, right) = if working {
        (
            "Agent is working".to_string(),
            row![note(time), note(counted)].spacing(12),
        )
    } else {
        (format!("Worked for {time}"), row![note(counted)])
    };
    let mut title = row![Icon::Agents.view(theme::STEPS_INK, 18.0)]
        .spacing(8)
        .align_y(Alignment::Center);
    if working {
        title = title.push(Icon::Spinner.view(theme::STEPS_INK, 14.0));
    }
    let header = row![
        title.push(
            text(left)
                .size(theme::CONTEXT)
                .font(theme::manrope(Weight::Bold))
                .color(theme::STEPS_INK),
        ),
        space::horizontal(),
        right,
    ]
    .align_y(Alignment::Center);

    let skip = if compact {
        group.len().saturating_sub(COMPACT_STEPS)
    } else {
        0
    };
    let steps = column(
        group
            .iter()
            .enumerate()
            .skip(skip)
            .filter_map(|(at, entry)| {
                let id = first + at;
                step(entry, id, unfolded.contains(&id), toggle, compact)
            }),
    )
    .spacing(if compact { 2 } else { 6 });
    let steps: Element<'a, AppMessage> = if compact {
        steps.into()
    } else {
        container(scrollable(steps))
            .max_height(MAX_HEIGHT)
            .width(Length::Fill)
            .into()
    };
    container(column![header, steps].spacing(10))
        .padding([12, 14])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::STEPS_FILL.into()),
            border: Border {
                color: theme::STEPS_BORDER,
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// One step: its line (and the line of its result), and when `open` all of it under them. None for an entry that is
/// not a step.
fn step<'a>(
    entry: &'a Entry,
    id: usize,
    open: bool,
    toggle: fn(usize) -> AppMessage,
    compact: bool,
) -> Option<Element<'a, AppMessage>> {
    let limit = if compact { COMPACT_LINE } else { LINE };
    let size = if compact {
        theme::LABEL
    } else {
        theme::CONTEXT
    };
    let (line, result, body): (Element<'a, AppMessage>, _, _) = match entry {
        Entry::Thinking(thought) => (
            row![
                Icon::Spark.view(theme::TEXT_MUTED, 14.0),
                text(shorten(first_line(thought), limit))
                    .size(size)
                    .font(italic())
                    .wrapping(Wrapping::None)
                    .color(theme::TEXT_MUTED),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
            None,
            Some(
                text(thought.trim())
                    .size(size)
                    .font(italic())
                    .color(theme::TEXT_MUTED)
                    .into(),
            ),
        ),
        Entry::Tool(call) => (
            tool_line(call, limit, size),
            result_line(call, limit),
            Some(transcript::details(call)),
        ),
        _ => return None,
    };
    if compact {
        return Some(line);
    }
    let chevron = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    let mut head = column![
        row![chevron.view(theme::TEXT_MUTED, 14.0), line]
            .spacing(6)
            .align_y(Alignment::Center)
    ]
    .spacing(2);
    if let (false, Some(result)) = (open, result) {
        head = head.push(container(result).padding([0, 20]));
    }
    let head = button(head)
        .width(Length::Fill)
        .padding([3, 4])
        .on_press(toggle(id))
        .style(|_, status| button::Style {
            background: matches!(status, button::Status::Hovered)
                .then(|| theme::SURFACE.scale_alpha(0.6).into()),
            text_color: theme::TEXT,
            border: Border {
                radius: theme::RADIUS_SMALL.into(),
                ..Border::default()
            },
            ..button::Style::default()
        });
    let mut step = column![head].spacing(6);
    if let (true, Some(body)) = (open, body) {
        step = step.push(container(body).padding([0, 24]));
    }
    Some(step.into())
}

/// The line of a tool call: its name in bold, what it works on, and a tag where the check refused it or it still runs.
fn tool_line(call: &ToolCall, limit: usize, size: f32) -> Element<'_, AppMessage> {
    let refused = call.refused();
    let mut line = row![
        text(match call.mcp {
            Some(_) => call.title(),
            None => capitalized(&call.tool),
        })
        .size(size)
        .font(theme::manrope(Weight::Bold))
        .color(theme::TEXT),
        text(shorten(&call.input.summary(), limit))
            .size(size)
            .wrapping(Wrapping::None)
            .color(if refused {
                theme::RED_TEXT
            } else {
                theme::TEXT
            }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if refused {
        line = line.push(tag("refused".to_string(), Tone::Red));
    } else if call.execution == Some(Execution::Running) {
        line = line.push(tag("running".to_string(), Tone::Teal));
    } else if matches!(
        call.execution,
        Some(Execution::Finished { failed: true, .. })
    ) {
        line = line.push(tag("failed".to_string(), Tone::Amber));
    }
    line.into()
}

/// `result: ` and the first line of what the tool printed, cut off; none while it has printed nothing, or for a
/// call that was refused (it did not run) or whose result is its input (an edit's or a write's).
fn result_line(call: &ToolCall, limit: usize) -> Option<Element<'_, AppMessage>> {
    if call.refused() || matches!(call.input, Input::Unknown) {
        return None;
    }
    let first = first_line(&call.output);
    (!first.is_empty()).then(|| {
        text(format!("result:  {}", shorten(first, limit)))
            .size(theme::LABEL)
            .font(theme::MONO)
            .wrapping(Wrapping::None)
            .color(theme::TEXT_MUTED)
            .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_is_written_as_a_person_reads_it() {
        assert_eq!(elapsed(-3), "0s");
        assert_eq!(elapsed(42), "42s");
        assert_eq!(elapsed(437), "7m 17s");
        assert_eq!(elapsed(3720), "1h 02m");
    }

    #[test]
    fn a_long_line_is_cut_with_an_ellipsis() {
        assert_eq!(shorten("short", 10), "short");
        assert_eq!(shorten("abcdef ghij", 6), "abcdef…");
    }
}
