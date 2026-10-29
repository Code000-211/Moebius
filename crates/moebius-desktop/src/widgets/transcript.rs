//! How a conversation's tool calls, errors and messages are drawn, for every screen that follows a run: a tool call
//! with the decision of its check, a refused one as a broken rule, a diff, a written file and what a tool printed.

use iced::font::Weight;
use iced::highlighter::{self, Stream};
use iced::widget::text::Wrapping;
use iced::widget::{column, container, rich_text, row, scrollable, space, span, text};
use iced::{Alignment, Border, Color, Element, Font, Length};

use crate::app::Message as AppMessage;
use crate::theme;
use crate::widgets::conversation::{Execution, Input, ToolCall};
use crate::widgets::page::{Tone, tag};

/// The most lines of a tool's output, or of a diff or a file, drawn; the rest is said to be left out.
const MAX_LINES: usize = 200;

/// A message in a box with a small label above it.
pub fn note<'a>(
    label: &'a str,
    message: &'a str,
    fill: Color,
    border: Color,
    ink: Color,
) -> Element<'a, AppMessage> {
    container(
        column![
            text(label)
                .size(theme::LABEL)
                .font(theme::manrope(Weight::Bold))
                .color(ink),
            text(message).size(theme::CONTEXT).color(theme::TEXT),
        ]
        .spacing(4),
    )
    .padding([10, 12])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(fill.into()),
        border: Border {
            color: border,
            width: 1.0,
            radius: theme::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// The name of a tool in its pill.
fn tool_name<'a>(name: &'a str, fill: Color) -> Element<'a, AppMessage> {
    container(
        text(name)
            .font(theme::MONO)
            .size(theme::CODE)
            .color(theme::TEXT),
    )
    .padding([2, 8])
    .style(move |_| container::Style {
        background: Some(fill.into()),
        border: Border {
            radius: theme::RADIUS_ROUND.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// The first row of a tool call: its name, what it works on, and on the right what the check and the tool did.
fn head<'a>(call: &'a ToolCall, blocked: bool) -> Element<'a, AppMessage> {
    let fill = if blocked {
        theme::SURFACE
    } else {
        theme::HEAD_PILL
    };
    let arg = text(call.input.summary())
        .font(theme::MONO)
        .size(theme::CODE)
        .wrapping(Wrapping::None)
        .color(if blocked {
            theme::RED_TEXT
        } else {
            theme::TEXT_SECONDARY
        });
    let mut right = row![].spacing(8).align_y(Alignment::Center);
    match (&call.check, &call.execution) {
        (Some(check), _) if !check.allowed => {
            right = right.push(tag("Blocked".to_string(), Tone::Red))
        }
        (_, Some(Execution::Running)) => right = right.push(tag("Running".to_string(), Tone::Teal)),
        (Some(_), _) => right = right.push(tag("Allowed".to_string(), Tone::Teal)),
        _ => {}
    }
    let mut notes = Vec::new();
    if let Some(decider) = call.check.as_ref().and_then(|c| c.decider.as_deref()) {
        notes.push(capitalized(decider));
    }
    if !blocked
        && let Some(Execution::Finished {
            millis: Some(ms),
            failed,
        }) = &call.execution
    {
        notes.push(match failed {
            true => format!("failed after {}", duration(*ms)),
            false => format!("ran {}", duration(*ms)),
        });
    }
    if !notes.is_empty() {
        right = right.push(
            text(notes.join(" · "))
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED),
        );
    }
    row![
        tool_name(call.tool.as_str(), fill),
        container(arg).width(Length::Fill).clip(true),
        right,
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// A decider's name as the page writes it: `patterns` is the Pattern check.
fn capitalized(word: &str) -> String {
    let word = if word == "patterns" { "pattern" } else { word };
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A time in the unit a person reads: `12 ms`, `0.4 s`, `14 s`.
fn duration(ms: u64) -> String {
    match ms {
        0..1000 => format!("{ms} ms"),
        1000..10_000 => format!("{:.1} s", ms as f64 / 1000.0),
        _ => format!("{} s", ms / 1000),
    }
}

/// A tool call: the red box of a broken rule when its check refused it, else the call with its check and what it did.
pub fn tool_call(call: &ToolCall) -> Element<'_, AppMessage> {
    if call.refused() {
        refused_view(call)
    } else {
        tool_view(call)
    }
}

/// The error that ended a run.
pub fn error(message: &str) -> Element<'_, AppMessage> {
    note(
        "The run failed",
        message,
        theme::RED_FILL,
        theme::RED_BORDER,
        theme::RED_TEXT,
    )
}

fn tool_view(call: &ToolCall) -> Element<'_, AppMessage> {
    let mut content = column![head(call, false)].spacing(8);
    match &call.input {
        Input::Edit { path, changes } => content = content.push(diff_view(path, changes)),
        Input::Write {
            path,
            content: file,
        } => {
            content = content.push(file_view(path, file));
        }
        _ if !call.output.is_empty() && !matches!(call.input, Input::Read(_)) => {
            content = content.push(output_view(&call.output));
        }
        _ => {}
    }
    container(content)
        .padding([10, 12])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::LANE.into()),
            border: Border {
                color: theme::TRACK,
                width: 1.0,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// All of a tool call that its line does not say, for when its step is unfolded: why a refusal happened, the input
/// (the command, the diff, the file written) and what the tool printed (a file read, shown as that file).
pub(crate) fn details(call: &ToolCall) -> Element<'_, AppMessage> {
    let mut content = column![].spacing(8);
    if let Some(check) = call.check.as_ref().filter(|check| !check.allowed) {
        let why = match check.rule() {
            Some((rule, why)) => format!("Rule broken: {rule}. {why}"),
            None => format!("Refused. {}", check.reason),
        };
        content = content.push(text(why).size(theme::CONTEXT).color(theme::RED_TEXT));
        if let Some(replay) = &check.replay {
            content = content.push(
                text(format!("Replayed: {replay}"))
                    .size(theme::LABEL)
                    .color(theme::TEXT_MUTED),
            );
        }
    }
    let who = check_line(call);
    if !who.is_empty() {
        content = content.push(text(who).size(theme::LABEL).color(theme::TEXT_MUTED));
    }
    let printed = !call.output.is_empty();
    match &call.input {
        Input::Edit { path, changes } => content = content.push(diff_view(path, changes)),
        Input::Write {
            path,
            content: file,
        } => content = content.push(file_view(path, file)),
        Input::Read(path) if printed => content = content.push(file_view(path, &call.output)),
        Input::Command(command) => {
            content = content.push(command_view(command));
            if printed {
                content = content.push(output_view(&call.output));
            }
        }
        Input::Other(json) => {
            content = content.push(code_block("json", json));
            if printed {
                content = content.push(output_view(&call.output));
            }
        }
        _ if printed => content = content.push(output_view(&call.output)),
        _ => {}
    }
    content.into()
}

