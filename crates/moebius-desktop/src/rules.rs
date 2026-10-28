//! The Rules page: the built-in patterns and what each refused this week, the server's rules in prose with an editor
//! when they live in a harness configuration repository, and the rules each repository adds. A save is `PUT
//! /v1/rules`, which the server makes a commit; the repositories' own rules change through a change to the repository.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space, text, text_editor};
use iced::{Alignment, Border, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Rule, RulesView, Run};
use crate::live::say;
use crate::page::{self, Tone};
use crate::settings::Settings;
use crate::theme;

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<RulesView, ClientError>),
    Edited(text_editor::Action),
    Save,
    /// The server answered the save: the rules as committed, or why not.
    Saved(Result<RulesView, ClientError>),
    /// "Write them" (false) or "Let the Moebius agent write them" (true) was chosen.
    ByAgent(bool),
    WishEdited(text_editor::Action),
    /// "Ask the agent" was pressed.
    AskAgent,
    /// The server started the Moebius agent's chat that changes the rules, or said why not.
    Asked(Result<Run, ClientError>),
}

/// The agent that changes the rules for the person.
const AGENT: &str = "Moebius";

/// What the Moebius agent is asked, with what the person said.
fn request(wanted: &str) -> String {
    format!(
        "Change the server's rules in prose. Read the current rules with `\"$MOEBIUS\" rules` and the rules guide of \
         the moebius skill first. Write the change as rules in prose, each a `## name` heading and a few plain \
         sentences, keep every existing rule as it is unless the person asks to change or remove it, and never \
         weaken a rule on your own. Write the whole rules file to a file and save it with \
         `\"$MOEBIUS\" rules save <file>`. Then say what changed.\n\nWhat should be allowed or forbidden:\n\n{wanted}"
    )
}

enum Notice {
    /// The save is a commit with this short id; none when the text changed nothing.
    Saved(Option<String>),
    Failed(String),
}

#[derive(Default)]
pub struct Rules {
    view: Option<RulesView>,
    editor: text_editor::Content,
    /// The text the editor was last filled with, to tell whether a person has edited it since.
    loaded: String,
    notice: Option<Notice>,
    /// Whether the Moebius agent changes the rules from what is in `wish`.
    by_agent: bool,
    wish: text_editor::Content,
    /// Whether the request to the agent is on its way.
    asking: bool,
}

/// The text without the final line breaks, which the editor adds and drops on its own.
fn trimmed(text: &str) -> &str {
    text.trim_end_matches('\n')
}

