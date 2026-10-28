//! Importing skills or MCP servers: the panel that the "Import" button of the Skills and MCP servers pages opens. The
//! person gives a zip file, which is sent to the server with `POST /v1/uploads`, or a link to a repository or a page,
//! and "Ask the agent" starts a chat with the Moebius agent that reads the source and writes what it finds with the
//! same `PUT` routes the pages use. The app shows the chat in the floating window.

use std::path::PathBuf;

use iced::widget::{column, row, text_input};
use iced::{Alignment, Element, Length, Task};

use crate::agents::{field, notice_line};
use crate::app::Message as AppMessage;
use crate::client::{Client, ClientError, Run, Upload};
use crate::live::say;
use crate::settings::Settings;
use crate::skills::boxed;
use crate::{connect, mcp_servers, page, skills, theme};

/// The largest file the server takes (`UPLOAD_LIMIT` of the desktop API): a larger one is not sent.
const LIMIT: u64 = 20 * 1024 * 1024;

/// What is imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Skills,
    McpServers,
}

/// Where the agent finds what to import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A zip file stored on the server, at this path.
    Zip(String),
    /// A git repository or a page.
    Link(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    /// "Import" opens the panel (true); "Cancel" closes it.
    Show(bool),
    /// "From a zip" (true) or "From a link" (false) was chosen.
    FromZip(bool),
    /// "Choose file…" was pressed.
    Choose,
    /// The file dialog answered: the file chosen, or none when it was closed.
    Chosen(Option<PathBuf>),
    /// The file with this name was sent: where the server keeps it, or why it was not taken.
    Uploaded(String, Result<Upload, String>),
    LinkEdited(String),
    NoteEdited(String),
    /// "Ask the agent" was pressed.
    Ask,
    /// The server started the Moebius agent's chat, or said why not.
    Asked(Result<Run, ClientError>),
}

/// The import panel of one page.
pub struct Import {
    kind: Kind,
    open: bool,
    from_zip: bool,
    /// The name of the file chosen, while it is sent and after.
    file: Option<String>,
    /// Where the server keeps the file, once it took it.
    uploaded: Option<String>,
    uploading: bool,
    link: String,
    note: String,
    /// Whether the request to the agent is on its way.
    asking: bool,
    error: Option<String>,
}