/// Who checked a call and how long it ran, as the head of a tool call says it: `Pattern · ran 3 ms`.
fn check_line(call: &ToolCall) -> String {
    let mut notes = Vec::new();
    if let Some(decider) = call.check.as_ref().and_then(|c| c.decider.as_deref()) {
        notes.push(capitalized(decider));
    }
    if let Some(Execution::Finished {
        millis: Some(ms),
        failed,
    }) = &call.execution
    {
        notes.push(match failed {
            true => format!("failed after {}", duration(*ms)),
            false => format!("ran {}", duration(*ms)),
        });
    }
    notes.join(" · ")
}

/// A refused call: the red box with the rule broken and why. The call did not run.
fn refused_view(call: &ToolCall) -> Element<'_, AppMessage> {
    let check = call.check.as_ref().expect("a refused call has its check");
    let (title, why) = match check.rule() {
        Some((rule, why)) => (
            row![
                text("Rule broken: ")
                    .size(13.5)
                    .font(theme::manrope(Weight::Bold))
                    .color(theme::RED_TEXT),
                text(rule)
                    .font(theme::MONO)
                    .size(theme::CODE)
                    .color(theme::RED_TEXT),
            ],
            why,
        ),
        None => (
            row![
                text("Refused")
                    .size(13.5)
                    .font(theme::manrope(Weight::Bold))
                    .color(theme::RED_TEXT)
            ],
            check.reason.as_str(),
        ),
    };
    let dot = container(space().width(8).height(8)).style(|_| container::Style {
        background: Some(theme::RED.into()),
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    });
    let mut content = column![
        row![dot, title].spacing(8).align_y(Alignment::Center),
        head(call, true),
        text(why).size(theme::CONTEXT).color(theme::TEXT),
    ]
    .spacing(8);
    if let Some(replay) = &check.replay {
        content = content.push(
            text(format!("Replayed: {replay}"))
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED),
        );
    }
    container(content)
        .padding([10, 12])
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::RED_FILL.into()),
            border: Border {
                color: theme::RED_BORDER,
                width: 1.5,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

/// The lines of the last `MAX_LINES`, with a note when earlier ones are left out.
fn tail<'a>(lines: &[&'a str]) -> (usize, Vec<&'a str>) {
    let skipped = lines.len().saturating_sub(MAX_LINES);
    (skipped, lines[skipped..].to_vec())
}

