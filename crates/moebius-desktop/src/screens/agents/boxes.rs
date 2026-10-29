//! The boxes under the editor's head: model and settings, instructions (written or asked of the Moebius agent), the
//! built-in instructions and skills, the skills and MCP servers chosen, and the paths of skills and extensions.

use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, pick_list, row, space, text, text_editor, text_input,
};
use iced::{Alignment, Border, Element, Length};

use crate::app::Message as AppMessage;
use crate::client::{AvailableModel, BuiltInAgent, McpServer, Runner, Skill};
use crate::widgets::form::{
    HARNESSES, LEVELS, PROTOCOLS, aside, box_heading, field, menu_style, pick_style,
};
use crate::widgets::inline_edit::editor_style;
use crate::widgets::list_editor::boxed;
use crate::widgets::model_picker;
use crate::widgets::page;
use crate::{screens::connect, theme};

use super::Agents;
use super::editor::Editor;
use super::editor::add_chosen;
use super::{COMPRESS, ENGINEERING, ENGINEERING_AGENTS, Message, NOT_SET, THIS_SERVER};

/// A row of a list: what names it, and a line under it that is red when something is wrong.
fn item<'a>(name: Element<'a, AppMessage>, line: &str, red: bool) -> Element<'a, AppMessage> {
    let mut content = column![name].spacing(3);
    if !line.is_empty() {
        content = content.push(text(line.to_string()).size(theme::LABEL).color(if red {
            theme::RED_TEXT
        } else {
            theme::TEXT_MUTED
        }));
    }
    content.into()
}

/// A box of the editor: a heading with a muted word on its right, and a list of items.
fn section<'a>(
    heading: &'a str,
    note: &str,
    items: impl Iterator<Item = Element<'a, AppMessage>>,
    empty: &'a str,
    footer: Option<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    let items: Vec<_> = items.collect();
    let list: Element<_> = if items.is_empty() {
        page::body(empty)
    } else {
        column(items).spacing(10).into()
    };
    let mut content = column![box_heading(heading, aside(note.to_string())), list].spacing(12);
    if let Some(footer) = footer {
        content = content.push(footer);
    }
    boxed(content)
}

