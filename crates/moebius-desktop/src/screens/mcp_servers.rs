//! The MCP servers page: the servers of `mcp.toml` that agents can name, and an editor for the one chosen. An edit is
//! `PUT /v1/mcp-servers/{name}`, which the server makes a commit in the harness configuration repository. A value may
//! be `${NAME}`, which the server takes from its environment when a run starts; the page never shows what it stands
//! for. A server without a harness configuration repository lists no servers, and the page then offers to set one up
//! (see `harness_setup`).

use iced::widget::text::Wrapping;
use iced::widget::{button, column, pick_list, row, space, text, text_editor, text_input};
use iced::{Alignment, Border, Element, Length, Task};

use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, McpDefinition, McpServer, McpServerDetail, Skill};
use crate::screens::harness_setup::{self, HarnessSetup};
use crate::screens::live::say;
use crate::settings::Settings;
use crate::widgets::form::{
    box_heading, committed_line, field, menu_style, mono_editor, notice_line, pick_style,
};
use crate::widgets::import::{self, Import};
use crate::widgets::list_editor::{self, Actions, boxed, list_card};
use crate::widgets::page;
use crate::{screens::connect, theme};

const COMMAND: &str = "Command";
const URL: &str = "URL";

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Vec<McpServer>, ClientError>),
    /// The skills were listed, which says whether the server has a harness configuration repository.
    Harness(Result<Vec<Skill>, ClientError>),
    /// A server of the list was chosen.
    Select(String),
    /// The server with this name was read.
    Opened(String, Result<McpServerDetail, ClientError>),
    NewServer,
    NameEdited(String),
    KindChosen(String),
    CommandEdited(String),
    Args(text_editor::Action),
    UrlEdited(String),
    RowNameEdited(usize, String),
    RowValueEdited(usize, String),
    AddRow,
    RemoveRow(usize),
    Save,
    /// The server answered the edit: the server as committed, or why not.
    Saved(Result<McpServerDetail, ClientError>),
    AskDelete,
    Keep,
    Delete,
    Deleted(Result<(), ClientError>),
    /// A message of the import panel.
    Import(import::Message),
}

enum Notice {
    /// The edit is a commit with this short id; none when the edit changed nothing.
    Saved(Option<String>),
    Failed(String),
}

struct Editor {
    name: String,
    /// Whether the name is still being typed, as for a new server.
    naming: bool,
    /// Whether the server is in `mcp.toml`, so that it can be deleted.
    exists: bool,
    reading: bool,
    /// Whether the server is a remote one, with a URL, and not a command.
    remote: bool,
    command: String,
    /// The arguments, one per line.
    args: text_editor::Content,
    url: String,
    /// The environment of a command, or the headers of a URL: name and value.
    rows: Vec<(String, String)>,
    notice: Option<Notice>,
    /// Whether "Delete" asks to be confirmed.
    confirming: bool,
}

impl Editor {
    fn blank(name: &str) -> Self {
        Editor {
            name: name.to_string(),
            naming: false,
            exists: false,
            reading: false,
            remote: false,
            command: String::new(),
            args: text_editor::Content::new(),
            url: String::new(),
            rows: vec![],
            notice: None,
            confirming: false,
        }
    }

    fn fill(&mut self, definition: McpDefinition) {
        self.exists = true;
        self.remote = definition.url.is_some();
        self.command = definition.command.unwrap_or_default();
        self.args = text_editor::Content::with_text(&definition.args.join("\n"));
        self.url = definition.url.unwrap_or_default();
        let values = if self.remote {
            definition.headers
        } else {
            definition.env
        };
        self.rows = values.into_iter().collect();
    }