/// What a tool printed, in the dark block: the last lines, scrollable.
pub(crate) fn output_view(output: &str) -> Element<'_, AppMessage> {
    let (skipped, lines) = tail(&lines_of(output));
    let mut block = column![].spacing(0);
    if skipped > 0 {
        block = block.push(
            text(format!("{skipped} earlier lines left out"))
                .font(theme::MONO)
                .size(theme::CODE_SMALL)
                .color(theme::TERMINAL_DIM),
        );
    }
    block = block.push(
        text(lines.join("\n"))
            .font(theme::MONO)
            .size(theme::CODE_SMALL)
            .line_height(1.6)
            .color(theme::TERMINAL_TEXT),
    );
    container(scrollable(
        container(block).padding([10, 12]).width(Length::Fill),
    ))
    .max_height(240)
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(theme::TERMINAL.into()),
        border: Border {
            radius: theme::RADIUS_CONTROL.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// The highlighted spans of each line of `lines`, for the language of `path`'s extension.
fn highlighted<'a>(lines: &[&str], path: &str) -> Vec<Vec<iced::widget::text::Span<'a, (), Font>>> {
    let token = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt");
    highlighted_as(lines, token)
}

/// The language the highlighter knows for the name or extension `token`. It ships the syntaxes of a text editor's
/// defaults, which has neither Luau, TOML nor TypeScript; each is shown as the closest one it has.
pub fn language(token: &str) -> &str {
    match token.to_ascii_lowercase().as_str() {
        "luau" => "lua",
        "toml" | "cfg" | "conf" => "ini",
        "ts" | "tsx" | "jsx" | "mjs" | "cjs" | "javascript" | "typescript" => "js",
        "jsonc" | "json5" => "json",
        "shell" | "zsh" | "console" => "sh",
        "hpp" | "cc" | "cxx" | "cu" | "cuh" | "hip" => "cpp",
        "yml" => "yaml",
        "markdown" => "md",
        "patch" => "diff",
        _ => token,
    }
}

/// The highlighted spans of each line of `lines`, for the language `token` names.
fn highlighted_as<'a>(
    lines: &[&str],
    token: &str,
) -> Vec<Vec<iced::widget::text::Span<'a, (), Font>>> {
    let token = language(token).to_string();
    let mut stream = Stream::new(&highlighter::Settings {
        theme: highlighter::Theme::InspiredGitHub,
        token,
    });
    lines
        .iter()
        .map(|line| {
            let spans: Vec<_> = stream
                .highlight_line(line)
                .map(|(range, highlight)| {
                    let mut piece = span(line[range].to_string()).font(theme::MONO);
                    if let Some(color) = highlight.color() {
                        piece = piece.color(color);
                    }
                    if let Some(font) = highlight.font() {
                        piece = piece.font(Font {
                            family: theme::MONO.family,
                            ..font
                        });
                    }
                    piece
                })
                .collect();
            stream.commit();
            if spans.is_empty() {
                vec![span(" ".to_string()).font(theme::MONO)]
            } else {
                spans
            }
        })
        .collect()
}

