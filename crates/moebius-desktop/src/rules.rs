//! The Rules page: the built-in patterns and what each refused this week, the server's rules in prose and the rules
//! each repository adds. Every rule in prose is a card with its name, its text and a pencil; the pencil grows the card
//! in place into an editor of the rule's name and text with Save, Cancel and Delete (which asks first). Under the last
//! rule an empty "New rule" card with a pencil adds one. A save puts the rule's `## name` section back into its file
//! (or appends it, or drops it) and saves the whole file: the server's with `PUT /v1/rules`, which the server checks
//! and commits, a repository's `.moebius/rules.md` with `PUT /v1/repositories/{id}/files/.moebius/rules.md`, a commit
//! on its default branch that the server checks first. "Let the Moebius agent write them" asks the Moebius agent
//! instead, in the floating chat.

use iced::font::Weight;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, space, text, text_editor, text_input};
use iced::{Alignment, Border, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, RepositoryRules, Rule, RulesView, Run};
use crate::inline_edit::{self, editor_style};
use crate::live::say;
use crate::page::{self, Tone};
use crate::settings::Settings;
use crate::{connect, theme};

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<RulesView, ClientError>),
    /// "Let the Moebius agent write them" (true) or its Cancel (false) was pressed.
    ByAgent(bool),
    WishEdited(text_editor::Action),
    /// "Ask the agent" was pressed.
    AskAgent,
    /// The server started the Moebius agent's chat that changes the rules, or said why not.
    Asked(Result<Run, ClientError>),
    /// A pencil: the rule with this name of the server or of a repository is edited in place; without a name, the
    /// "New rule" card is.
    EditRule(Place, Option<String>),
    NameEdited(String),
    TextEdited(text_editor::Action),
    SaveRule,
    CancelRule,
    /// "Delete rule" was pressed: the card asks first.
    AskDelete,
    /// The deletion was called off.
    KeepRule,
    /// The deletion was confirmed: the rule's section is dropped from the file, which is saved.
    DeleteRule,
    /// The server answered the save of a rules file: the short id of the commit of the server's rules (none when the
    /// text changed nothing, and for a repository's), or why not.
    RuleSaved(Result<Option<String>, ClientError>),
}

/// Whose rules file a rule belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Server,
    /// The repository with this id.
    Repository(i64),
}

/// A rule as it is being edited in its card.
struct Editing {
    place: Place,
    /// The rule's name as the file has it; none for a new rule.
    rule: Option<String>,
    name: String,
    text: text_editor::Content,
    /// Whether Delete was pressed and waits for a confirmation.
    deleting: bool,
    saving: bool,
    error: Option<String>,
}

enum Notice {
    /// The save is a commit with this short id; none when the server does not say, or the text changed nothing.
    Saved(Option<String>),
    Failed(String),
}

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

#[derive(Default)]
pub struct Rules {
    view: Option<RulesView>,
    /// What came of the last save, and whose file it was.
    notice: Option<(Place, Notice)>,
    /// Whether the request to the Moebius agent is open.
    by_agent: bool,
    wish: text_editor::Content,
    /// Whether the request to the agent is on its way.
    asking: bool,
    editing: Option<Editing>,
}

/// The heading of a rule's section, if `line` is one.
fn heading(line: &str) -> Option<&str> {
    line.strip_prefix("## ").map(str::trim)
}

/// `file` with the `## name` section replaced by `section`, or dropped when `section` is blank; the parts of the file
/// are kept apart by one blank line. None when the file has no rule of that name.
pub fn replace_rule(file: &str, name: &str, section: &str) -> Option<String> {
    let lines: Vec<&str> = file.lines().collect();
    let start = lines.iter().position(|line| heading(line) == Some(name))?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| heading(line).is_some())
        .map_or(lines.len(), |offset| start + 1 + offset);
    let before = lines[..start].join("\n");
    let after = lines[end..].join("\n");
    Some(joined(&[&before, section, &after]))
}

/// `file` with `section` added at its end.
pub fn append_rule(file: &str, section: &str) -> String {
    joined(&[file, section])
}