impl Import {
    pub fn new(kind: Kind) -> Self {
        Import {
            kind,
            open: false,
            from_zip: true,
            file: None,
            uploaded: None,
            uploading: false,
            link: String::new(),
            note: String::new(),
            asking: false,
            error: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The source the agent would be given: the uploaded zip or the link, whichever is chosen; none before it is given.
    fn source(&self) -> Option<Source> {
        if self.from_zip {
            self.uploaded.clone().map(Source::Zip)
        } else {
            let link = self.link.trim();
            (!link.is_empty()).then(|| Source::Link(link.to_string()))
        }
    }

    /// What "Ask the agent" would send the Moebius agent; none until a source is given.
    pub fn request(&self) -> Option<String> {
        self.source()
            .map(|source| request(self.kind, &source, self.note.trim()))
    }

    fn wrap(&self) -> fn(Message) -> AppMessage {
        match self.kind {
            Kind::Skills => |message| AppMessage::Skills(skills::Message::Import(message)),
            Kind::McpServers => |message| AppMessage::Mcp(mcp_servers::Message::Import(message)),
        }
    }

    pub fn update(&mut self, message: Message, settings: &Settings) -> Task<AppMessage> {
        let wrap = self.wrap();
        match message {
            Message::Show(open) => {
                *self = Import::new(self.kind);
                self.open = open;
            }
            Message::FromZip(from_zip) => {
                self.from_zip = from_zip;
                self.error = None;
            }
            Message::Choose => {
                let dialog = rfd::AsyncFileDialog::new()
                    .set_title("Choose a zip file")
                    .add_filter("Zip", &["zip"]);
                return Task::perform(async move { dialog.pick_file().await }, move |file| {
                    wrap(Message::Chosen(file.map(|file| file.path().to_path_buf())))
                });
            }
            Message::Chosen(None) => {}
            Message::Chosen(Some(path)) => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.file = Some(name.clone());
                self.uploaded = None;
                self.uploading = true;
                self.error = None;
                let client = Client::new(settings.clone());
                return Task::perform(send(client, path), move |result| {
                    wrap(Message::Uploaded(name.clone(), result))
                });
            }
            Message::Uploaded(name, result) => {
                if self.file.as_deref() == Some(name.as_str()) {
                    self.uploading = false;
                    match result {
                        Ok(upload) => self.uploaded = Some(upload.path),
                        Err(error) => self.error = Some(error),
                    }
                }
            }
            Message::LinkEdited(link) => {
                self.link = link;
                self.error = None;
            }
            Message::NoteEdited(note) => self.note = note,
            Message::Ask => {
                let Some(asked) = self.request().filter(|_| !self.asking) else {
                    return Task::none();
                };
                self.asking = true;
                self.error = None;
                let client = Client::new(settings.clone());
                return Task::perform(
                    async move { client.ask(&asked, None, "").await },
                    move |result| wrap(Message::Asked(result)),
                );
            }
            Message::Asked(Ok(_)) => *self = Import::new(self.kind),
            Message::Asked(Err(e)) => {
                self.asking = false;
                self.error = Some(say(&e));
            }
        }
        Task::none()
    }

    /// The panel, under the page's header while it is open.
    pub fn view(&self) -> Element<'_, AppMessage> {
        let wrap = self.wrap();
        let what = match self.kind {
            Kind::Skills => "skills",
            Kind::McpServers => "MCP servers",
        };
        let ways = row![
            page::chip(
                "From a zip".to_string(),
                self.from_zip,
                false,
                wrap(Message::FromZip(true))
            ),
            page::chip(
                "From a link".to_string(),
                !self.from_zip,
                false,
                wrap(Message::FromZip(false))
            ),
        ]
        .spacing(8);
        let source: Element<_> = if self.from_zip {
            let chosen = match (&self.file, self.uploading, &self.uploaded) {
                (Some(file), true, _) => format!("Sending {file}…"),
                (Some(file), false, Some(_)) => format!("{file} is on the server."),
                _ => "No file chosen.".to_string(),
            };
            row![
                page::white_button(
                    "Choose file…",
                    (!self.uploading).then_some(wrap(Message::Choose))
                ),
                page::body(chosen),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        } else {
            field(
                "A git repository or a page",
                text_input("https://github.com/acme/skills", &self.link)
                    .on_input(move |link| wrap(Message::LinkEdited(link)))
                    .size(theme::BODY)
                    .padding([9, 12])
                    .style(connect::input_style),
            )
        };
        let note = field(
            "Note (optional)",
            text_input("What to import, which key it needs", &self.note)
                .on_input(move |note| wrap(Message::NoteEdited(note)))
                .size(theme::BODY)
                .padding([9, 12])
                .style(connect::input_style),
        );
        let ask = (self.request().is_some() && !self.asking && !self.uploading)
            .then_some(wrap(Message::Ask));
        let mut content = column![
            page::body(format!(
                "The Moebius agent reads the {what} from a zip file or a repository and saves them here; the chat opens."
            )),
            ways,
            source,
            note,
        ]
        .spacing(12)
        .width(Length::Fill);
        if let Some(error) = &self.error {
            content = content.push(notice_line(error, true));
        }
        content = content.push(
            row![
                page::primary_button("Ask the agent", ask),
                page::white_button("Cancel", Some(wrap(Message::Show(false)))),
            ]
            .spacing(10),
        );
        boxed(content)
    }
}

/// Reads the file at `path` and sends it to the server; what is wrong is said in a line.
async fn send(client: Client, path: PathBuf) -> Result<Upload, String> {
    let size = std::fs::metadata(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?
        .len();
    if size > LIMIT {
        return Err("The file is larger than 20 MB, the most the server takes.".into());
    }
    let zip = std::fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    client.upload(zip).await.map_err(|e| match e {
        ClientError::Server { status: 413, .. } => {
            "The file is larger than the server takes.".to_string()
        }
        e => say(&e),
    })
}

/// What the Moebius agent is asked: where the source is, how to fetch it, what to write and what to say at the end,
/// with the person's note.
pub fn request(kind: Kind, source: &Source, note: &str) -> String {
    let fetch = match source {
        Source::Zip(path) => format!(
            "The source is the zip file `{path}` on the server. Unzip it into a new directory of your working \
             directory (`unzip -q {path} -d import`) and read it there."
        ),
        Source::Link(url) => format!(
            "The source is {url}. If it is a git repository, clone it into a new directory of your working directory \
             (`git clone --depth 1 {url} import`) and read it there; if it is a page, read it and follow it to the \
             repository or the files it points to."
        ),
    };
    let task = match kind {
        Kind::Skills => {
            "Import skills into the harness configuration repository. Read the skills and MCP servers section of the \
             moebius-configure skill first.\n\n{fetch}\n\nA skill is a directory with a SKILL.md. List the skills \
             there are with `\"$MOEBIUS\" api GET /v1/skills` first: never replace an existing skill without saying \
             so (ask the person), and never import a skill under a built-in name (`moebius` and the `moebius-` task \
             skills); give it another name instead. For each skill, write its description (one line that says when \
             to use it) and the text of its SKILL.md after the frontmatter with `\"$MOEBIUS\" api PUT \
             /v1/skills/<name> --body body.json`, building the body with `jq`. The route writes only SKILL.md: name \
             the skill's other files, which the person adds through a chat in the harness configuration repository. \
             Read each skill back with `\"$MOEBIUS\" api GET /v1/skills/<name>`.\n\nThen list what was imported, \
             under which names, and what the person must do: the API keys or programs a skill needs (a key is stored \
             on the Settings page, never written into a skill) and which agents should be given the skill."
        }
        Kind::McpServers => {
            "Import MCP servers into mcp.toml of the harness configuration repository. Read the skills and MCP \
             servers section of the moebius-configure skill and `mcp.toml` in reference/harness-configuration.md \
             first.\n\n{fetch}\n\nFind how each server is run in its documentation and configuration: the README, \
             package.json or pyproject.toml, and examples such as mcp.json or claude_desktop_config.json. List the \
             servers there are with `\"$MOEBIUS\" api GET /v1/mcp-servers` first and never replace one without \
             saying so (ask the person). Write each server with `\"$MOEBIUS\" api PUT /v1/mcp-servers/<name> --body \
             body.json`: `command`, `args` and `env` for a server that runs as a command, or `url` and `headers` for \
             a remote one. Never write a key into it: a key is `${NAME}`, and NAME must be in `harness.mcp_env` of \
             the settings (`\"$MOEBIUS\" settings`); add a name that is missing with `\"$MOEBIUS\" settings set`, \
             keeping the names there, and say whether the server must restart. Read each server back with \
             `\"$MOEBIUS\" api GET /v1/mcp-servers/<name>`.\n\nThen list what was imported, under which names, and \
             what the person must do: each key to set in the server's environment under its NAME, the programs the \
             server needs on the server, and which agents should be given it."
        }
    };
    let mut asked = task.replace("{fetch}", &fetch);
    if !note.is_empty() {
        asked.push_str(&format!("\n\nWhat the person said:\n\n{note}"));
    }
    asked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importing_skills_from_a_zip_unzips_it_and_saves_each_skill_without_replacing_one() {
        let path = "/srv/moebius/uploads/ab12.zip".to_string();
        let asked = request(Kind::Skills, &Source::Zip(path), "only the pdf skill");
        assert!(asked.contains("`/srv/moebius/uploads/ab12.zip`"));
        assert!(asked.contains("unzip -q /srv/moebius/uploads/ab12.zip -d import"));
        assert!(asked.contains(r#"`"$MOEBIUS" api GET /v1/skills`"#));
        assert!(asked.contains(r#"`"$MOEBIUS" api PUT /v1/skills/<name> --body body.json`"#));
        assert!(asked.contains("never replace an existing skill without saying so"));
        assert!(asked.contains("built-in name"));
        assert!(asked.contains("Settings page"));
        assert!(asked.ends_with("What the person said:\n\nonly the pdf skill"));
    }

    #[test]
    fn importing_an_mcp_server_from_a_link_clones_it_and_takes_keys_as_names() {
        let url = "https://github.com/acme/docs-mcp".to_string();
        let asked = request(Kind::McpServers, &Source::Link(url), "");
        assert!(asked.contains("git clone --depth 1 https://github.com/acme/docs-mcp import"));
        assert!(asked.contains(r#"`"$MOEBIUS" api PUT /v1/mcp-servers/<name> --body body.json`"#));
        assert!(asked.contains("README"));
        assert!(asked.contains("`${NAME}`"));
        assert!(asked.contains("`harness.mcp_env`"));
        assert!(asked.contains("server's environment"));
        assert!(!asked.contains("What the person said"));
    }
}
