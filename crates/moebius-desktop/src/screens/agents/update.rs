//! How a message changes the Agents page: the lists read, an agent opened, edited, saved or deleted, and the
//! instructions asked of the Moebius agent.

use iced::Task;
use iced::widget::text_editor;

use crate::app::Message as AppMessage;
use crate::client::{AgentDetail, Client, ClientError};
use crate::screens::live::say;
use crate::settings::Settings;
use crate::widgets::form::PROTOCOLS;

use super::boxes::instructions_request;
use super::editor::{Editor, file_table, from_file, toggle};
use super::{Agents, COMPRESS, ENGINEERING, Files, Message, NOT_SET, Notice, THIS_SERVER};

impl Agents {
    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        let in_file = self.in_file();
        match message {
            Message::Listed(Ok(list)) => self.list = list,
            Message::SkillsListed(Ok(skills)) => self.skills = Some(skills),
            Message::McpListed(Ok(mcp)) => self.mcp = mcp,
            Message::ProposalsListed(Ok(proposals)) => self.proposals = proposals,
            Message::RunnersListed(Ok(runners)) => self.runners = runners,
            Message::RunnersListed(Err(_)) => {}
            Message::Available(Ok(available)) => self.available = Some(available),
            Message::Available(Err(_)) => {}
            Message::RefreshAvailable => return self.ask_available(settings, true),
            Message::Listed(Err(_))
            | Message::SkillsListed(Err(_))
            | Message::McpListed(Err(_))
            | Message::ProposalsListed(Err(_)) => {}
            Message::Select(name) => return self.open(name, settings),
            Message::Opened(name, result) => self.opened(&name, result),
            Message::BuiltIn(name, Ok(detail)) => {
                if let Some(editor) = self.editor.as_mut().filter(|editor| editor.name == name) {
                    editor.built_in = detail.built_in;
                }
            }
            Message::BuiltIn(_, Err(_)) => {}
            Message::OpenedFromFile(name, result) => {
                let result = result.and_then(|server| from_file(&name, &server));
                self.opened(&name, result);
            }
            Message::NewAgent => {
                self.editor = Some(Editor {
                    naming: true,
                    ..Editor::blank("")
                });
            }
            Message::Save => return self.save(settings),
            Message::AskAgent => {
                let Some(editor) = &mut self.editor else {
                    return Task::none();
                };
                let wanted = editor.wish.text().trim().to_string();
                if wanted.is_empty() || editor.asking {
                    return Task::none();
                }
                editor.asking = true;
                editor.notice = None;
                let client = Client::new(settings.clone());
                let message = instructions_request(&editor.name, &wanted);
                return Task::perform(
                    async move { client.ask(&message, None, "").await },
                    |result| AppMessage::Agents(Message::Asked(result)),
                );
            }
            Message::Asked(result) => {
                if let Some(editor) = &mut self.editor {
                    editor.asking = false;
                    match result {
                        Ok(_) => {
                            editor.by_agent = false;
                            editor.wish = text_editor::Content::new();
                        }
                        Err(e) => editor.notice = Some(Notice::Failed(say(&e))),
                    }
                }
            }
            Message::Saved(Ok(detail)) => {
                if let Some(editor) = &mut self.editor
                    && editor.name == detail.name
                {
                    editor.naming = false;
                    editor.exists = true;
                    editor.kept = editor.request().ok();
                    editor.notice = Some(if in_file {
                        Notice::Written
                    } else {
                        Notice::Saved(detail.commit)
                    });
                }
                return self.reload(settings);
            }
            Message::Delete => {
                if let Some(editor) = &self.editor {
                    let (client, name) = (Client::new(settings.clone()), editor.name.clone());
                    return Task::perform(
                        async move {
                            if in_file {
                                client
                                    .save_settings(
                                        Default::default(),
                                        vec![format!("agents.{name}")],
                                    )
                                    .await
                                    .map(drop)
                            } else {
                                client.delete_agent(&name).await
                            }
                        },
                        |result| AppMessage::Agents(Message::Deleted(result)),
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
            edit => self.edit(edit),
        }
        Task::none()
    }

    /// The changes of the editor's fields.
    pub(super) fn edit(&mut self, message: Message) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        editor.notice = None;
        match message {
            Message::NameEdited(name) => editor.name = name,
            Message::ModelEdited(model) => editor.model = model,
            Message::TypeModel => editor.typing_model = true,
            Message::ModelPicked(model) => {
                editor.model = model.id;
                if PROTOCOLS.contains(&model.protocol.as_str()) {
                    editor.protocol = model.protocol;
                }
            }
            Message::ProtocolChosen(protocol) => editor.protocol = protocol,
            Message::HarnessChosen(harness) => editor.harness = harness,
            Message::RunnerChosen(runner) => {
                editor.runner = (runner != THIS_SERVER).then_some(runner)
            }
            Message::ThinkingChosen(level) => {
                editor.thinking = (level != NOT_SET).then_some(level);
            }
            Message::BudgetEdited(budget) => editor.budget = budget,
            Message::HideAfterEdited(turns) => editor.hide_after = turns,
            Message::MaxLinesEdited(lines) => editor.max_lines = lines,
            Message::MaxBytesEdited(bytes) => editor.max_bytes = bytes,
            Message::CompressChosen(choice) => editor.compress = choice == COMPRESS[0],
            Message::EngineeringChosen(choice) => {
                editor.engineering_skills = choice == ENGINEERING[0]
            }
            Message::Instructions(action) => editor.instructions.perform(action),
            Message::InstructionsByAgent(by_agent) => editor.by_agent = by_agent,
            Message::WishEdited(action) => editor.wish.perform(action),
            Message::ToggleSkill(name) => toggle(&mut editor.skills, name),
            Message::ToggleMcp(name) => toggle(&mut editor.mcp, name),
            Message::SkillPathEdited(path) => editor.skill_path = path,
            Message::AddSkillPath => {
                let path = editor.skill_path.trim().to_string();
                if !path.is_empty() && !editor.skills.contains(&path) {
                    editor.skills.push(path);
                }
                editor.skill_path.clear();
            }
            Message::RemoveSkillPath(path) => editor.skills.retain(|held| *held != path),
            Message::ExtensionEdited(path) => editor.extension = path,
            Message::AddExtension => {
                let path = editor.extension.trim().to_string();
                if !path.is_empty() && !editor.extensions.contains(&path) {
                    editor.extensions.push(path);
                }
                editor.extension.clear();
            }
            Message::RemoveExtension(path) => editor.extensions.retain(|held| *held != path),
            Message::AskDelete => editor.confirming = true,
            Message::KeepAgent => editor.confirming = false,
            _ => {}
        }
    }

    pub(super) fn open(&mut self, name: String, settings: &Settings) -> Task<AppMessage> {
        let mut editor = Editor::blank(&name);
        editor.files = Files::Reading;
        self.editor = Some(editor);
        let client = Client::new(settings.clone());
        if self.in_file() {
            let built_in = self
                .list
                .iter()
                .any(|agent| agent.name == name && agent.built_in);
            let read = {
                let client = Client::new(settings.clone());
                let name = name.clone();
                Task::perform(
                    async move { client.server_settings().await },
                    move |result| AppMessage::Agents(Message::OpenedFromFile(name.clone(), result)),
                )
            };
            if !built_in {
                return read;
            }
            // The server answers a built-in agent's built-in part without a harness configuration repository.
            let built_in = Task::perform(
                {
                    let name = name.clone();
                    async move { client.agent(&name).await }
                },
                move |result| AppMessage::Agents(Message::BuiltIn(name.clone(), result)),
            );
            return Task::batch([read, built_in]);
        }
        Task::perform(
            {
                let name = name.clone();
                async move { client.agent(&name).await }
            },
            move |result| AppMessage::Agents(Message::Opened(name.clone(), result)),
        )
    }

    pub(super) fn opened(&mut self, name: &str, result: Result<AgentDetail, ClientError>) {
        let built_in_without_files = self
            .list
            .iter()
            .find(|agent| agent.name == name)
            .filter(|agent| agent.built_in)
            .cloned();
        let Some(editor) = self.editor.as_mut().filter(|editor| editor.name == name) else {
            return;
        };
        match result {
            Ok(detail) => editor.fill(detail),
            // The Moebius agent is there without files; saving it makes them.
            Err(ClientError::Server { status: 404, .. }) if built_in_without_files.is_some() => {
                editor.files = Files::Ready;
            }
            Err(e) => {
                // A broken agent has files, so it can be deleted.
                editor.exists = true;
                editor.files = Files::Broken(say(&e));
            }
        }
    }

    pub(super) fn save(&mut self, settings: &Settings) -> Task<AppMessage> {
        let (request, in_file) = (self.save_request(), self.in_file());
        let Some(editor) = &mut self.editor else {
            return Task::none();
        };
        match request {
            Ok((name, agent, instructions)) => {
                // An agent saved under a new name is open under it from now on.
                editor.name.clone_from(&name);
                editor.notice = None;
                let client = Client::new(settings.clone());
                if in_file {
                    let set = serde_json::Map::from_iter([(
                        format!("agents.{name}"),
                        file_table(&agent, &instructions),
                    )]);
                    return Task::perform(
                        async move {
                            client
                                .save_settings(set, vec![])
                                .await
                                .map(|_| AgentDetail {
                                    name,
                                    settings: Some(agent),
                                    instructions,
                                    built_in: None,
                                    commit: None,
                                })
                        },
                        |result| AppMessage::Agents(Message::Saved(result)),
                    );
                }
                Task::perform(
                    async move { client.save_agent(&name, &agent, &instructions).await },
                    |result| AppMessage::Agents(Message::Saved(result)),
                )
            }
            Err(problem) => {
                editor.notice = Some(Notice::Failed(problem));
                Task::none()
            }
        }
    }
}
