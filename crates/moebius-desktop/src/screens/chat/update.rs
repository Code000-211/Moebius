//! How a message changes the chat screen: the list and the chat read, a message sent, a chat stopped, ended or
//! offered, and the choices of a draft.

use iced::Task;

use crate::app::Message as AppMessage;
use crate::screens::live::say;
use crate::settings::Settings;

use super::thread::{OnPage, load};
use super::{Chats, Message, request};

impl Chats {
    pub fn update(&mut self, message: Message, settings: Option<&Settings>) -> Task<AppMessage> {
        let Some(settings) = settings else {
            return Task::none();
        };
        match message {
            Message::Listed(Ok(chats)) => {
                self.list = chats;
                self.show_started_terminal();
            }
            Message::Listed(Err(_)) => {}
            Message::Loaded(id, Ok(events)) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.thread.id == id) {
                    open.notice = None;
                    for event in events {
                        open.thread.push(event);
                    }
                }
            }
            Message::Loaded(id, Err(e)) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.thread.id == id) {
                    open.notice = Some(say(&e));
                }
            }
            Message::Judged(id, Ok(judgements)) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.thread.id == id) {
                    open.thread.judge(judgements);
                }
            }
            Message::Judged(_, Err(_)) => {}
            Message::Select(id) => return self.open(id, settings),
            Message::ShowTerminal(true) => self.terminal.open(),
            Message::ShowTerminal(false) => {
                self.terminal.close();
                if let Some(open) = &self.open {
                    return load::<OnPage>(
                        settings.clone(),
                        open.thread.id,
                        open.thread.last_seq(),
                    );
                }
            }
            Message::Terminal(message) => self.terminal.update(message),
            Message::Edited(text) => {
                if let Some(draft) = &mut self.draft {
                    draft.message = text;
                } else if let Some(open) = &mut self.open {
                    open.message = text;
                }
            }
            Message::Send => {
                if let Some(draft) = &mut self.draft {
                    if draft.message.trim().is_empty() || draft.sending || !draft.ready() {
                        return Task::none();
                    }
                    draft.sending = true;
                    draft.notice = None;
                    self.starting_terminal = false;
                    let chat = draft.new_chat(Some(draft.message.clone()));
                    return request(settings, Message::Started, move |client| async move {
                        client.start_chat(&chat).await
                    });
                }
                if let Some(open) = &self.open
                    && !open.message.trim().is_empty()
                    && let Some(chat) = self.chat(open.thread.id)
                {
                    let (id, message) = (open.thread.id, open.message.clone());
                    // Writing to an ended chat goes on in a new chat that is told this one.
                    if !chat.is_open() {
                        return request(settings, Message::Continued, move |client| async move {
                            client.continue_chat(id, &message).await
                        });
                    }
                    return request(settings, Message::Sent, move |client| async move {
                        client.say(id, &message, None).await
                    });
                }
            }
            Message::Continued(Ok(run)) => {
                return Task::batch([self.open(run.id, settings), self.reload(settings)]);
            }
            Message::Continued(Err(e)) => {
                if let Some(open) = &mut self.open {
                    open.notice = Some(say(&e));
                }
            }
            Message::Sent(result) => {
                if let Some(open) = &mut self.open {
                    match result {
                        Ok(_) => {
                            open.message.clear();
                            open.notice = None;
                        }
                        Err(e) => open.notice = Some(say(&e)),
                    }
                }
            }
            Message::AskStop => self.set_confirming(true),
            Message::KeepGoing => self.set_confirming(false),
            Message::Stop => {
                self.set_confirming(false);
                if let Some(open) = &self.open {
                    let id = open.thread.id;
                    return request(settings, Message::Ended, move |client| async move {
                        client.stop(id).await
                    });
                }
            }
            Message::End => {
                if let Some(open) = &self.open {
                    let id = open.thread.id;
                    return request(settings, Message::Ended, move |client| async move {
                        client.end_chat(id).await
                    });
                }
            }
            Message::Ended(result) => {
                if let Some(open) = &mut self.open {
                    open.notice = result.err().map(|e| say(&e));
                }
            }
            Message::Offer => {
                if let Some(open) = &self.open {
                    let id = open.thread.id;
                    return request(settings, Message::Offered, move |client| async move {
                        client.offer_chat(id).await
                    });
                }
            }
            Message::Offered(result) => {
                if let Some(open) = &mut self.open {
                    open.notice = Some(match result {
                        Ok(change) => format!(
                            "Offered as a change on {}: it waits in the Inbox, ready to merge.",
                            change.branch
                        ),
                        Err(e) => say(&e),
                    });
                }
                return self.reload(settings);
            }
            Message::NewChat => return self.open_draft(settings),
            Message::NewChatIn(repository) => {
                let task = self.open_draft(settings);
                if let Some(draft) = &mut self.draft {
                    draft.repository = Some(repository);
                }
                return task;
            }
            Message::Agents(Ok(agents)) => {
                // An agent whose files are broken is listed without a model and cannot be chosen.
                self.agents = agents
                    .into_iter()
                    .filter(|agent| !agent.model.is_empty())
                    .collect();
                // The Moebius agent is the first, if the server can run it.
                if let (Some(draft), Some(first)) = (&mut self.draft, self.agents.first())
                    && !self.agents.iter().any(|agent| agent.name == draft.agent)
                {
                    draft.agent = first.name.clone();
                }
            }
            Message::Agents(Err(e)) => {
                if let Some(draft) = &mut self.draft {
                    draft.notice = Some(say(&e));
                }
            }
            Message::Models(Ok(available)) => {
                let mut models: Vec<String> = available.models.into_iter().map(|m| m.id).collect();
                models.sort();
                models.dedup();
                // The model classes come first.
                let classes = available.classes.into_iter().map(|class| class.name);
                self.models = classes.chain(models).collect();
            }
            Message::Models(Err(_)) => {}
            Message::AgentChosen(agent) => {
                if let Some(draft) = &mut self.draft {
                    // The model and reasoning follow the agent unless the person chose others.
                    draft.agent = agent;
                }
            }
            Message::Skills(skills) => {
                self.has_harness = skills.is_ok_and(|skills| !skills.is_empty());
            }
            Message::RepositoryChosen(repository) => {
                if let Some(draft) = &mut self.draft {
                    draft.repository = repository;
                    draft.harness = false;
                }
            }
            Message::HarnessChosen => {
                if let Some(draft) = self.draft.as_mut().filter(|_| self.has_harness) {
                    draft.repository = None;
                    draft.harness = true;
                }
            }
            Message::ModelChosen(model) => return self.choose(settings, Some(model), None),
            Message::ThinkingChosen(level) => return self.choose(settings, None, Some(level)),
            Message::SettingsChanged(result) => {
                if let Some(open) = &mut self.open {
                    open.notice = result.as_ref().err().map(say);
                }
                return self.reload_list(settings);
            }
            Message::StartInTerminal => {
                if let Some(draft) = &mut self.draft
                    && draft.ready()
                    && !draft.sending
                {
                    draft.sending = true;
                    draft.notice = None;
                    self.starting_terminal = true;
                    let chat = draft.new_chat(None);
                    return request(settings, Message::Started, move |client| async move {
                        client.start_chat(&chat).await
                    });
                }
            }
            Message::Started(Ok(run)) => {
                let task = Task::batch([self.open(run.id, settings), self.reload(settings)]);
                if std::mem::take(&mut self.starting_terminal) {
                    self.terminal_next = Some(run.id);
                    if run.status == "waiting" {
                        self.terminal_next = None;
                        self.terminal.open();
                    }
                }
                return task;
            }
            Message::Started(Err(e)) => {
                self.starting_terminal = false;
                if let Some(draft) = &mut self.draft {
                    draft.sending = false;
                    draft.notice = Some(say(&e));
                }
            }
            Message::Tick(now) => self.now = now,
            Message::MarkedRead => {}
            Message::Fold(step) => {
                if let Some(open) = &mut self.open {
                    open.thread.fold(step);
                }
            }
            Message::LinkClicked(uri) => {
                if let Some(open) = &mut self.open {
                    open.notice = Some(format!("Links are not opened from here: {uri}"));
                }
            }
        }
        Task::none()
    }

    /// A model or a reasoning level was chosen: the draft's, or, while the open chat waits, the chat's, which the
    /// server is asked to change. Choosing what the agent has keeps the agent's.
    fn choose(
        &mut self,
        settings: &Settings,
        model: Option<String>,
        thinking: Option<String>,
    ) -> Task<AppMessage> {
        if let Some(draft) = &mut self.draft {
            let agent = self.agents.iter().find(|agent| agent.name == draft.agent);
            if let Some(model) = model {
                let agents = agent.is_some_and(|agent| agent.model == model);
                draft.model = (!agents).then_some(model);
            }
            if let Some(level) = thinking {
                let agents = agent.is_some_and(|agent| agent.thinking.as_ref() == Some(&level));
                draft.thinking = (!agents).then_some(level);
            }
            return Task::none();
        }
        let Some(open) = &self.open else {
            return Task::none();
        };
        if self
            .chat(open.thread.id)
            .is_none_or(|chat| chat.status != "waiting")
        {
            return Task::none();
        }
        let id = open.thread.id;
        request(
            settings,
            Message::SettingsChanged,
            move |client| async move {
                client
                    .change_chat_settings(id, model.as_deref(), thinking.as_deref())
                    .await
            },
        )
    }

    fn set_confirming(&mut self, confirming: bool) {
        if let Some(open) = &mut self.open {
            open.confirming = confirming;
        }
    }
}