impl Rules {
    /// Reads the rules again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let client = Client::new(settings.clone());
        Task::perform(async move { client.rules().await }, |result| {
            AppMessage::Rules(Message::Loaded(result))
        })
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Loaded(Err(ClientError::Unauthorized))
                | Message::Saved(Err(ClientError::Unauthorized))
                | Message::Asked(Err(ClientError::Unauthorized))
        )
    }

    /// The text a save would send.
    pub fn save_text(&self) -> String {
        trimmed(&self.editor.text()).to_string()
    }

    /// Whether the text in the editor differs from the rules as the server has them.
    fn changed(&self) -> bool {
        trimmed(&self.editor.text()) != trimmed(&self.loaded)
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Loaded(Ok(view)) => {
                // What a person is typing stays, unless the server's text is what they started from.
                let untouched = trimmed(&self.editor.text()) == trimmed(&self.loaded);
                if untouched {
                    self.fill(&view);
                }
                self.view = Some(view);
            }
            Message::Loaded(Err(_)) => {}
            Message::Edited(action) => {
                if action.is_edit() {
                    self.notice = None;
                }
                self.editor.perform(action);
            }
            Message::Save => {
                let Some(settings) = settings else {
                    return Task::none();
                };
                self.notice = None;
                let (client, text) = (Client::new(settings.clone()), self.save_text());
                return Task::perform(async move { client.save_rules(&text).await }, |result| {
                    AppMessage::Rules(Message::Saved(result))
                });
            }
            Message::Saved(Ok(view)) => {
                self.notice = Some(Notice::Saved(view.commit.clone()));
                self.loaded.clone_from(&view.prose.text);
                self.view = Some(view);
            }
            Message::Saved(Err(e)) => self.notice = Some(Notice::Failed(say(&e))),
            Message::ByAgent(by_agent) => self.by_agent = by_agent,
            Message::WishEdited(action) => self.wish.perform(action),
            Message::AskAgent => {
                let wanted = self.wish.text().trim().to_string();
                let (Some(settings), false, false) = (settings, self.asking, wanted.is_empty())
                else {
                    return Task::none();
                };
                self.notice = None;
                self.asking = true;
                let (client, message) = (Client::new(settings.clone()), request(&wanted));
                return Task::perform(
                    async move { client.start_chat(AGENT, &message, None, false).await },
                    |result| AppMessage::Rules(Message::Asked(result)),
                );
            }
            Message::Asked(Ok(_)) => {
                self.asking = false;
                self.by_agent = false;
                self.wish = text_editor::Content::new();
            }
            Message::Asked(Err(e)) => {
                self.asking = false;
                self.notice = Some(Notice::Failed(say(&e)));
            }
        }
        Task::none()
    }

    fn fill(&mut self, view: &RulesView) {
        self.editor = text_editor::Content::with_text(&view.prose.text);
        self.loaded.clone_from(&view.prose.text);
    }

    /// The Rules page.
    pub fn view(&self) -> Element<'_, AppMessage> {
        let editable = self.view.as_ref().is_some_and(|view| view.prose.editable);
        let save = editable.then(|| {
            let ready = self.changed() && !self.by_agent;
            page::primary_button("Save", ready.then_some(AppMessage::Rules(Message::Save)))
        });
        let header = page::header(
            "Build",
            "Rules",
            None,
            page::context(
                "What a run must not do. Every tool call is checked against these before it runs; a refusal names the rule.",
            ),
            save.unwrap_or_else(|| space().into()),
        );
        let mut boxes = column![header].spacing(theme::PAGE_GAP).width(Length::Fill);
        match &self.view {
            None => boxes = boxes.push(page::boxed("Rules", page::body("Reading the rules…"))),
            Some(view) => {
                boxes = boxes
                    .push(self.patterns_box(view))
                    .push(self.prose_box(view));
                for repository in &view.repositories {
                    boxes = boxes.push(repository_box(repository, view));
                }
            }
        }
        iced::widget::scrollable(boxes.padding(iced::Padding::default().right(12)))
            .height(Length::Fill)
            .into()
    }

    fn patterns_box<'a>(&'a self, view: &'a RulesView) -> Element<'a, AppMessage> {
        let rows = view.patterns.iter().map(|pattern| {
            row![
                column![
                    text(pattern.name.as_str())
                        .size(theme::BODY)
                        .font(theme::MONO),
                    text(pattern.description.as_str())
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY)
                        .wrapping(Wrapping::Word),
                ]
                .spacing(3)
                .width(Length::Fill),
                refusals_tag(view, &pattern.name),
            ]
            .spacing(14)
            .align_y(Alignment::Center)
            .into()
        });
        section(
            "Built-in patterns",
            "Always on · checked first, no model asked",
            column(rows).spacing(12).into(),
        )
    }

    fn prose_box<'a>(&'a self, view: &'a RulesView) -> Element<'a, AppMessage> {
        let prose = &view.prose;
        let aside = match prose.source.as_deref() {
            Some("harness") => "rules.md of the harness configuration repository",
            Some(_) => "the rules_file of moebius.toml",
            None => "No rules file",
        };
        let mut content = column![].spacing(12);
        if prose.editable {
            content = content.push(self.editor_view());
        } else if !prose.text.is_empty() {
            content = content.push(
                container(
                    text(prose.text.as_str())
                        .size(theme::CODE)
                        .font(theme::MONO)
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
                }),
            );
        }
        if let Some(error) = &prose.error {
            content = content.push(notice_line(error, true));
        }
        content = content.push(page::rule()).push(rule_list(
            &prose.rules,
            view,
            "No rule in prose. Only the built-in patterns apply.",
        ));
        match &self.notice {
            Some(Notice::Saved(Some(commit))) => {
                content = content.push(notice_line(&format!("Saved as a commit: {commit}"), false));
            }
            Some(Notice::Saved(None)) => {
                content = content.push(notice_line(
                    "Nothing changed, so nothing was committed.",
                    false,
                ));
            }
            Some(Notice::Failed(error)) => content = content.push(notice_line(error, true)),
            None => {}
        }
        if !prose.editable {
            content = content.push(page::body(
                "These rules are read only here: this server has no rules file.",
            ));
        }
        section("Rules in prose", aside, content.into())
    }

    /// The chips that choose who writes the rules, and the editor or the request to the agent.
    fn editor_view(&self) -> Element<'_, AppMessage> {
        let message = AppMessage::Rules;
        let ways = row![
            page::chip(
                "Write them".to_string(),
                !self.by_agent,
                false,
                message(Message::ByAgent(false))
            ),
            page::chip(
                "Let the Moebius agent write them".to_string(),
                self.by_agent,
                false,
                message(Message::ByAgent(true))
            ),
        ]
        .spacing(8);
        let style = |_: &iced::Theme, status: text_editor::Status| text_editor::Style {
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
        };
        if self.by_agent {
            let ask = (!self.asking && !self.wish.text().trim().is_empty())
                .then_some(message(Message::AskAgent));
            let wish = text_editor(&self.wish)
                .placeholder(
                    "What should be allowed or forbidden, in a few words. The Moebius agent changes the \
                     rules and saves them; the chat opens.",
                )
                .on_action(move |action| message(Message::WishEdited(action)))
                .size(theme::BODY)
                .padding(14)
                .height(120)
                .style(style);
            return column![ways, wish, row![page::primary_button("Ask the agent", ask)]]
                .spacing(12)
                .into();
        }
        let input = text_editor(&self.editor)
            .placeholder("## name\nWhat a run must not do, in a few plain sentences.")
            .on_action(move |action| message(Message::Edited(action)))
            .font(theme::MONO)
            .size(theme::CODE)
            .padding(14)
            .height(220)
            .style(style);
        column![ways, input].spacing(12).into()
    }
}

