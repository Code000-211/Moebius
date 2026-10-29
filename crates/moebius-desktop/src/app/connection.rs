//! The app's part of connecting over SSH: the host list and the setup of a host on the connect screen, the tunnel that
//! carries the connection once it is made, and the servers remembered, which the sidebar's header switches between.

use std::path::PathBuf;

use iced::{Subscription, Task};

use crate::screens::connect_ssh::{self, Phase, SshConnect};
use crate::settings::{self, Server, Settings};
use crate::ssh::Ssh;
use crate::ssh::askpass::Askpass;
use crate::ssh::config;
use crate::ssh::remote::{self, Event, Plan, REMOTE_PORT, Start, Step};
use crate::ssh::tunnel::{self, Tunnel, TunnelEvent};

use super::message::Message;
use super::{App, Connection, Form, Screen, check};

/// The SSH connection in use or being made.
pub(super) struct Session {
    pub host: String,
    askpass: Option<Askpass>,
    /// Set when the setup has read the token; the tunnel is held for as long as it is.
    tunnel: Option<Tunnel>,
    up: bool,
    /// What the server is to be tried with once the tunnel is up.
    pending: Option<Settings>,
    /// Ends the setup when the session is dropped.
    _setup: Option<iced::task::Handle>,
}

/// What the app keeps for SSH besides the screen's state.
pub(super) struct SshState {
    pub connect: SshConnect,
    pub session: Option<Session>,
    /// The `ssh` to run.
    program: String,
    /// The configuration file a new host is written to.
    config: Option<PathBuf>,
    /// The other configuration files whose hosts are listed.
    more_config: Option<PathBuf>,
    /// The servers connected to before.
    pub servers: Vec<Server>,
    pub servers_open: bool,
}

impl SshState {
    pub fn new(stored: &settings::Stored) -> Self {
        SshState {
            connect: SshConnect::default(),
            session: None,
            program: stored.ssh_program.clone().unwrap_or_else(|| "ssh".into()),
            config: config::default_path(),
            more_config: stored.ssh_config.clone(),
            servers: stored.servers.clone(),
            servers_open: false,
        }
    }

    pub fn use_config(&mut self, file: PathBuf) {
        self.config = None;
        self.more_config = Some(file);
    }

    fn files(&self) -> Vec<PathBuf> {
        self.config
            .iter()
            .chain(self.more_config.iter())
            .cloned()
            .collect()
    }

    /// Where a new host is written: the file named in the settings, else `~/.ssh/config`.
    fn write_to(&self) -> Option<PathBuf> {
        self.more_config.clone().or_else(|| self.config.clone())
    }

    fn ssh(&self, askpass: Option<Askpass>) -> Ssh {
        let ssh = Ssh::new(&self.program);
        match askpass {
            Some(askpass) => ssh.with_askpass(askpass),
            None => ssh,
        }
    }
}

impl App {
    /// The name the sidebar's header shows: the SSH host, else the address; none for a single server by address, whose
    /// name says nothing the person does not know.
    pub(super) fn server_name(&self) -> Option<String> {
        match (&self.ssh.session, &self.settings) {
            (Some(session), _) => Some(session.host.clone()),
            (None, Some(settings)) if self.ssh.servers.len() > 1 => {
                Some(settings::display_address(&settings.address))
            }
            _ => None,
        }
    }

    /// The header's list of servers.
    pub(super) fn server_menu(&self) -> crate::sidebar::ServerMenu {
        crate::sidebar::ServerMenu {
            name: self.server_name(),
            servers: self.ssh.servers.iter().map(Server::name).collect(),
            open: self.ssh.servers_open,
        }
    }

    /// What the sidebar says about the connection: while the tunnel is down, that it is made again.
    pub(super) fn shown_connection(&self) -> Connection {
        match &self.ssh.session {
            Some(Session {
                tunnel: Some(_),
                up: false,
                ..
            }) if self.screen == Screen::Main => Connection::Reconnecting,
            _ => self.connection,
        }
    }