    /// The server's name and what a `PUT` sends for it, or what is wrong with the fields.
    fn request(&self) -> Result<(String, McpDefinition), String> {
        let name = self.name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("A name is made of letters, digits, - and _.".into());
        }
        let mut values = std::collections::BTreeMap::new();
        for (key, value) in &self.rows {
            match (key.trim(), value) {
                ("", value) if value.trim().is_empty() => {}
                ("", _) => return Err("Give each value a name.".into()),
                (key, value) => {
                    values.insert(key.to_string(), value.clone());
                }
            }
        }
        let definition = if self.remote {
            if self.url.trim().is_empty() {
                return Err("Enter the URL.".into());
            }
            McpDefinition {
                url: Some(self.url.trim().to_string()),
                headers: values,
                ..McpDefinition::default()
            }
        } else {
            if self.command.trim().is_empty() {
                return Err("Enter the command.".into());
            }
            McpDefinition {
                command: Some(self.command.trim().to_string()),
                args: self
                    .args
                    .text()
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(str::to_string)
                    .collect(),
                env: values,
                ..McpDefinition::default()
            }
        };
        Ok((name.to_string(), definition))
    }
}

pub struct McpServers {
    list: Vec<McpServer>,
    /// Whether the server has a harness configuration repository, so that servers can be edited; none until it said.
    harness: Option<bool>,
    editor: Option<Editor>,
    import: Import,
}

impl Default for McpServers {
    fn default() -> Self {
        McpServers {
            list: vec![],
            harness: None,
            editor: None,
            import: Import::new(import::Kind::McpServers),
        }
    }
}

fn args_action(action: text_editor::Action) -> AppMessage {
    AppMessage::Mcp(Message::Args(action))
}

impl McpServers {
    /// Reads the servers again.
    pub fn reload(&self, settings: &Settings) -> Task<AppMessage> {
        let (servers, skills) = (Client::new(settings.clone()), Client::new(settings.clone()));
        Task::batch([
            Task::perform(async move { servers.mcp_servers().await }, |result| {
                AppMessage::Mcp(Message::Listed(result))
            }),
            Task::perform(async move { skills.skills().await }, |result| {
                AppMessage::Mcp(Message::Harness(result))
            }),
        ])
    }

    /// Whether the server refused the token in an answer, which the app answers with the connect screen.
    pub fn refused(message: &Message) -> bool {
        matches!(
            message,
            Message::Listed(Err(ClientError::Unauthorized))
                | Message::Harness(Err(ClientError::Unauthorized))
        )
    }

