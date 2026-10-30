//! How a message changes the app: `update` hands each to the screen it is for, and the steps that more than one
//! message takes (connecting, reading the snapshot again, opening a card or a page) are here too.

use iced::Task;

use crate::client::{self, Client, ClientError, Snapshot};
use crate::screens::add_repository::{self, AddRepository};
use crate::screens::agents::{self, Agents};
use crate::screens::automations::Automations;
use crate::screens::chat;
use crate::screens::decision_model::DecisionModel;
use crate::screens::experiments::Experiments;
use crate::screens::harness_setup::HarnessSetup;
use crate::screens::inbox;
use crate::screens::live::{self, Live};
use crate::screens::mcp_servers::{self, McpServers};
use crate::screens::project::{self, Project, Tab};
use crate::screens::repositories::{self, Repositories};
use crate::screens::retro::Retros;
use crate::screens::rules::{self, Rules};
use crate::screens::setup::{self, Setup};
use crate::screens::skills::{self, Skills};
use crate::screens::start::{self, Start};
use crate::screens::usage::Usage;
use crate::search::{self, Target};
use crate::settings::{self, Settings};
use crate::widgets::edits::Edits;
use crate::widgets::float_chat;
use crate::widgets::import;
use crate::widgets::new_issue::{self, NewIssue};
use crate::widgets::script_editor::{self, ScriptEditor};

use super::message::{Message, Page};
use super::{App, Connection, Form, Opened, Screen, check};

impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        // The steps are read when their page opens, and when any page opens while one is missing, so that the
        // sidebar drops the entry soon after the last step is done.
        let reads_start = matches!(&message, Message::Select(page) | Message::Visit(page) if *page == Page::Start || self.start.incomplete());
        let left = Self::navigates(&message).then(|| self.location());
        let task = self.handle(message);
        if let Some(left) = left {
            self.arrive(left);
        }
        let task = match (&self.settings, reads_start) {
            (Some(settings), true) => Task::batch([task, self.start.reload(settings)]),
            _ => task,
        };
        // What the chat on the screen holds is read, however it got there. The Fleet shows the working runs as they
        // are now, and nothing once it is left.
        match (&self.screen, self.page(), &self.settings) {
            (Screen::Main, Page::Chat, Some(settings)) => {
                let read = self.chats.mark_read(&settings.clone());
                self.fleet.clear();
                Task::batch([task, read])
            }
            (Screen::Main, Page::Fleet, Some(settings)) => {
                let follow = self
                    .fleet
                    .follow(&self.snapshot, self.chats.list(), settings);
                Task::batch([task, follow])
            }
            _ => {
                self.fleet.clear();
                task
            }
        }
    }

    pub(super) fn handle(&mut self, message: Message) -> Task<Message> {
        if refused(&message) {
            self.show_connect_error(&ClientError::Unauthorized);
            return Task::none();
        }
        match message {
            Message::AddressChanged(address) => self.form.address = address,
            Message::TokenChanged(token) => self.form.token = token,
            Message::Submit => return self.submit(),
            Message::Checked(Ok((settings, snapshot))) => return self.adopt(settings, snapshot),
            Message::Checked(Err(e)) => self.show_connect_error(&e),
            Message::Reported(_) => {}
            Message::Loaded(count, Ok(snapshot)) => {
                if count > self.shown {
                    self.shown = count;
                    self.snapshot = snapshot;
                    self.inbox.follow(&self.snapshot);
                }
            }
            Message::Loaded(_, Err(ClientError::Unauthorized)) => {
                self.show_connect_error(&ClientError::Unauthorized);
            }
            Message::Loaded(_, Err(_)) => self.connection = Connection::Offline,
            Message::Stream(event) => return self.on_stream(event),
            Message::Select(page) => {
                self.history.clear();
                return self.show_page(page);
            }
            Message::Visit(page) => return self.show_page(page),
            Message::Back => {
                let Some(location) = self.history.pop() else {
                    return Task::none();
                };
                return self.go_to(location);
            }
            Message::OpenCard(id) => {
                self.edits.close();
                return self.open(id);
            }
            Message::CloseCard => {
                self.edits.close();
                self.opened = None;
            }
            Message::Edit(message) => {
                let saved = Edits::saved(&message);
                let task = self.edits.update(message, self.settings.as_ref());
                if saved {
                    return Task::batch([task, self.reload()]);
                }
                return task;
            }
            Message::EditScript(repository, column) => {
                let (Some(settings), Some(board)) = (
                    &self.settings,
                    self.snapshot
                        .boards
                        .iter()
                        .find(|board| board.repository.id == repository),
                ) else {
                    return Task::none();
                };
                let (editor, task) =
                    ScriptEditor::open(repository, &board.repository.name, &column, settings);
                self.opened = Some(Opened::Script(Box::new(editor)));
                self.projects
                    .entry(repository)
                    .or_default()
                    .show_tab(Tab::Board);
                return task;
            }
            Message::Project(repository, message) => {
                if matches!(message, project::Message::Tab(_)) {
                    // A tab is chosen over a card's page, which belongs to the board.
                    self.opened = None;
                }
                return self.projects.entry(repository).or_default().update(
                    repository,
                    message,
                    self.settings.as_ref(),
                );
            }
            Message::SaveFile => {
                if let (Page::Board(repository), None) = (self.page(), &self.opened)
                    && let Some(project) = self.projects.get_mut(&repository)
                    && matches!(project.tab(), Tab::Editor(_))
                {
                    return project.update(
                        repository,
                        project::Message::Save,
                        self.settings.as_ref(),
                    );
                }
            }
            Message::ProjectKey(event) => {
                if let (Page::Board(repository), None) = (self.page(), &self.opened)
                    && let Some(project) = self.projects.get_mut(&repository)
                {
                    return project.update(
                        repository,
                        project::Message::Key(event),
                        self.settings.as_ref(),
                    );
                }
            }
            Message::Find(message) => {
                if let (Page::Board(repository), None) = (self.page(), &self.opened) {
                    return self.projects.entry(repository).or_default().update(
                        repository,
                        project::Message::Find(message),
                        self.settings.as_ref(),
                    );
                }
            }
            Message::Script(script_editor::Message::AskAgent) => {
                // The floating chat opens with the start of a message about the script; nothing is sent.
                let Some(Opened::Script(editor)) = &self.opened else {
                    return Task::none();
                };
                return self.float.prefill(editor.about());
            }
            Message::Script(message) => {
                if let Some(Opened::Script(editor)) = &mut self.opened {
                    return editor.update(message, self.settings.as_ref());
                }
            }
            Message::Live(message) => {
                if let Some(Opened::Run(live)) = &mut self.opened {
                    return live.update(message, self.settings.as_ref());
                }
            }
            Message::Inbox(message) => return self.on_inbox(message),
            Message::Agents(agents::Message::Asked(Ok(run))) => {
                // The Moebius agent writes the instructions in a chat, which the floating chat shows.
                let settings = self.settings.clone();
                let task = self
                    .agents
                    .update(agents::Message::Asked(Ok(run.clone())), settings.as_ref());
                return Task::batch([task, self.float_adopt(&run, None)]);
            }
            Message::Agents(message) => return self.agents.update(message, self.settings.as_ref()),
            Message::Skills(skills::Message::Import(import::Message::Asked(Ok(run)))) => {
                // The Moebius agent imports in a chat, which the floating chat shows.
                let (asked, settings) = (
                    import::Message::Asked(Ok(run.clone())),
                    self.settings.clone(),
                );
                let task = self
                    .skills
                    .update(skills::Message::Import(asked), settings.as_ref());
                return Task::batch([task, self.float_adopt(&run, None)]);
            }
            Message::Mcp(mcp_servers::Message::Import(import::Message::Asked(Ok(run)))) => {
                let (asked, settings) = (
                    import::Message::Asked(Ok(run.clone())),
                    self.settings.clone(),
                );
                let task = self
                    .mcp_servers
                    .update(mcp_servers::Message::Import(asked), settings.as_ref());
                return Task::batch([task, self.float_adopt(&run, None)]);
            }
            Message::Skills(message) => return self.skills.update(message, self.settings.as_ref()),
            Message::Mcp(message) => {
                return self.mcp_servers.update(message, self.settings.as_ref());
            }
            Message::Usage(message) => return self.usage.update(message, self.settings.as_ref()),
            Message::Retro(message) => return self.retros.update(message, self.settings.as_ref()),
            Message::Rules(rules::Message::Asked(Ok(run))) => {
                // The Moebius agent changes the rules in a chat, which the floating chat shows.
                let settings = self.settings.clone();
                let task = self
                    .rules
                    .update(rules::Message::Asked(Ok(run.clone())), settings.as_ref());
                return Task::batch([task, self.float_adopt(&run, None)]);
            }
            Message::Rules(message) => return self.rules.update(message, self.settings.as_ref()),
            Message::Experiments(message) => {
                return self
                    .experiments
                    .update(message, self.settings.as_ref(), &self.snapshot);
            }
            Message::DecisionModel(message) => {
                return self.decision_model.update(message, self.settings.as_ref());
            }
            Message::Setup(message) => return self.setup.update(message, self.settings.as_ref()),
            Message::Harness(message) => {
                // Once the repository is made, the server restarts to use it, as after a change of the settings.
                let succeeded = HarnessSetup::succeeded(&message);
                let task = self.harness.update(message, self.settings.as_ref());
                if succeeded {
                    let restart = self
                        .setup
                        .update(setup::Message::Restart, self.settings.as_ref());
                    return Task::batch([task, restart]);
                }
                return task;
            }
            Message::Terminal(message) => self.terminal.update(message),
            Message::Ports(message) => return self.on_ports(message),
            Message::OpenPorts(scope) => return self.open_ports(scope),
            Message::Start(message) => {
                let first = matches!(message, start::Message::Loaded(_));
                let task = self.start.update(message, self.settings.as_ref());
                let opening = first && std::mem::take(&mut self.open_on_start);
                if opening && self.start.incomplete() && self.page.is_none() {
                    self.page = Some(Page::Start);
                }
                return task;
            }
            Message::Repositories(message) => {
                // The list is read again once a repository is gone, without waiting for the event.
                let removed = matches!(message, repositories::Message::Removed(_, Ok(_)));
                let task = self.repositories.update(message, self.settings.as_ref());
                if removed {
                    let start = match &self.settings {
                        Some(settings) => self.start.reload(settings),
                        None => Task::none(),
                    };
                    return Task::batch([task, self.reload(), start]);
                }
                return task;
            }
            Message::AddRepository(message) => {
                // The new board opens, and the snapshot is read again without waiting for the event.
                let added = match &message {
                    add_repository::Message::Answered(Ok(repository)) => Some(repository.id),
                    _ => None,
                };
                let task = self.add_repository.update(message, self.settings.as_ref());
                if let Some(id) = added {
                    self.page = Some(Page::Board(id));
                    self.opened = None;
                    let start = match &self.settings {
                        Some(settings) => self.start.reload(settings),
                        None => Task::none(),
                    };
                    return Task::batch([task, self.reload(), start]);
                }
                return task;
            }
            Message::NewIssue(new_issue::Message::Asked(Ok(run))) => {
                // The agent writes the issue in a chat in the board's repository, which the floating chat shows.
                let task = self
                    .new_issue
                    .update(new_issue::Message::Asked(Ok(run.clone())), None);
                return Task::batch([task, self.float_adopt(&run, self.page_repository())]);
            }
            Message::NewIssue(message) => {
                let created = matches!(message, new_issue::Message::Answered(Ok(_)));
                let task = self.new_issue.update(message, self.settings.as_ref());
                if created {
                    return Task::batch([task, self.reload()]);
                }
                return task;
            }
            Message::Chat(message) => {
                // A chat the project's chat tab started, or went on in a new one, is the tab's.
                let started = match &message {
                    chat::Message::Started(Ok(run)) | chat::Message::Continued(Ok(run)) => {
                        Some(run.id)
                    }
                    _ => None,
                };
                if let (Some(chat), Page::Board(repository)) = (started, self.page())
                    && let Some(project) = self.projects.get_mut(&repository)
                {
                    project.follow_chat(chat);
                }
                return self.chats.update(message, self.settings.as_ref());
            }
            Message::Float(message) => return self.on_float(message),
            Message::Fleet(message) => self.fleet.update(message),
            Message::Automations(message) => {
                return self.automations.update(message, self.settings.as_ref());
            }
            Message::OpenChat(id) => {
                self.page = Some(Page::Chat);
                self.opened = None;
                if let Some(settings) = &self.settings {
                    return self.chats.open(id, settings);
                }
            }
            Message::History(id, Ok(entries)) => {
                if let Some(Opened::History {
                    card,
                    entries: shown,
                    ..
                }) = &mut self.opened
                    && *card == id
                {
                    *shown = Some(entries);
                }
            }
            Message::History(_, Err(_)) => {}
            Message::Search(message) => return self.on_search(message),
            Message::ToggleSidebar => self.collapsed = !self.collapsed,
            Message::Ssh(message) => return self.on_ssh(message),
            Message::SshAskpass(host, start, askpass) => {
                return self.ssh_ready(host, start, askpass);
            }
            Message::SshSetup(event) => return self.on_setup(event),
            Message::Tunnel(event) => return self.on_tunnel(event),
            Message::ToggleServers => self.ssh.servers_open = !self.ssh.servers_open,
            Message::SwitchServer(index) => return self.switch_server(index),
            Message::ChangeConnection => {
                self.form.error = None;
                self.form.token.clear();
                self.screen = Screen::Connect;
            }
        }
        Task::none()
    }

    /// Something on the search field or its list.
    pub(super) fn on_search(&mut self, message: search::Message) -> Task<Message> {
        match message {
            search::Message::Edited(text) => {
                let opening = !self.search.is_open();
                self.search.text = text;
                self.search.selected = 0;
                self.search.notice = None;
                // The agents are listed only when their page is opened; the search needs them.
                if opening
                    && self.search.is_open()
                    && let Some(settings) = &self.settings
                {
                    return self.agents.reload(settings);
                }
            }
            search::Message::Move(direction) => {
                let count = self.search_entries().len();
                self.search.step(direction, count);
            }
            search::Message::Submit => {
                let selected = self.search.selected;
                return self.open_entry(selected);
            }
            search::Message::Pick(index) => return self.open_entry(index),
            // Escape closes the list, and otherwise goes back from an editor.
            search::Message::Close if self.search.is_open() => self.search.clear(),
            search::Message::Close => return self.escape(),
            search::Message::Focus => {
                self.collapsed = false;
                return iced::widget::operation::focus(search::FIELD);
            }
            search::Message::Started(_, Ok(run)) => {
                self.page = Some(Page::Chat);
                self.opened = None;
                return self
                    .chats
                    .update(chat::Message::Started(Ok(run)), self.settings.as_ref());
            }
            search::Message::Started(text, Err(ClientError::Unauthorized)) => {
                self.search.text = text;
                self.show_connect_error(&ClientError::Unauthorized);
            }
            search::Message::Started(text, Err(e)) => {
                self.search.text = text;
                self.search.notice = Some(live::say(&e));
            }
        }
        Task::none()
    }

    /// The entries the search matches now.
    pub(super) fn search_entries(&self) -> Vec<search::Entry> {
        search::matches(
            &self.search.text,
            &self.snapshot,
            self.chats.list(),
            self.agents.list(),
        )
    }

    /// Opens the entry at `index` of the list, and clears the field.
    pub(super) fn open_entry(&mut self, index: usize) -> Task<Message> {
        let Some(entry) = self.search_entries().into_iter().nth(index) else {
            return Task::none();
        };
        self.search.clear();
        match entry.target {
            Target::Card(id) => self.open(id),
            Target::Chat(id) => self.handle(Message::OpenChat(id)),
            Target::Agent(name) => Task::batch([
                self.handle(Message::Visit(Page::Agents)),
                self.handle(Message::Agents(agents::Message::Select(name))),
            ]),
            Target::Page(page) => self.handle(Message::Visit(page)),
            Target::Ask(text) => match &self.settings {
                Some(settings) => search::ask(settings, text),
                None => Task::none(),
            },
        }
    }

    /// Something on the inbox page. The snapshot is read again when a request succeeded, without waiting for the event.
    pub(super) fn on_inbox(&mut self, message: inbox::Message) -> Task<Message> {
        match &message {
            inbox::Message::Done(_, Err(ClientError::Unauthorized)) => {
                self.show_connect_error(&ClientError::Unauthorized);
                return Task::none();
            }
            inbox::Message::Done(_, Ok(())) => {
                let task = self.inbox.update(message, self.settings.as_ref());
                return Task::batch([task, self.reload()]);
            }
            _ => {}
        }
        self.inbox.update(message, self.settings.as_ref())
    }

    /// Something on the floating chat. Sending adds where the person is, and the repository of the page when the
    /// message starts the chat.
    pub(super) fn on_float(&mut self, message: float_chat::Message) -> Task<Message> {
        match message {
            float_chat::Message::Send => {
                let Some(settings) = self.settings.clone() else {
                    return Task::none();
                };
                let (context, repository) = (self.page_context(), self.page_repository());
                self.float.send(&settings, context, repository)
            }
            float_chat::Message::OpenInChat => {
                let Some(id) = self.float.chat() else {
                    return Task::none();
                };
                // The window gives way to the Chat page, which shows the same chat.
                let _ = self.float.update(float_chat::Message::Show(false), None);
                self.handle(Message::OpenChat(id))
            }
            float_chat::Message::Started(Err(ClientError::Unauthorized))
            | float_chat::Message::Sent(Err(ClientError::Unauthorized)) => {
                self.show_connect_error(&ClientError::Unauthorized);
                Task::none()
            }
            message => {
                // A chat that started is in the Chat page's list too.
                let started = matches!(message, float_chat::Message::Started(Ok(_)));
                let task = self.float.update(message, self.settings.as_ref());
                match &self.settings {
                    Some(settings) if started => Task::batch([task, self.chats.reload(settings)]),
                    _ => task,
                }
            }
        }
    }

    /// Opens the floating chat on `run`, a chat with the Moebius agent started by an "Ask the agent", in `repository`.
    pub(super) fn float_adopt(
        &mut self,
        run: &client::Run,
        repository: Option<i64>,
    ) -> Task<Message> {
        let Some(settings) = self.settings.clone() else {
            return Task::none();
        };
        Task::batch([
            self.float.adopt(run, repository, &settings),
            self.chats.reload(&settings),
        ])
    }

    /// Tries what was typed; an empty token keeps the saved one.
    pub(super) fn submit(&mut self) -> Task<Message> {
        let address = settings::normalize_address(&self.form.address);
        let token = match (&self.form.token, &self.settings) {
            (typed, _) if !typed.is_empty() => typed.clone(),
            (_, Some(saved)) => saved.token.clone(),
            _ => String::new(),
        };
        if address == "http://" || token.is_empty() {
            self.form.error = Some("Enter the address and the token.".to_string());
            return Task::none();
        }
        self.form.error = None;
        self.form.connecting = true;
        Task::perform(check(Settings { address, token }), Message::Checked)
    }

    /// The server took the settings: save them if they are new and show the shell.
    pub(super) fn adopt(&mut self, settings: Settings, snapshot: Snapshot) -> Task<Message> {
        if self.ssh.session.is_some() || self.settings.as_ref() != Some(&settings) {
            let saved = if self.ssh.session.is_some() {
                self.ssh_adopted()
            } else {
                match &self.settings_path {
                    Some(path) => settings::save(path, &settings).map(|()| self.refresh_servers()),
                    None => Ok(()),
                }
            };
            if let Err(e) = saved {
                self.form.connecting = false;
                self.form.error = Some(e.to_string());
                self.screen = Screen::Connect;
                return Task::none();
            }
        }
        self.form = Form {
            address: settings.address.clone(),
            ..Form::default()
        };
        self.settings = Some(settings);
        self.snapshot = snapshot;
        self.inbox.follow(&self.snapshot);
        self.screen = Screen::Main;
        // The server just answered. The event stream may already be open, and it does not say so again.
        self.connection = Connection::Online;
        self.open_on_start = true;
        let ports = self.connect_ports();
        match &self.settings {
            Some(settings) => {
                Task::batch([self.start.reload(settings), ports, self.report_client()])
            }
            None => Task::none(),
        }
    }

    pub(super) fn show_connect_error(&mut self, error: &ClientError) {
        self.form.connecting = false;
        let message = match error {
            ClientError::Unauthorized => "The token is not accepted.".to_string(),
            ClientError::Unreachable(address) => format!("Cannot reach {address}."),
            other => other.to_string(),
        };
        // Over SSH the steps show where it failed.
        if self.ssh.session.is_some() {
            self.ssh.connect.panel = true;
            self.ssh
                .connect
                .on_setup(crate::ssh::remote::Event::Failed(message.clone()));
        }
        self.form.error = Some(message);
        self.screen = Screen::Connect;
    }

    /// Opens a card's page: the live view of its latest run, or its history.
    pub(super) fn open(&mut self, id: i64) -> Task<Message> {
        let Some(settings) = self.settings.clone() else {
            return Task::none();
        };
        let Some(card) = self
            .snapshot
            .boards
            .iter()
            .flat_map(|board| &board.cards)
            .find(|card| card.id == id)
        else {
            return Task::none();
        };
        // A card is opened on the page of its board, also when the click was in the inbox.
        self.page = Some(Page::Board(card.repository_id));
        self.projects
            .entry(card.repository_id)
            .or_default()
            .show_tab(Tab::Board);
        match card.run.clone() {
            Some(run) => {
                let task = live::load(settings, id, run.id, 0);
                self.opened = Some(Opened::Run(Box::new(Live::new(id, run))));
                task
            }
            None => {
                self.opened = Some(Opened::History {
                    card: id,
                    entries: None,
                    now: live::now(),
                });
                Task::perform(
                    async move { Client::new(settings).history(id).await },
                    move |entries| Message::History(id, entries),
                )
            }
        }
    }

    /// Opens `page` as the sidebar does, with what was last open on it.
    pub(super) fn show_page(&mut self, page: Page) -> Task<Message> {
        self.page = Some(page);
        self.opened = None;
        self.edits.close();
        self.chats.leave();
        if page == Page::Terminal {
            self.terminal.open();
        }
        match (page, &self.settings) {
            (Page::Board(id), Some(settings)) => {
                self.projects.entry(id).or_default().enter(id, settings)
            }
            (Page::Chat | Page::Fleet, Some(settings)) => self.chats.reload(settings),
            (Page::Automations, Some(settings)) => self.automations.reload(settings),
            (Page::Agents, Some(settings)) => self.agents.reload(settings),
            (Page::Skills, Some(settings)) => self.skills.reload(settings),
            (Page::McpServers, Some(settings)) => self.mcp_servers.reload(settings),
            (Page::Usage, Some(settings)) => self.usage.reload(settings),
            (Page::Retro, Some(settings)) => self.retros.reload(settings),
            (Page::Rules, Some(settings)) => self.rules.reload(settings),
            (Page::Experiments, Some(settings)) => self.experiments.reload(settings),
            (Page::DecisionModel, Some(settings)) => self.decision_model.reload(settings),
            (Page::Ports, Some(settings)) => self.ports.reload(settings),
            (Page::Models | Page::Plugins | Page::Settings, Some(settings)) => {
                self.setup.reload(settings)
            }
            _ => Task::none(),
        }
    }

    /// Reads the snapshot again.
    pub(super) fn reload(&mut self) -> Task<Message> {
        let Some(settings) = self.settings.clone() else {
            return Task::none();
        };
        self.requested += 1;
        let count = self.requested;
        Task::perform(
            async move { Client::new(settings).snapshot().await },
            move |result| Message::Loaded(count, result),
        )
    }
}

/// Whether `message` is the server refusing the token, on any page: the connect screen opens again.
fn refused(message: &Message) -> bool {
    match message {
        Message::Edit(message) => Edits::refused(message),
        Message::Project(_, message) => Project::refused(message),
        Message::Script(message) => ScriptEditor::refused(message),
        Message::Chat(chat::Message::Listed(result)) => client::unauthorized(result),
        Message::Agents(message) => Agents::refused(message),
        Message::Automations(message) => Automations::refused(message),
        Message::Skills(message) => Skills::refused(message),
        Message::Mcp(message) => McpServers::refused(message),
        Message::Usage(message) => Usage::refused(message),
        Message::Retro(message) => Retros::refused(message),
        Message::Rules(message) => Rules::refused(message),
        Message::Experiments(message) => Experiments::refused(message),
        Message::DecisionModel(message) => DecisionModel::refused(message),
        Message::Setup(message) => Setup::refused(message),
        Message::Harness(message) => HarnessSetup::refused(message),
        Message::Start(message) => Start::refused(message),
        Message::Repositories(message) => Repositories::refused(message),
        Message::AddRepository(message) => AddRepository::refused(message),
        Message::NewIssue(message) => NewIssue::refused(message),
        _ => false,
    }
}