    /// The tunnel of the session, for as long as there is one.
    pub(super) fn tunnel_subscription(&self) -> Subscription<Message> {
        match self.ssh.session.as_ref().and_then(|s| s.tunnel.as_ref()) {
            Some(tunnel) => {
                Subscription::run_with(tunnel.clone(), tunnel::run).map(Message::Tunnel)
            }
            None => Subscription::none(),
        }
    }

    pub(super) fn on_ssh(&mut self, message: connect_ssh::Message) -> Task<Message> {
        use connect_ssh::Message as M;
        match message {
            M::Open => {
                self.ssh.connect.panel = true;
                self.ssh.connect.phase = Phase::Idle;
                self.list_hosts();
            }
            M::Close => self.ssh.connect.panel = false,
            M::Pick(host) => return self.begin_ssh(host, Start::Check),
            M::Retry(host) => return self.begin_ssh(host, Start::Check),
            M::Adding => self.ssh.connect.adding = true,
            M::Typed(typed) => self.ssh.connect.typed = typed,
            M::Add => {
                let Some(file) = self.ssh.write_to() else {
                    self.ssh.connect.add_error = Some("There is no SSH configuration file.".into());
                    return Task::none();
                };
                let typed = self.ssh.connect.typed.clone();
                if let Some(name) = self.ssh.connect.added(config::add_host(&file, &typed)) {
                    self.list_hosts();
                    return self.begin_ssh(name, Start::Check);
                }
            }
            M::Install(how) => {
                let Some(host) = self.ssh.connect.host().map(str::to_string) else {
                    return Task::none();
                };
                return self.begin_ssh(host, Start::Install(how));
            }
            M::Cancel => {
                self.ssh.session = None;
                self.ssh.connect.phase = Phase::Idle;
                self.ssh.connect.prompt = None;
                self.form.connecting = false;
            }
            M::ToggleLog => self.ssh.connect.show_log = !self.ssh.connect.show_log,
            M::AnswerEdited(answer) => {
                if let Some(prompt) = &mut self.ssh.connect.prompt {
                    prompt.answer = answer;
                }
            }
            M::Answer | M::CancelAnswer => {
                let answer = match (&message, self.ssh.connect.prompt.take()) {
                    (M::Answer, Some(prompt)) => Some(prompt.answer),
                    _ => None,
                };
                if let Some(askpass) = self.ssh.session.as_ref().and_then(|s| s.askpass.as_ref()) {
                    askpass.answer(answer);
                }
            }
        }
        Task::none()
    }

    fn list_hosts(&mut self) {
        let files = self.ssh.files();
        self.ssh.connect.list_hosts(&files, &self.ssh.servers);
    }

    /// Starts the setup of `host` on the connect screen: a fresh `ssh` that asks this app for passwords.
    pub(super) fn begin_ssh(&mut self, host: String, start: Start) -> Task<Message> {
        self.screen = Screen::Connect;
        self.ssh.connect.panel = true;
        if start == Start::Check {
            self.ssh.connect.begin(&host);
        } else {
            self.ssh.connect.phase = Phase::Running {
                host: host.clone(),
                step: Step::Install,
            };
        }
        self.form.error = None;
        // The old session, with its tunnel and its setup, ends here.
        self.ssh.session = None;
        let program = std::env::current_exe().unwrap_or_else(|_| "moebius-desktop".into());
        Task::perform(Askpass::start(program), move |started| {
            Message::SshAskpass(host.clone(), start, started.map_err(|e| e.to_string()))
        })
    }