    /// What a save would send: the server's name and definition, or what is wrong with the editor.
    pub fn save_request(&self) -> Result<(String, McpDefinition), String> {
        match &self.editor {
            Some(editor) => editor.request(),
            None => Err("No server is open.".into()),
        }
    }

    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        match message {
            Message::Listed(Ok(list)) => self.list = list,
            Message::Harness(Ok(skills)) => self.harness = Some(!skills.is_empty()),
            Message::Listed(Err(_)) | Message::Harness(Err(_)) => {}
            Message::Select(name) => {
                let mut editor = Editor::blank(&name);
                editor.reading = true;
                self.editor = Some(editor);
                let client = Client::new(settings.clone());
                return Task::perform(
                    {
                        let name = name.clone();
                        async move { client.mcp_server(&name).await }
                    },
                    move |result| AppMessage::Mcp(Message::Opened(name.clone(), result)),
                );
            }
            Message::Opened(name, result) => {
                if let Some(editor) = self.editor.as_mut().filter(|editor| editor.name == name) {
                    editor.reading = false;
                    match result {
                        Ok(detail) => editor.fill(detail.definition),
                        Err(e) => {
                            // A server that cannot be read is in the file, so it can be replaced or deleted.
                            editor.exists = true;
                            editor.notice = Some(Notice::Failed(say(&e)));
                        }
                    }
                }
            }
            Message::NewServer => {
                self.editor = Some(Editor {
                    naming: true,
                    ..Editor::blank("")
                });
            }
            Message::Save => return self.save(settings),
            Message::Saved(Ok(detail)) => {
                if let Some(editor) = &mut self.editor
                    && editor.name == detail.name
                {
                    editor.naming = false;
                    editor.exists = true;
                    editor.notice = Some(Notice::Saved(detail.commit));
                }
                return self.reload(settings);
            }
            Message::Delete => {
                if let Some(editor) = &self.editor {
                    let (client, name) = (Client::new(settings.clone()), editor.name.clone());
                    return Task::perform(
                        async move { client.delete_mcp_server(&name).await },
                        |result| AppMessage::Mcp(Message::Deleted(result)),
                    );
                }
            }
            Message::Deleted(Ok(())) => {
                self.editor = None;
                return self.reload(settings);
            }
            Message::Saved(Err(e)) | Message::Deleted(Err(e)) => {
                if let Some(editor) = &mut self.editor {
                    editor.confirming = false;
                    editor.notice = Some(Notice::Failed(say(&e)));
                }
            }
            Message::Import(message) => return self.import.update(message, settings),
            edit => self.edit(edit),
        }
        Task::none()
    }

    /// The changes of the editor's fields.
    fn edit(&mut self, message: Message) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        editor.notice = None;
        match message {
            Message::NameEdited(name) => editor.name = name,
            Message::KindChosen(kind) => editor.remote = kind == URL,
            Message::CommandEdited(command) => editor.command = command,
            Message::Args(action) => editor.args.perform(action),
            Message::UrlEdited(url) => editor.url = url,
            Message::RowNameEdited(at, name) => {
                if let Some(row) = editor.rows.get_mut(at) {
                    row.0 = name;
                }
            }
            Message::RowValueEdited(at, value) => {
                if let Some(row) = editor.rows.get_mut(at) {
                    row.1 = value;
                }
            }
            Message::AddRow => editor.rows.push((String::new(), String::new())),
            Message::RemoveRow(at) => {
                if at < editor.rows.len() {
                    editor.rows.remove(at);
                }
            }
            Message::AskDelete => editor.confirming = true,
            Message::Keep => editor.confirming = false,
            _ => {}
        }
    }

    fn save(&mut self, settings: &Settings) -> Task<AppMessage> {
        let request = self.save_request();
        let Some(editor) = &mut self.editor else {
            return Task::none();
        };
        match request {
            Ok((name, definition)) => {
                // A server saved under a new name is open under it from now on.
                editor.name.clone_from(&name);
                editor.notice = None;
                let client = Client::new(settings.clone());
                Task::perform(
                    async move { client.save_mcp_server(&name, &definition).await },
                    |result| AppMessage::Mcp(Message::Saved(result)),
                )
            }
            Err(problem) => {
                editor.notice = Some(Notice::Failed(problem));
                Task::none()
            }
        }
    }

    /// The MCP servers page: the list, and the editor of the server chosen.
    pub fn view<'a>(&'a self, harness: &'a HarnessSetup) -> Element<'a, AppMessage> {
        let editable = self.harness == Some(true);
        let new_server = editable.then_some(AppMessage::Mcp(Message::NewServer));
        let buttons = [
            page::white_button(
                "Import",
                new_server
                    .as_ref()
                    .map(|_| AppMessage::Mcp(Message::Import(import::Message::Show(true)))),
            ),
            page::white_button("New server", new_server.clone()),
        ]
        .map(|button| {
            if editable {
                button
            } else {
                harness_setup::muted(button)
            }
        });
        let header = page::header(
            "Build",
            "MCP servers",
            None,
            page::context(
                "The servers of mcp.toml in the harness configuration repository, which agents name to be given their tools. An edit is a commit.",
            ),
            row(buttons).spacing(10).into(),
        );
        if !editable {
            // Until the server has said, nothing is offered.
            let content = match self.harness {
                Some(_) => harness.view(),
                None => page::body("Reading the MCP servers…"),
            };
            return column![header, content]
                .spacing(theme::PAGE_GAP)
                .width(Length::Fill)
                .into();
        }
        let chosen = self.editor.as_ref().map(|editor| editor.name.as_str());
        let cards: Element<_> = if self.list.is_empty() {
            page::boxed("MCP servers", page::body("No server yet."))
        } else {
            column(self.list.iter().map(|server| {
                let (line, red) = match &server.error {
                    Some(error) => (error.as_str(), true),
                    None => (server.kind.as_str(), false),
                };
                list_card(
                    &server.name,
                    Vec::new(),
                    line,
                    red,
                    chosen == Some(server.name.as_str()),
                    Some(AppMessage::Mcp(Message::Select(server.name.clone()))),
                )
            }))
            .spacing(8)
            .padding(iced::Padding::default().right(12))
            .into()
        };
        let mut detail = column![].spacing(theme::PAGE_GAP).width(Length::Fill);
        if let Some(editor) = &self.editor {
            detail = detail.push(editor_view(editor));
        }
        list_editor::layout(
            header,
            self.import.is_open().then(|| self.import.view()),
            cards,
            detail,
        )
    }
}