fn repository_box<'a>(
    repository: &'a crate::client::RepositoryRules,
    view: &'a RulesView,
) -> Element<'a, AppMessage> {
    let mut content = column![].spacing(12);
    if let Some(error) = &repository.error {
        content = content.push(notice_line(error, true));
    }
    content = content
        .push(rule_list(
            &repository.rules,
            view,
            "No rule in this file.",
        ))
        .push(page::body(
            "Read only: these rules change through a change to the repository (.moebius/rules.md on its default branch).",
        ));
    section(
        &repository.repository,
        "Added to the server's rules; cannot weaken them",
        content.into(),
    )
}

/// The rules of a file, each with its text and what it refused this week.
fn rule_list<'a>(
    rules: &'a [Rule],
    view: &'a RulesView,
    empty: &'a str,
) -> Element<'a, AppMessage> {
    if rules.is_empty() {
        return page::body(empty);
    }
    column(rules.iter().map(|rule| {
        row![
            column![
                text(rule.name.as_str()).size(theme::BODY).font(theme::MONO),
                text(rule.text.as_str())
                    .size(theme::CONTEXT)
                    .color(theme::TEXT_SECONDARY)
                    .wrapping(Wrapping::Word),
            ]
            .spacing(3)
            .width(Length::Fill),
            refusals_tag(view, &rule.name),
        ]
        .spacing(14)
        .align_y(Alignment::Center)
        .into()
    }))
    .spacing(12)
    .into()
}

/// How often the rule refused a call in the last 7 days; red when it did.
fn refusals_tag<'a>(view: &RulesView, name: &str) -> Element<'a, AppMessage> {
    match view.refusals.get(name).copied().unwrap_or(0) {
        0 => page::tag("No refusals this week".to_string(), Tone::Plain),
        1 => page::tag("1 refusal this week".to_string(), Tone::Red),
        n => page::tag(format!("{n} refusals this week"), Tone::Red),
    }
}

/// A box with a heading, a muted word on its right and its content.
fn section<'a>(
    heading: &'a str,
    aside: &'a str,
    content: Element<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    container(
        column![
            row![
                text(heading)
                    .size(theme::BOX_HEADING)
                    .font(theme::manrope(Weight::Bold)),
                space::horizontal(),
                text(aside).size(theme::LABEL).color(theme::TEXT_MUTED),
            ]
            .align_y(Alignment::Center),
            content,
        ]
        .spacing(12),
    )
    .padding(20)
    .width(Length::Fill)
    .style(|_| page::box_style())
    .into()
}

/// One line about what happened to the save or what is wrong with a file, red when it failed.
fn notice_line<'a>(line: &str, red: bool) -> Element<'a, AppMessage> {
    text(line.to_string())
        .size(theme::CONTEXT)
        .font(theme::manrope(Weight::Semibold))
        .color(if red {
            theme::RED_TEXT
        } else {
            theme::ONLINE_TEXT
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_is_told_to_keep_the_existing_rules_and_save_the_file() {
        let asked = request("never touch the lockfile");
        assert!(asked.contains(r#"`"$MOEBIUS" rules`"#));
        assert!(asked.contains(r#"`"$MOEBIUS" rules save <file>`"#));
        assert!(asked.contains("never weaken"));
        assert!(
            asked.ends_with("What should be allowed or forbidden:\n\nnever touch the lockfile")
        );
    }
}