fn settings_row<'a>(
    editor: &'a Editor,
    models: &[AvailableModel],
    runners: &[Runner],
) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Agents(message);
    let model = model_picker::field(
        models,
        editor.typing_model,
        "The model's name",
        &editor.model,
        move |model| message(Message::ModelEdited(model)),
        move |model| message(Message::ModelPicked(model)),
        message(Message::TypeModel),
    );
    let protocols: Vec<String> = PROTOCOLS.iter().map(|p| p.to_string()).collect();
    let protocol = pick_list(protocols, Some(editor.protocol.clone()), move |p| {
        message(Message::ProtocolChosen(p))
    })
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let harnesses: Vec<String> = HARNESSES.iter().map(|h| h.to_string()).collect();
    let harness = pick_list(harnesses, Some(editor.harness.clone()), move |h| {
        message(Message::HarnessChosen(h))
    })
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let mut levels: Vec<String> = std::iter::once(NOT_SET)
        .chain(LEVELS)
        .map(String::from)
        .collect();
    if let Some(level) = &editor.thinking
        && !levels.contains(level)
    {
        levels.push(level.clone());
    }
    let thinking = pick_list(
        levels,
        Some(
            editor
                .thinking
                .clone()
                .unwrap_or_else(|| NOT_SET.to_string()),
        ),
        move |level| message(Message::ThinkingChosen(level)),
    )
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let budget = text_input("No limit", &editor.budget)
        .on_input(move |budget| message(Message::BudgetEdited(budget)))
        .size(theme::BODY)
        .padding([9, 12])
        .style(connect::input_style);
    let number = |placeholder, value: &'a str, edited: fn(String) -> Message| {
        text_input(placeholder, value)
            .on_input(move |value| message(edited(value)))
            .size(theme::BODY)
            .padding([9, 12])
            .style(connect::input_style)
    };
    let compress = pick_list(
        COMPRESS.map(String::from).to_vec(),
        Some(COMPRESS[usize::from(!editor.compress)].to_string()),
        move |choice| message(Message::CompressChosen(choice)),
    )
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let mut settings = row![
        field("Model", model),
        field("Protocol", protocol),
        field("Harness", harness),
    ]
    .spacing(14);
    // Where its runs go: the server itself, or a machine of `[[runners]]` over SSH. Shown only when there are any (or
    // the agent names one that is gone, so that it can be taken off).
    if !runners.is_empty() || editor.runner.is_some() {
        let mut places: Vec<String> = std::iter::once(THIS_SERVER.to_string())
            .chain(runners.iter().map(|runner| runner.name.clone()))
            .collect();
        if let Some(runner) = &editor.runner
            && !places.contains(runner)
        {
            places.push(runner.clone());
        }
        let runner = pick_list(
            places,
            Some(
                editor
                    .runner
                    .clone()
                    .unwrap_or_else(|| THIS_SERVER.to_string()),
            ),
            move |runner| message(Message::RunnerChosen(runner)),
        )
        .text_size(theme::BODY)
        .padding([9, 12])
        .width(Length::Fill)
        .style(pick_style)
        .menu_style(menu_style);
        settings = settings.push(field("Runner", runner));
    }
    let settings = settings
        .push(field("Thinking", thinking))
        .push(field("Token budget per card", budget));
    // The skills Moebius brings for some of the built-in roles can be turned off for the agent.
    let engineering = ENGINEERING_AGENTS.contains(&editor.name.as_str()).then(|| {
        let given = pick_list(
            ENGINEERING.map(String::from).to_vec(),
            Some(ENGINEERING[usize::from(!editor.engineering_skills)].to_string()),
            move |choice| message(Message::EngineeringChosen(choice)),
        )
        .text_size(theme::BODY)
        .padding([9, 12])
        .width(Length::Fill)
        .style(pick_style)
        .menu_style(menu_style);
        row![
            field("Engineering skills", given),
            space().width(Length::FillPortion(4))
        ]
        .spacing(14)
    });
    // How the outputs of its tools are kept small (see the reference's `tool_output`): Pi-Bolt's bridge does it, so
    // an agent on another harness has no such settings.
    if editor.harness != HARNESSES[0] {
        return match engineering {
            Some(engineering) => column![settings, engineering].spacing(12).into(),
            None => settings.into(),
        };
    }
    let outputs = row![
        field(
            "Hide tool outputs after turns",
            number("0 keeps them", &editor.hide_after, Message::HideAfterEdited)
        ),
        field(
            "Lines per tool result",
            number("0 is no limit", &editor.max_lines, Message::MaxLinesEdited)
        ),
        field(
            "Bytes per tool result",
            number("0 is no limit", &editor.max_bytes, Message::MaxBytesEdited)
        ),
        field("Tool outputs", compress),
    ]
    .spacing(14);
    let mut rows = column![settings, outputs].spacing(12);
    if let Some(engineering) = engineering {
        rows = rows.push(engineering);
    }
    rows.into()
}

/// What the Moebius agent is asked to write the instructions of the agent `name`, with what the person wants.
pub(super) fn instructions_request(name: &str, wanted: &str) -> String {
    format!(
        "Write the instructions of the agent {name} and save them. Read the agent first with \
         `\"$MOEBIUS\" agent show {name}`, and the moebius skill's guide on agents. `\"$MOEBIUS\" agent save` \
         replaces the whole agent, so keep its model, protocol, harness, thinking, token budget, tool output \
         settings, skills, MCP servers and extensions as they are, and pass the new instructions with \
         `--instructions <file>`. Then say what you \
         wrote.\n\nWhat the person wants {name} to do:\n\n{wanted}"
    )
}