fn editor_view(editor: &Editor) -> Element<'_, AppMessage> {
    let message = AppMessage::Mcp;
    let actions = list_editor::actions(
        &editor.name,
        editor.exists,
        editor.confirming,
        "Delete server",
        Actions {
            ask_delete: message(Message::AskDelete),
            keep: message(Message::Keep),
            delete: message(Message::Delete),
            save: (!editor.reading).then_some(message(Message::Save)),
        },
    );
    let head = list_editor::head(
        editor.naming,
        &editor.name,
        "Name of the new server",
        move |name| message(Message::NameEdited(name)),
        actions,
    );
    let mut top = column![head].spacing(12);
    if editor.reading {
        top = top.push(page::body("Reading the server…"));
    }
    match &editor.notice {
        Some(Notice::Saved(commit)) => top = top.push(committed_line(commit.as_deref())),
        Some(Notice::Failed(error)) => top = top.push(notice_line(error, true)),
        None => {}
    }
    let mut boxes = column![boxed(top)].spacing(theme::PAGE_GAP);
    if !editor.reading {
        boxes = boxes.push(definition_box(editor));
    }
    boxes.into()
}

fn mono_input<'a>(
    placeholder: &'a str,
    value: &'a str,
    edited: impl Fn(String) -> Message + 'a,
) -> Element<'a, AppMessage> {
    text_input(placeholder, value)
        .on_input(move |text| AppMessage::Mcp(edited(text)))
        .font(theme::MONO)
        .size(theme::CODE)
        .padding([8, 10])
        .style(connect::input_style)
        .into()
}

fn definition_box(editor: &Editor) -> Element<'_, AppMessage> {
    let message = AppMessage::Mcp;
    let kinds = vec![COMMAND.to_string(), URL.to_string()];
    let kind = pick_list(
        kinds,
        Some(if editor.remote { URL } else { COMMAND }.to_string()),
        move |kind| message(Message::KindChosen(kind)),
    )
    .text_size(theme::BODY)
    .padding([9, 12])
    .width(Length::Fill)
    .style(pick_style)
    .menu_style(menu_style);
    let mut content = column![row![field("Kind", kind), space::horizontal()]].spacing(12);
    if editor.remote {
        content = content.push(field(
            "URL",
            mono_input(
                "https://search.example.com/mcp",
                &editor.url,
                Message::UrlEdited,
            ),
        ));
    } else {
        content = content
            .push(field(
                "Command",
                mono_input("docs-server", &editor.command, Message::CommandEdited),
            ))
            .push(field(
                "Arguments, one per line",
                mono_editor(&editor.args, "--root\n/srv/docs", Some(args_action), 110),
            ));
    }
    let (heading, aside, add) = if editor.remote {
        ("Headers", "Values may be ${NAME}", "Add header")
    } else {
        ("Environment", "Values may be ${NAME}", "Add variable")
    };
    let rows = editor.rows.iter().enumerate().map(|(at, (key, value))| {
        row![
            mono_input("NAME", key, move |text| Message::RowNameEdited(at, text)),
            mono_input("value or ${NAME}", value, move |text| {
                Message::RowValueEdited(at, text)
            }),
            button(text("Remove").size(theme::LABEL))
                .padding([3, 8])
                .on_press(message(Message::RemoveRow(at)))
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
    content = content
        .push(box_heading(
            heading,
            text(aside)
                .size(theme::LABEL)
                .color(theme::TEXT_MUTED)
                .wrapping(Wrapping::None),
        ))
        .push(column(rows).spacing(8))
        .push(page::white_button(add, Some(message(Message::AddRow))));
    boxed(content)
}