/// The parts that are not blank, one blank line apart, ending with a line break; empty when all are blank.
fn joined(parts: &[&str]) -> String {
    let parts: Vec<&str> = parts
        .iter()
        .map(|part| part.trim_end())
        .filter(|part| !part.trim().is_empty())
        .collect();
    match parts.as_slice() {
        [] => String::new(),
        parts => format!("{}\n", parts.join("\n\n")),
    }
}

/// A rule as its section of a rules file.
fn section(name: &str, text: &str) -> String {
    format!("## {}\n{}", name.trim(), text.trim_end())
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
                | Message::Asked(Err(ClientError::Unauthorized))
                | Message::RuleSaved(Err(ClientError::Unauthorized))
        )
    }

    /// What the floating chat tells the Moebius agent of the page: the rule being edited, or the server's rules.
    pub fn context_text(&self) -> String {
        match &self.editing {
            Some(editing) => format!(
                "Editing {} in place:\n{}",
                match &editing.rule {
                    Some(rule) => format!("the rule {rule}"),
                    None => "a new rule".to_string(),
                },
                section(&editing.name, &editing.text.text())
            ),
            None => self
                .view
                .as_ref()
                .map_or(String::new(), |view| view.prose.text.clone()),
        }
    }

    /// Whether a rule is open in its editor or a request to the agent is typed, which Escape must not drop.
    pub fn unsaved(&self) -> bool {
        self.editing.is_some() || !self.wish.text().trim().is_empty()
    }

    /// The text of the rules file of `place`, as the server has it.
    fn file_of(&self, place: Place) -> Option<&str> {
        let view = self.view.as_ref()?;
        match place {
            Place::Server => Some(&view.prose.text),
            Place::Repository(id) => view
                .repositories
                .iter()
                .find(|repository| repository.repository_id == id)
                .map(|repository| repository.text.as_str()),
        }
    }

    /// The rules of `place`, as the server read them.
    fn rules_of(&self, place: Place) -> &[Rule] {
        let Some(view) = &self.view else {
            return &[];
        };
        match place {
            Place::Server => &view.prose.rules,
            Place::Repository(id) => view
                .repositories
                .iter()
                .find(|repository| repository.repository_id == id)
                .map_or(&[], |repository| &repository.rules),
        }
    }

    /// The whole file a save of the rule being edited sends: the rule put back where it was, appended when it is new,
    /// or dropped when `delete` says so.
    pub fn file_to_save(&self, delete: bool) -> Option<String> {
        let editing = self.editing.as_ref()?;
        let file = self.file_of(editing.place)?;
        let edited = if delete {
            String::new()
        } else {
            section(&editing.name, &editing.text.text())
        };
        match &editing.rule {
            Some(rule) => replace_rule(file, rule, &edited),
            None => Some(append_rule(file, &edited)),
        }
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        match message {
            Message::Loaded(Ok(view)) => self.view = Some(view),
            Message::Loaded(Err(_)) => {}
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
                    async move { client.ask(&message, None, "").await },
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
                self.notice = Some((Place::Server, Notice::Failed(say(&e))));
            }
            Message::EditRule(place, rule) => {
                let (name, text) = match &rule {
                    Some(name) => match self.rules_of(place).iter().find(|r| &r.name == name) {
                        Some(found) => (found.name.clone(), found.text.clone()),
                        None => return Task::none(),
                    },
                    None => (String::new(), String::new()),
                };
                self.notice = None;
                self.editing = Some(Editing {
                    place,
                    rule,
                    name,
                    text: text_editor::Content::with_text(&text),
                    deleting: false,
                    saving: false,
                    error: None,
                });
            }
            Message::NameEdited(name) => {
                if let Some(editing) = &mut self.editing {
                    editing.name = name;
                }
            }
            Message::TextEdited(action) => {
                if let Some(editing) = &mut self.editing {
                    editing.text.perform(action);
                }
            }
            Message::CancelRule => self.editing = None,
            Message::AskDelete => {
                if let Some(editing) = &mut self.editing {
                    editing.deleting = true;
                }
            }
            Message::KeepRule => {
                if let Some(editing) = &mut self.editing {
                    editing.deleting = false;
                }
            }
            Message::SaveRule => return self.save(false, settings),
            Message::DeleteRule => return self.save(true, settings),
            Message::RuleSaved(Ok(commit)) => {
                if let Some(editing) = self.editing.take() {
                    self.notice = Some((editing.place, Notice::Saved(commit)));
                }
                if let Some(settings) = settings {
                    return self.reload(settings);
                }
            }
            Message::RuleSaved(Err(e)) => {
                if let Some(editing) = &mut self.editing {
                    editing.saving = false;
                    editing.deleting = false;
                    editing.error = Some(say(&e));
                }
            }
        }
        Task::none()
    }

    /// Saves the file of the rule being edited, with the rule changed, added or, with `delete`, dropped.
    fn save(&mut self, delete: bool, settings: Option<&Settings>) -> Task<AppMessage> {
        let (Some(settings), Some(file)) = (settings, self.file_to_save(delete)) else {
            return Task::none();
        };
        let Some(editing) = self.editing.as_mut().filter(|e| !e.saving) else {
            return Task::none();
        };
        if !delete && !valid_name(&editing.name) {
            return Task::none();
        }
        editing.saving = true;
        editing.error = None;
        let (client, place) = (Client::new(settings.clone()), editing.place);
        Task::perform(
            async move {
                match place {
                    Place::Server => client.save_rules(&file).await.map(|view| view.commit),
                    Place::Repository(id) => client
                        .save_file(id, ".moebius/rules.md", &file)
                        .await
                        .map(|_| None),
                }
            },
            |result| AppMessage::Rules(Message::RuleSaved(result)),
        )
    }

    /// The Rules page.
    pub fn view(&self) -> Element<'_, AppMessage> {
        let header = page::header(
            "Build",
            "Rules",
            None,
            page::context(
                "What a run must not do. Every tool call is checked against these before it runs; a refusal names the rule.",
            ),
            space().into(),
        );
        let mut boxes = column![header].spacing(theme::PAGE_GAP).width(Length::Fill);
        match &self.view {
            None => boxes = boxes.push(page::boxed("Rules", page::body("Reading the rules…"))),
            Some(view) => {
                boxes = boxes
                    .push(self.patterns_box(view))
                    .push(self.prose_box(view));
                for repository in &view.repositories {
                    boxes = boxes.push(self.repository_box(repository, view));
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
        boxed(
            "Built-in patterns",
            aside("Always on · checked first, no model asked"),
            column(rows).spacing(12).into(),
        )
    }

    fn prose_box<'a>(&'a self, view: &'a RulesView) -> Element<'a, AppMessage> {
        let prose = &view.prose;
        let where_from = match prose.source.as_deref() {
            Some("harness") => "rules.md of the harness configuration repository",
            Some(_) => "the rules_file of moebius.toml",
            None => "No rules file",
        };
        let mut heading_side = row![aside(where_from)]
            .spacing(12)
            .align_y(Alignment::Center);
        if prose.editable && !self.by_agent {
            heading_side = heading_side.push(page::white_button(
                "Let the Moebius agent write them",
                Some(AppMessage::Rules(Message::ByAgent(true))),
            ));
        }
        let mut content = column![].spacing(10);
        if prose.editable && self.by_agent {
            content = content.push(self.wish_view());
        }
        if let Some(error) = &prose.error {
            content = content.push(notice_line(error, true));
        }
        let place = prose.editable.then_some(Place::Server);
        content = content.push(self.rule_cards(
            place,
            &prose.rules,
            view,
            "No rule in prose. Only the built-in patterns apply.",
        ));
        content = self.with_notice(content, Place::Server);
        if !prose.editable {
            content = content.push(page::body(
                "These rules are read only here: this server has no rules file.",
            ));
        }
        boxed("Rules in prose", heading_side.into(), content.into())
    }

    fn repository_box<'a>(
        &'a self,
        repository: &'a RepositoryRules,
        view: &'a RulesView,
    ) -> Element<'a, AppMessage> {
        let place = Place::Repository(repository.repository_id);
        let mut content = column![].spacing(10);
        if let Some(error) = &repository.error {
            content = content.push(notice_line(error, true));
        }
        content = content.push(self.rule_cards(
            Some(place),
            &repository.rules,
            view,
            "No rule of its own.",
        ));
        content = self.with_notice(content, place);
        content = content.push(page::body(
            "Saved as a commit on the default branch (.moebius/rules.md), checked first.",
        ));
        boxed(
            &repository.repository,
            aside("Added to the server's rules; cannot weaken them"),
            content.into(),
        )
    }

    /// `content` with what came of the last save of the file of `place`.
    fn with_notice<'a>(
        &'a self,
        content: iced::widget::Column<'a, AppMessage>,
        place: Place,
    ) -> iced::widget::Column<'a, AppMessage> {
        match &self.notice {
            Some((at, Notice::Saved(Some(commit)))) if *at == place => {
                content.push(notice_line(&format!("Saved as a commit: {commit}"), false))
            }
            Some((at, Notice::Saved(None))) if *at == place => {
                content.push(notice_line("Saved.", false))
            }
            Some((at, Notice::Failed(error))) if *at == place => {
                content.push(notice_line(error, true))
            }
            _ => content,
        }
    }

    /// The request to the Moebius agent: what should be allowed or forbidden, "Ask the agent" and Cancel.
    fn wish_view(&self) -> Element<'_, AppMessage> {
        let message = AppMessage::Rules;
        let ask = (!self.asking && !self.wish.text().trim().is_empty())
            .then_some(message(Message::AskAgent));
        let wish = text_editor(&self.wish)
            .placeholder(
                "What should be allowed or forbidden, in a few words. The Moebius agent changes the rules and saves \
                 them; the chat opens.",
            )
            .on_action(move |action| message(Message::WishEdited(action)))
            .size(theme::BODY)
            .padding(14)
            .height(120)
            .style(editor_style);
        column![
            wish,
            row![
                page::primary_button("Ask the agent", ask),
                page::white_button("Cancel", Some(message(Message::ByAgent(false)))),
            ]
            .spacing(8),
        ]
        .spacing(10)
        .into()
    }

    /// The rules of a file as cards, each with a pencil when the file of `place` can be edited, and then the "New
    /// rule" card; the card being edited is its editor.
    fn rule_cards<'a>(
        &'a self,
        place: Option<Place>,
        rules: &'a [Rule],
        view: &'a RulesView,
        empty: &'a str,
    ) -> Element<'a, AppMessage> {
        let mut cards = column![].spacing(10);
        if rules.is_empty() && place.is_none() {
            return page::body(empty);
        }
        for rule in rules {
            if let Some(editor) = place.and_then(|place| self.editor_card(place, Some(&rule.name)))
            {
                cards = cards.push(editor);
                continue;
            }
            let mut name = row![text(rule.name.as_str()).size(theme::BODY).font(theme::MONO)]
                .spacing(6)
                .align_y(Alignment::Center);
            if let Some(place) = place {
                name = name.push(inline_edit::pencil(
                    pencil_id(place, &rule.name),
                    AppMessage::Rules(Message::EditRule(place, Some(rule.name.clone()))),
                ));
            }
            let content = row![
                column![
                    name,
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
            .align_y(Alignment::Center);
            cards = cards.push(card(content.into(), CardLine::Plain));
        }
        let Some(place) = place else {
            return cards.into();
        };
        if let Some(editor) = self.editor_card(place, None) {
            return cards.push(editor).into();
        }
        let id = match place {
            Place::Server => "new-rule".to_string(),
            Place::Repository(repository) => format!("new-rule-{repository}"),
        };
        let new = row![
            text(if rules.is_empty() {
                format!("{empty} Add a rule")
            } else {
                "Add a rule".to_string()
            })
            .size(theme::BODY)
            .color(theme::TEXT_MUTED),
            inline_edit::pencil(id, AppMessage::Rules(Message::EditRule(place, None))),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        cards.push(card(new.into(), CardLine::Muted)).into()
    }

    /// The card of the rule `rule` (a new rule without one) of `place` grown into its editor, if that is the one
    /// being edited.
    fn editor_card(&self, place: Place, rule: Option<&str>) -> Option<Element<'_, AppMessage>> {
        let editing = self
            .editing
            .as_ref()
            .filter(|e| e.place == place && e.rule.as_deref() == rule)?;
        let message = AppMessage::Rules;
        let fields = column![
            text_input("name-without-spaces", &editing.name)
                .on_input(move |name| message(Message::NameEdited(name)))
                .font(theme::MONO)
                .size(theme::BODY)
                .padding([9, 12])
                .style(connect::input_style),
            text_editor(&editing.text)
                .placeholder("What a run must not do, in a few plain sentences.")
                .on_action(move |action| message(Message::TextEdited(action)))
                .size(theme::BODY)
                .padding(14)
                .height(110)
                .style(editor_style),
        ]
        .spacing(10);
        let save =
            (!editing.saving && valid_name(&editing.name)).then_some(message(Message::SaveRule));
        let delete: Option<Element<'_, AppMessage>> = editing.rule.as_ref().map(|_| {
            if editing.deleting {
                row![
                    text("Delete this rule?")
                        .size(theme::CONTEXT)
                        .color(theme::TEXT_SECONDARY),
                    page::white_button("Keep it", Some(message(Message::KeepRule))),
                    page::white_button(
                        "Delete",
                        (!editing.saving).then_some(message(Message::DeleteRule))
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
            } else {
                page::white_button("Delete rule", Some(message(Message::AskDelete)))
            }
        });
        let editor = inline_edit::editor_with(
            fields.into(),
            save,
            message(Message::CancelRule),
            delete,
            editing.error.as_deref(),
        );
        Some(card(editor, CardLine::Editing))
    }
}

