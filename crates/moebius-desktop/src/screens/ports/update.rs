//! How a message changes the Ports page, and what it asks of the system: opening the browser, copying an address.

use iced::Task;

use crate::app::Message as AppMessage;
use crate::client::Forwarding;
use crate::widgets::browser;

use super::{Message, Ports, State};

/// A port number as typed: 1 to 65535.
fn parse(typed: &str) -> Option<u16> {
    typed.trim().parse::<u16>().ok().filter(|port| *port != 0)
}

impl Ports {
    pub fn update(&mut self, message: Message) -> Task<AppMessage> {
        match message {
            Message::Listed(Ok(ports)) => self.listed(ports),
            Message::Listed(Err(_)) => {}
            Message::Forward(event) => {
                let Some(forward) = self.forwards.iter_mut().find(|f| f.remote == event.remote)
                else {
                    return Task::none();
                };
                match event.what {
                    Forwarding::Listening(local) => {
                        forward.state = State::Listening;
                        forward.local = local;
                        forward.refused = None;
                    }
                    Forwarding::Failed(reason) => forward.state = State::Failed(reason),
                    Forwarding::Refused(reason) => forward.refused = Some(reason),
                }
            }
            Message::OpenForm => {
                self.form = Some(String::new());
                self.note = None;
            }
            Message::FormEdited(typed) => self.form = Some(typed),
            Message::FormCancel => {
                self.form = None;
                self.note = None;
            }
            Message::FormSubmit => {
                let Some(typed) = self.form.clone() else {
                    return Task::none();
                };
                match parse(&typed) {
                    Some(remote) => {
                        self.stopped.remove(&remote);
                        self.forward(remote, true);
                        self.form = None;
                        self.note = None;
                    }
                    None => self.note = Some("Enter a port from 1 to 65535.".to_string()),
                }
            }
            Message::Start(remote) => {
                self.stopped.remove(&remote);
                self.forward(remote, false);
            }
            Message::Open(remote) => {
                if let Some(address) = self.address(remote) {
                    browser::open(&format!("http://{address}"));
                }
            }
            Message::Copy(remote) => {
                if let Some(address) = self.address(remote) {
                    return iced::clipboard::write(address);
                }
            }
            Message::Change(remote) => {
                if let Some(forward) = self.forwards.iter().find(|f| f.remote == remote) {
                    self.editing = Some((remote, forward.local.to_string()));
                    self.note = None;
                }
            }
            Message::LocalEdited(typed) => {
                if let Some((_, text)) = &mut self.editing {
                    *text = typed;
                }
            }
            Message::LocalCancel => {
                self.editing = None;
                self.note = None;
            }
            Message::LocalSubmit => {
                let Some((remote, typed)) = self.editing.clone() else {
                    return Task::none();
                };
                let taken: Vec<u16> = self
                    .forwards
                    .iter()
                    .filter(|f| f.remote != remote)
                    .map(|f| f.local)
                    .collect();
                match parse(&typed) {
                    None => self.note = Some("Enter a port from 1 to 65535.".to_string()),
                    Some(local) if taken.contains(&local) => {
                        self.note = Some(format!("Port {local} is forwarded already."));
                    }
                    Some(local) => {
                        if let Some(forward) = self.forwards.iter_mut().find(|f| f.remote == remote)
                        {
                            if forward.local != local {
                                forward.local = local;
                                forward.state = State::Starting;
                                forward.refused = None;
                            }
                            forward.by_hand = true;
                            self.changed = true;
                        }
                        self.editing = None;
                        self.note = None;
                    }
                }
            }
            Message::Stop(remote) => {
                let before = self.forwards.len();
                self.forwards.retain(|forward| forward.remote != remote);
                self.changed |= self.forwards.len() != before;
                if self.detected.iter().any(|port| port.port == remote) {
                    self.stopped.insert(remote);
                }
                self.offers.retain(|offer| offer.port != remote);
            }
            Message::ShowAll => self.scope = None,
            Message::ToggleRestore => {
                self.restore = !self.restore;
                self.changed = true;
            }
            Message::OpenOffer => {
                if !self.offers.is_empty() {
                    let offer = self.offers.remove(0);
                    if let Some(address) = self.address(offer.port) {
                        browser::open(&format!("http://{address}"));
                    }
                }
            }
            Message::DismissOffer => {
                if !self.offers.is_empty() {
                    self.offers.remove(0);
                }
            }
        }
        Task::none()
    }
}
