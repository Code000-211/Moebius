//! The app's report of where it runs: its program, version, system, computer and the way it reaches the server. Sent to
//! the server when the connection is made and whenever the event stream connects again, so that the Moebius agent
//! knows where the app is. Never a token. A server that does not know the route, or an answer that fails, changes
//! nothing: the report is a courtesy.

use iced::Task;

use crate::client::{Client, ClientReport};
use crate::settings::Settings;

use super::App;
use super::message::Message;

/// The report for an app that reaches the server by `address`, over SSH through `ssh_host` if it does.
pub fn this_app(address: &str, ssh_host: Option<&str>) -> ClientReport {
    ClientReport {
        program: std::env::current_exe()
            .map_or_else(|_| String::new(), |p| p.display().to_string()),
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        host: host_name(),
        server_address: address.to_string(),
        ssh_host: ssh_host.map(str::to_string),
        reported_at: 0,
    }
}

/// The name of this computer, "unknown" when it cannot be found.
fn host_name() -> String {
    let from_env = ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok());
    from_env
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .or_else(|| {
            let out = std::process::Command::new("hostname").output().ok()?;
            String::from_utf8(out.stdout).ok()
        })
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// Sends `report` with `settings`.
async fn send(settings: Settings, report: ClientReport) -> Result<(), crate::client::ClientError> {
    Client::new(settings)
        .report_client(&report)
        .await
        .map(|_| ())
}

impl App {
    /// Reports this app to the server it is connected to.
    pub(super) fn report_client(&self) -> Task<Message> {
        let Some(settings) = self.settings.clone() else {
            return Task::none();
        };
        let ssh_host = self
            .ssh
            .session
            .as_ref()
            .map(|session| session.host.as_str());
        let report = this_app(&settings.address, ssh_host);
        Task::perform(send(settings, report), Message::Reported)
    }
}