/// Whether `name` can be a rule's name: not blank, without spaces or line breaks.
fn valid_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && !name.contains(char::is_whitespace)
}

/// The id of the pencil of a rule, for a test to find it.
fn pencil_id(place: Place, rule: &str) -> String {
    match place {
        Place::Server => format!("edit-rule-{rule}"),
        Place::Repository(repository) => format!("edit-rule-{repository}-{rule}"),
    }
}

/// The line around a rule's card: a rule, the empty "New rule" card, or a card grown into its editor.
enum CardLine {
    Plain,
    Muted,
    Editing,
}

fn card(content: Element<'_, AppMessage>, line: CardLine) -> Element<'_, AppMessage> {
    let (color, width, fill, padding) = match line {
        CardLine::Plain => (theme::LINE, 1.0, theme::SURFACE, 12),
        CardLine::Muted => (theme::LINE, 1.0, theme::LANE, 12),
        CardLine::Editing => (theme::TEAL, 2.0, theme::SURFACE, 16),
    };
    container(content)
        .padding([padding, 14])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(fill.into()),
            border: Border {
                color,
                width,
                radius: theme::RADIUS_CARD.into(),
            },
            ..container::Style::default()
        })
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

/// A muted word on the right of a box's heading.
fn aside(line: &str) -> Element<'_, AppMessage> {
    text(line)
        .size(theme::LABEL)
        .color(theme::TEXT_MUTED)
        .into()
}