/// One line of code: the marker (`+`, `-` or a space), the line, and the fill of the row.
fn code_row<'a>(
    marker: char,
    spans: Vec<iced::widget::text::Span<'a, (), Font>>,
    fill: Option<Color>,
) -> Element<'a, AppMessage> {
    let ink = match marker {
        '-' => theme::DIFF_REMOVED_MARK,
        '+' => theme::DIFF_ADDED_MARK,
        _ => theme::TEXT_MUTED,
    };
    container(
        row![
            text(marker.to_string())
                .font(theme::MONO)
                .size(theme::CODE)
                .color(ink)
                .width(24)
                .align_x(Alignment::Center),
            rich_text(spans)
                .size(theme::CODE)
                .line_height(1.6)
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: fill.map(Into::into),
        ..container::Style::default()
    })
    .into()
}

/// Code in a white frame under a header line with the path on the left and `note` on the right. The rows are
/// scrollable, and `left_out` lines that were not drawn are said so at the end.
fn code_frame<'a>(
    path: &'a str,
    note: Option<String>,
    rows: Vec<Element<'a, AppMessage>>,
    left_out: usize,
) -> Element<'a, AppMessage> {
    let mut title = row![
        text(path)
            .font(theme::MONO)
            .size(theme::CODE_SMALL)
            .color(theme::TEXT_SECONDARY),
        space::horizontal(),
    ];
    if let Some(note) = note {
        title = title.push(
            text(note)
                .font(theme::MONO)
                .size(theme::CODE_SMALL)
                .color(theme::TEXT_SECONDARY),
        );
    }
    let mut body = column(rows);
    if left_out > 0 {
        body = body.push(
            container(
                text(format!("{left_out} more lines left out"))
                    .font(theme::MONO)
                    .size(theme::CODE_SMALL)
                    .color(theme::TEXT_MUTED),
            )
            .padding([4, 10]),
        );
    }
    container(column![
        container(title)
            .padding([6, 10])
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(theme::LANE.into()),
                ..container::Style::default()
            }),
        container(body).padding([4, 0]),
    ])
    .width(Length::Fill)
    .clip(true)
    .style(|_| container::Style {
        background: Some(theme::SURFACE.into()),
        border: Border {
            color: theme::TRACK,
            width: 1.0,
            radius: theme::RADIUS_CONTROL.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// A file's first lines, highlighted for its language.
pub(crate) fn file_view<'a>(path: &'a str, file: &str) -> Element<'a, AppMessage> {
    let lines = lines_of(file);
    let rows = highlighted(&lines[..lines.len().min(MAX_LINES)], path)
        .into_iter()
        .map(|spans| code_row(' ', spans, None))
        .collect();
    code_frame(path, None, rows, lines.len().saturating_sub(MAX_LINES))
}

/// A command in the frame of a file, highlighted as shell.
pub(crate) fn command_view(command: &str) -> Element<'_, AppMessage> {
    code_block("sh", command)
}

/// A code block of the agent's text: the first lines of `code`, highlighted for `language`, in the frame of a file.
pub fn code_block<'a>(language: &'a str, code: &str) -> Element<'a, AppMessage> {
    let lines = lines_of(code);
    let rows = highlighted_as(&lines[..lines.len().min(MAX_LINES)], language)
        .into_iter()
        .map(|spans| code_row(' ', spans, None))
        .collect();
    code_frame(language, None, rows, lines.len().saturating_sub(MAX_LINES))
}

/// An edit's changes as removed and added lines, each side highlighted for the file's language.
pub(crate) fn diff_view<'a>(
    path: &'a str,
    changes: &[(String, String)],
) -> Element<'a, AppMessage> {
    let mut rows: Vec<Element<'a, AppMessage>> = Vec::new();
    let (mut removed, mut added) = (0, 0);
    for (old, new) in changes {
        let (old, new) = (lines_of(old), lines_of(new));
        removed += old.len();
        added += new.len();
        // Past the cap the lines are counted but not highlighted or drawn.
        let room = MAX_LINES.saturating_sub(rows.len());
        for spans in highlighted(&old[..old.len().min(room)], path) {
            rows.push(code_row('-', spans, Some(theme::DIFF_REMOVED)));
        }
        let room = MAX_LINES.saturating_sub(rows.len());
        for spans in highlighted(&new[..new.len().min(room)], path) {
            rows.push(code_row('+', spans, Some(theme::DIFF_ADDED)));
        }
    }
    let left_out = removed + added - rows.len();
    code_frame(path, Some(format!("-{removed} +{added}")), rows, left_out)
}