    /// The listener for passwords is ready (or could not be made, and `ssh` then asks on its own terminal, if any).
    pub(super) fn ssh_ready(
        &mut self,
        host: String,
        start: Start,
        askpass: Result<Askpass, String>,
    ) -> Task<Message> {
        if self.ssh.connect.host() != Some(host.as_str()) {
            return Task::none();
        }
        let askpass = match askpass {
            Ok(askpass) => Some(askpass),
            Err(e) => {
                self.ssh
                    .connect
                    .on_setup(Event::Log(format!("Cannot ask for passwords here: {e}")));
                None
            }
        };
        let plan = Plan::new(self.ssh.ssh(askpass.clone()), &host);
        let (task, handle) = Task::run(remote::connect(plan, start), Message::SshSetup).abortable();
        self.ssh.session = Some(Session {
            host,
            askpass,
            tunnel: None,
            up: false,
            pending: None,
            _setup: Some(handle.abort_on_drop()),
        });
        task
    }

    /// What the setup of the host tells.
    pub(super) fn on_setup(&mut self, event: Event) -> Task<Message> {
        let ssh_for = |state: &SshState, askpass| state.ssh(askpass);
        if let (Event::Ready { token, port }, Some(session)) = (&event, &self.ssh.session) {
            let tunnel = Tunnel {
                ssh: ssh_for(&self.ssh, session.askpass.clone()),
                host: session.host.clone(),
                local_port: *port,
                remote_port: REMOTE_PORT,
            };
            let session = self.ssh.session.as_mut().expect("checked above");
            session.pending = Some(Settings {
                address: format!("http://127.0.0.1:{port}"),
                token: token.clone(),
            });
            session.tunnel = Some(tunnel);
            session.up = false;
        }
        self.ssh.connect.on_setup(event);
        Task::none()
    }

    /// What the tunnel tells, on the connect screen while the connection is made and in the shell afterwards.
    pub(super) fn on_tunnel(&mut self, event: TunnelEvent) -> Task<Message> {
        self.ssh.connect.on_tunnel(&event);
        let Some(session) = &mut self.ssh.session else {
            return Task::none();
        };
        match event {
            TunnelEvent::Up => {
                session.up = true;
                if let Some(settings) = session.pending.take() {
                    return Task::perform(check(settings), Message::Checked);
                }
                // The tunnel came back: the screen is the shell's again, and what was missed is read.
                if self.settings.is_some() {
                    self.screen = Screen::Main;
                    self.ssh.connect.phase = Phase::Idle;
                    self.connection = Connection::Connecting;
                    return self.reload();
                }
            }
            TunnelEvent::Down(_) => session.up = false,
            TunnelEvent::Prompt(_) => {
                // A login that needs a password again: the question is asked on the connect screen.
                self.ssh.connect.panel = true;
                self.ssh.connect.phase = Phase::Running {
                    host: session.host.clone(),
                    step: Step::Tunnel,
                };
                self.screen = Screen::Connect;
            }
        }
        Task::none()
    }

    /// The connection by SSH is made: remember the host instead of an address and a token.
    pub(super) fn ssh_adopted(&mut self) -> Result<(), settings::SettingsError> {
        self.ssh.connect.panel = false;
        self.ssh.connect.phase = Phase::Idle;
        self.ssh.connect.prompt = None;
        let (Some(session), Some(path)) = (&self.ssh.session, &self.settings_path) else {
            return Ok(());
        };
        settings::save_ssh(path, &session.host)?;
        self.refresh_servers();
        Ok(())
    }

    /// Reads the remembered servers again after the file changed.
    pub(super) fn refresh_servers(&mut self) {
        if let Some(path) = &self.settings_path
            && let Ok(stored) = settings::load_all(path)
        {
            self.ssh.servers = stored.servers;
        }
    }

    /// A pick in the sidebar's list of servers.
    pub(super) fn switch_server(&mut self, index: usize) -> Task<Message> {
        self.ssh.servers_open = false;
        let Some(server) = self.ssh.servers.get(index).cloned() else {
            return Task::none();
        };
        if let Some(host) = server.host {
            return self.begin_ssh(host, Start::Check);
        }
        let (Some(address), Some(token)) = (server.address, server.token) else {
            return Task::none();
        };
        self.ssh.session = None;
        self.form = Form {
            address: address.clone(),
            ..Form::default()
        };
        self.connection = Connection::Connecting;
        Task::perform(check(Settings { address, token }), Message::Checked)
    }
}