pub(super) fn instructions_box(editor: &Editor) -> Element<'_, AppMessage> {
    let message = AppMessage::Agents;
    // A new agent is saved before the Moebius agent can change it.
    let ways: Element<'_, AppMessage> = if editor.naming {
        space().into()
    } else {
        row![
            page::chip(
                "Write them".to_string(),
                !editor.by_agent,
                false,
                message(Message::InstructionsByAgent(false))
            ),
            page::chip(
                "Let the Moebius agent write them".to_string(),
                editor.by_agent,
                false,
                message(Message::InstructionsByAgent(true))
            ),
        ]
        .spacing(8)
        .into()
    };
    let heading = box_heading(
        "Instructions",
        aside(if editor.built_in.is_some() {
            "Markdown · added after the built-in instructions"
        } else {
            "Markdown · added to the harness's system prompt"
        }),
    );
    if editor.by_agent && !editor.naming {
        let ask = (!editor.asking && !editor.wish.text().trim().is_empty())
            .then_some(message(Message::AskAgent));
        let wish = text_editor(&editor.wish)
            .placeholder(
                "What this agent should do and how, in a few words. The Moebius agent writes the \
                 instructions and saves them; the chat opens.",
            )
            .on_action(|action| AppMessage::Agents(Message::WishEdited(action)))
            .size(theme::BODY)
            .padding(14)
            .height(120)
            .style(editor_style);
        return boxed(
            column![
                heading,
                ways,
                wish,
                row![page::primary_button("Ask the agent", ask)]
            ]
            .spacing(12),
        );
    }
    let input = text_editor(&editor.instructions)
        .placeholder("What the agent is told before it starts, in Markdown.")
        .on_action(|action| AppMessage::Agents(Message::Instructions(action)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding(14)
        .height(220)
        .style(editor_style);
    boxed(column![heading, ways, input].spacing(12))
}

/// What a built-in agent is always told, which the person cannot change.
pub(super) fn built_in_instructions_box(built_in: &BuiltInAgent) -> Element<'_, AppMessage> {
    let body = container(
        text(built_in.instructions.as_str())
            .font(theme::MONO)
            .size(theme::CODE_SMALL)
            .color(theme::TEXT_SECONDARY)
            .wrapping(Wrapping::WordOrGlyph),
    )
    .padding(14)
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(theme::LANE.into()),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: theme::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    });
    section(
        "Built-in instructions",
        "Read only · Moebius brings them",
        std::iter::once(body.into()),
        "",
        None,
    )
}

/// The skills a built-in agent always has, before any of its own.
pub(super) fn built_in_skills_box(built_in: &BuiltInAgent) -> Element<'_, AppMessage> {
    let items = built_in.skills.iter().map(|skill| {
        let name = text(skill.name.as_str())
            .font(theme::MONO)
            .size(theme::CODE)
            .color(theme::TEXT);
        item(name.into(), &skill.description, false)
    });
    let aside = match built_in.skills.len() {
        1 => "Read only · 1 skill".to_string(),
        n => format!("Read only · {n} skills"),
    };
    section("Built-in skills", &aside, items, "No built-in skill.", None)
}

pub(super) fn extensions_box(editor: &Editor) -> Element<'_, AppMessage> {
    paths_box(
        "Extensions",
        "Files in the repository",
        "No extension.",
        "extensions/log.ts",
        "Add extension",
        &editor.extensions,
        &editor.extension,
        (
            Message::ExtensionEdited,
            Message::AddExtension,
            Message::RemoveExtension,
        ),
    )
}

/// The skills of an agent of `moebius.toml`: paths to skill directories.
pub(super) fn skill_paths_box(editor: &Editor) -> Element<'_, AppMessage> {
    paths_box(
        "Skills",
        "Directories, from moebius.toml",
        "No skill.",
        "skills/house-style",
        "Add skill",
        &editor.skills,
        &editor.skill_path,
        (
            Message::SkillPathEdited,
            Message::AddSkillPath,
            Message::RemoveSkillPath,
        ),
    )
}