/// A box with a heading, what stands on its right and its content.
fn boxed<'a>(
    heading: &'a str,
    aside: Element<'a, AppMessage>,
    content: Element<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    container(
        column![
            row![
                text(heading)
                    .size(theme::BOX_HEADING)
                    .font(theme::manrope(Weight::Bold)),
                space::horizontal(),
                aside,
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
    fn a_rule_is_put_back_where_it_was_appended_or_dropped() {
        let file = "Intro.\n\n## a\none\n\n## b\ntwo\n";
        assert_eq!(
            replace_rule(file, "a", "## a\nfirst\nsecond\n\n\n").unwrap(),
            "Intro.\n\n## a\nfirst\nsecond\n\n## b\ntwo\n"
        );
        assert_eq!(
            replace_rule(file, "b", "## b\nlast").unwrap(),
            "Intro.\n\n## a\none\n\n## b\nlast\n"
        );
        assert_eq!(
            replace_rule("## a\none", "a", "## renamed\nnew").unwrap(),
            "## renamed\nnew\n"
        );
        assert_eq!(
            replace_rule(file, "a", "").unwrap(),
            "Intro.\n\n## b\ntwo\n"
        );
        assert_eq!(replace_rule("## a\none\n", "a", ""), Some(String::new()));
        assert_eq!(replace_rule(file, "c", "## c\nx"), None);
        assert_eq!(
            append_rule(file, "## c\nthree"),
            "Intro.\n\n## a\none\n\n## b\ntwo\n\n## c\nthree\n"
        );
        assert_eq!(append_rule("", "## c\nthree"), "## c\nthree\n");
    }

    #[test]
    fn a_rules_name_has_no_spaces() {
        assert!(valid_name(" no-vendoring "));
        assert!(!valid_name("two words"));
        assert!(!valid_name("  "));
    }

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
