//! The connect screen's way over SSH and the sidebar's header: the list of hosts of the SSH configuration, adding a host,
//! the steps of the setup with a question of `ssh` and the log, the offer to install, and the tunnel that is down.

use moebius_desktop::app::{App, Message};
use moebius_desktop::screens::connect_ssh;
use moebius_desktop::settings::Settings;
use moebius_desktop::ssh::remote::{Event, Install, Start, Step};
use moebius_desktop::ssh::tunnel::TunnelEvent;

use crate::fixtures::{click, clicked, draw, overview, shows, update_all};

fn app_with_hosts() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    std::fs::write(
        &config,
        "Host *\n  ServerAliveInterval 30\nHost build\n  HostName 10.0.0.7\n  User deploy\nHost gpu-box\n  HostName gpu.example\n  Port 2222\n",
    )
    .unwrap();
    let (app, _) = App::start(None);
    (app.with_ssh_config(config), dir)
}

fn setup(app: &mut App, event: Event) {
    let _ = app.update(Message::SshSetup(event));
}

/// The list of hosts: compare `tests/snapshots/connect-ssh-hosts-tiny-skia.png`.
#[test]
fn the_connect_screen_lists_the_hosts_of_the_ssh_configuration() {
    let (mut app, _dir) = app_with_hosts();
    assert!(shows(&app, "Connect over SSH"));
    click(&mut app, "Connect over SSH");
    draw(
        &app,
        "connect-ssh-hosts",
        &[
            "build",
            "deploy@10.0.0.7",
            "gpu-box",
            "gpu.example:2222",
            "Add new SSH host",
            "Connect by address",
        ],
    );
    // The wildcard entry is not a host to pick.
    assert!(!shows(&app, "ServerAliveInterval"));
    click(&mut app, "Connect by address");
    assert!(shows(&app, "Connect to a server"));
}

#[test]
fn a_new_host_is_written_to_the_configuration_and_connected_to() {
    let (mut app, dir) = app_with_hosts();
    click(&mut app, "Connect over SSH");
    click(&mut app, "Add new SSH host");
    for message in [
        connect_ssh::Message::Typed("ssh -p 2200 leon@new.example".into()),
        connect_ssh::Message::Add,
    ] {
        let _ = app.update(Message::Ssh(message));
    }
    let written = std::fs::read_to_string(dir.path().join("config")).unwrap();
    assert!(
        written.ends_with(
            "Host new.example\n    HostName new.example\n    User leon\n    Port 2200\n"
        ),
        "{written}"
    );
    assert!(shows(&app, "Connecting to new.example"));

    // A name that is there already is refused with the reason.
    let (mut app, _dir) = app_with_hosts();
    click(&mut app, "Connect over SSH");
    click(&mut app, "Add new SSH host");
    for message in [
        connect_ssh::Message::Typed("build".into()),
        connect_ssh::Message::Add,
    ] {
        let _ = app.update(Message::Ssh(message));
    }
    assert!(shows(&app, "build is already in the configuration"));
}

/// The steps with a question of `ssh` and the log open: compare `tests/snapshots/connect-ssh-progress-tiny-skia.png`.
#[test]
fn the_steps_show_where_the_setup_is_with_the_question_of_ssh_and_the_log() {
    let (mut app, _dir) = app_with_hosts();
    click(&mut app, "Connect over SSH");
    click(&mut app, "build");
    for event in [
        Event::Log("$ uname -sm".into()),
        Event::Log("Linux x86_64".into()),
        Event::Step(Step::Find),
        Event::Log("$ command -v moebius".into()),
        Event::Step(Step::Configure),
        Event::Prompt("deploy@10.0.0.7's password:".into()),
    ] {
        setup(&mut app, event);
    }
    click(&mut app, "Show log");
    draw(
        &app,
        "connect-ssh-progress",
        &[
            "Connecting to build",
            "Connect and detect the system",
            "Set up the configuration",
            "deploy@10.0.0.7's password:",
            "Answer",
            "Hide log",
            "$ command -v moebius",
        ],
    );
    // The password is typed into a field that hides it, and goes to ssh, not to the log.
    let _ = app.update(Message::Ssh(connect_ssh::Message::AnswerEdited(
        "hunter2".into(),
    )));
    let _ = app.update(Message::Ssh(connect_ssh::Message::Answer));
    assert!(!shows(&app, "deploy@10.0.0.7's password:"));
    assert!(!shows(&app, "hunter2"));
}

