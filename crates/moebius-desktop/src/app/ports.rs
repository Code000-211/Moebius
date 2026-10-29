//! The app's part of the Ports page: what the stream says of ports opening and closing, which of them are being
//! watched (so that one is offered to be opened), the forwards remembered for each server in the settings file and
//! restored at the next connection, and the page opened from the live view, a chat and the Terminal page.

use iced::Task;

use crate::client::{Port, ServerEvent};
use crate::screens::ports::{self, Scope};
use crate::settings;

use super::message::{Message, Page};
use super::{App, Opened};

impl App {
    /// Whether `port` belongs to what the person is looking at: the run on the live view, the chat open on the Chat
    /// page, or the server's terminal on the Terminal page.
    fn watching(&self, port: &Port) -> bool {
        match self.page() {
            Page::Terminal => port.kind == "terminal" && port.run.is_none(),
            Page::Chat => port.run.is_some() && port.run == self.chats.opened(),
            Page::Board(_) => match &self.opened {
                Some(Opened::Run(live)) => port.run == Some(live.run_id()),
                _ => false,
            },
            _ => false,
        }
    }

    /// A `port_opened` or `port_closed` event of the stream.
    pub(super) fn on_port_event(&mut self, event: &ServerEvent) {
        let mut ports = std::mem::take(&mut self.ports);
        ports.on_event(&event.kind, &event.data, |port| self.watching(port));
        self.ports = ports;
        self.remember_ports();
    }

    /// Something on the Ports page.
    pub(super) fn on_ports(&mut self, message: ports::Message) -> Task<Message> {
        let task = self.ports.update(message);
        self.remember_ports();
        task
    }

    /// Opens the Ports page showing the ports of `scope`, or all of them, as a page "Back" returns from.
    pub(super) fn open_ports(&mut self, scope: Option<Scope>) -> Task<Message> {
        self.ports.show(scope);
        self.show_page(Page::Ports)
    }

    /// A server was connected: its remembered forwards start again, if the setting allows, and the ports it has open
    /// now are read.
    pub(super) fn connect_ports(&mut self) -> Task<Message> {
        let Some(settings) = self.settings.clone() else {
            return Task::none();
        };
        let host = self
            .ssh
            .session
            .as_ref()
            .map(|session| session.host.as_str());
        let (remembered, restore) = match &self.settings_path {
            Some(path) => (
                settings::forwards(path, host, &settings.address),
                settings::load_all(path).is_ok_and(|stored| stored.restores_forwards()),
            ),
            None => (Vec::new(), true),
        };
        self.ports.connected(remembered, restore);
        self.ports.reload(&settings)
    }

    /// Writes the forwards and the setting to the settings file when they changed.
    fn remember_ports(&mut self) {
        if !self.ports.take_changed() {
            return;
        }
        let (Some(path), Some(settings)) = (&self.settings_path, &self.settings) else {
            return;
        };
        let host = self
            .ssh
            .session
            .as_ref()
            .map(|session| session.host.as_str());
        let restore = self.ports.restores();
        let _ = settings::update(path, |stored| stored.restore_forwards = Some(restore));
        let _ = settings::save_forwards(path, host, &settings.address, &self.ports.remembered());
    }
}