/// What a box of paths sends: the input edited, a path added, a path removed.
type PathMessages = (fn(String) -> Message, Message, fn(String) -> Message);

/// A box with a list of paths, each of which can be removed, and an input to add one.
#[allow(clippy::too_many_arguments)]
fn paths_box<'a>(
    heading: &'a str,
    aside: &'a str,
    empty: &'a str,
    placeholder: &'a str,
    add_label: &'a str,
    chosen: &'a [String],
    input: &'a str,
    (edited, add, remove): PathMessages,
) -> Element<'a, AppMessage> {
    let message = |message| AppMessage::Agents(message);
    let paths = chosen.iter().map(|path| {
        row![
            text(path.as_str())
                .size(theme::CODE_SMALL)
                .font(theme::MONO)
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph),
            button(text("Remove").size(theme::LABEL))
                .padding([3, 8])
                .on_press(message(remove(path.clone())))
                .style(|_, _| button::Style {
                    background: Some(theme::SURFACE.into()),
                    text_color: theme::TEXT_SECONDARY,
                    border: Border {
                        color: theme::LINE,
                        width: 1.0,
                        radius: theme::RADIUS_SMALL.into(),
                    },
                    ..button::Style::default()
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    });
    let can_add = !input.trim().is_empty();
    let adder = column![
        text_input(placeholder, input)
            .on_input(move |path| message(edited(path)))
            .on_submit_maybe(can_add.then(|| message(add.clone())))
            .font(theme::MONO)
            .size(theme::CODE)
            .padding([8, 10])
            .style(connect::input_style),
        page::white_button(add_label, can_add.then(|| message(add))),
    ]
    .spacing(8);
    section(heading, aside, paths, empty, Some(adder.into()))
}

impl Agents {
    pub(super) fn skills_box<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let offered: Vec<&Skill> = self
            .skills
            .iter()
            .flatten()
            .filter(|skill| !skill.built_in)
            .collect();
        let mut names: Vec<&str> = offered.iter().map(|skill| skill.name.as_str()).collect();
        add_chosen(&mut names, &editor.skills);
        let items = names.into_iter().map(|name| {
            let description = offered
                .iter()
                .find(|skill| skill.name == name)
                .map_or("", |skill| skill.description.as_str());
            let chosen = editor.skills.iter().any(|held| held == name);
            let chip = page::chip(
                name.to_string(),
                chosen,
                false,
                AppMessage::Agents(Message::ToggleSkill(name.to_string())),
            );
            item(chip, description, false)
        });
        let aside = format!("{} of {}", editor.skills.len(), offered.len());
        section(
            "Skills",
            &aside,
            items,
            "No skill in skills/ of the repository.",
            None,
        )
    }

    pub(super) fn mcp_box<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let mut names: Vec<&str> = self.mcp.iter().map(|server| server.name.as_str()).collect();
        add_chosen(&mut names, &editor.mcp);
        let items = names.into_iter().map(|name| {
            let server = self.mcp.iter().find(|server| server.name == name);
            let chosen = editor.mcp.iter().any(|held| held == name);
            let broken = server.is_none_or(|server| server.error.is_some());
            let chip = page::chip(
                name.to_string(),
                chosen,
                broken,
                AppMessage::Agents(Message::ToggleMcp(name.to_string())),
            );
            match server {
                Some(McpServer {
                    error: Some(error), ..
                }) => item(chip, error, true),
                Some(server) => item(chip, &server.kind, false),
                None => item(chip, "Not in mcp.toml", true),
            }
        });
        let aside = format!("{} of {}", editor.mcp.len(), self.mcp.len());
        section(
            "MCP servers",
            &aside,
            items,
            "No server in mcp.toml of the repository.",
            None,
        )
    }

    /// Model, protocol, thinking level and token budget.
    pub(super) fn settings_row<'a>(&'a self, editor: &'a Editor) -> Element<'a, AppMessage> {
        let models = self.available.as_ref().map_or(vec![], |a| a.choices());
        settings_row(editor, &models, &self.runners)
    }
}