/// The offer to install: compare `tests/snapshots/connect-ssh-offer-tiny-skia.png`.
#[test]
fn a_host_without_moebius_is_offered_the_install_in_two_ways() {
    let (mut app, _dir) = app_with_hosts();
    click(&mut app, "Connect over SSH");
    click(&mut app, "gpu-box");
    setup(
        &mut app,
        Event::Missing {
            system: "Linux aarch64".into(),
            url: "https://github.com/Code000-211/Moebius/releases/download/v0.1.0/moebius-aarch64-unknown-linux-gnu".into(),
        },
    );
    draw(
        &app,
        "connect-ssh-offer",
        &[
            "Moebius is not installed on gpu-box",
            "Install on the host",
            "Download here and upload",
        ],
    );
    let sent = clicked(&app, "Download here and upload");
    assert!(matches!(
        &sent[..],
        [Message::Ssh(connect_ssh::Message::Install(Install::Upload))]
    ));
    let mut app = app;
    update_all(&mut app, sent);
    assert!(shows(&app, "Install Moebius"));
}

#[test]
fn a_failed_setup_names_the_step_and_can_be_tried_again() {
    let (mut app, _dir) = app_with_hosts();
    click(&mut app, "Connect over SSH");
    click(&mut app, "build");
    setup(&mut app, Event::Step(Step::Start));
    setup(
        &mut app,
        Event::Failed("cannot start the server: no space left".into()),
    );
    for text in [
        "failed",
        "Start the server",
        "cannot start the server: no space left",
        "Try again",
    ] {
        assert!(shows(&app, text), "{text}");
    }
}

fn over_ssh() -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Ssh(connect_ssh::Message::Pick("build".into())));
    let _ = app.update(Message::SshAskpass(
        "build".into(),
        Start::Check,
        Err("not needed".into()),
    ));
    setup(
        &mut app,
        Event::Ready {
            token: "t".repeat(32),
            port: 50123,
        },
    );
    let settings = Settings {
        address: "http://127.0.0.1:50123".into(),
        token: "t".repeat(32),
    };
    let _ = app.update(Message::Checked(Ok((settings, overview()))));
    app
}

/// The header names the host and says the tunnel is made again: compare
/// `tests/snapshots/sidebar-reconnecting-tiny-skia.png`.
#[test]
fn the_header_names_the_host_and_says_reconnecting_while_the_tunnel_is_down() {
    let mut app = over_ssh();
    // The tunnel has not reported itself up yet, so it counts as down.
    draw(&app, "sidebar-reconnecting", &["build", "Reconnecting…"]);
    let _ = app.update(Message::Tunnel(TunnelEvent::Up));
    assert!(shows(&app, "build"));
    assert!(shows(&app, "Online"));
    assert!(!shows(&app, "Reconnecting…"));
    let _ = app.update(Message::Tunnel(TunnelEvent::Down(
        "Connection reset".into(),
    )));
    assert!(shows(&app, "Reconnecting…"));
}

#[test]
fn the_header_lists_the_servers_remembered_and_switches_between_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("desktop.toml");
    std::fs::write(
        &path,
        "address = \"http://10.0.0.5:7171\"\ntoken = \"0123456789abcdef0123456789abcdef\"\n\
         [[servers]]\naddress = \"http://10.0.0.5:7171\"\ntoken = \"0123456789abcdef0123456789abcdef\"\n\
         [[servers]]\nhost = \"build\"\n",
    )
    .unwrap();
    let (mut app, _) = App::start(Some(path));
    let _ = app.update(Message::Checked(Ok((
        Settings {
            address: "http://10.0.0.5:7171".into(),
            token: "0123456789abcdef0123456789abcdef".into(),
        },
        overview(),
    ))));
    assert!(shows(&app, "10.0.0.5:7171"));
    click(&mut app, "10.0.0.5:7171");
    assert!(shows(&app, "build"));
    assert!(shows(&app, "Add a server…"));
    let sent = clicked(&app, "build");
    assert!(matches!(&sent[..], [Message::SwitchServer(1)]));
    update_all(&mut app, sent);
    assert!(shows(&app, "Connecting to build"));
}

/// A connection over SSH that was begun and not finished does not take the place of the address the person then
/// connects to: the address and its token are saved, and the app opens them again at its next start.
#[test]
fn connecting_by_address_after_an_unfinished_ssh_connection_saves_the_address() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("desktop.toml");
    std::fs::write(
        &path,
        "last_ssh = \"build\"\n[[servers]]\nhost = \"build\"\n",
    )
    .unwrap();
    let (mut app, _) = App::start(Some(path.clone()));
    let _ = app.update(Message::SshAskpass(
        "build".into(),
        Start::Check,
        Err("no askpass here".into()),
    ));
    let saved = Settings {
        address: "http://10.0.0.5:7171".into(),
        token: "0123456789abcdef0123456789abcdef".into(),
    };
    let _ = app.update(Message::AddressChanged(saved.address.clone()));
    let _ = app.update(Message::TokenChanged(saved.token.clone()));
    let _ = app.update(Message::Submit);
    let _ = app.update(Message::Checked(Ok((saved.clone(), overview()))));

    assert_eq!(moebius_desktop::settings::load(&path).unwrap(), Some(saved));
    let stored = moebius_desktop::settings::load_all(&path).unwrap();
    assert_eq!(stored.last_ssh, None);
}
