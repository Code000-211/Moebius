//! Connecting over SSH against a real server, with the fake `ssh` of `moebius-core` as the client: the "remote" is this
//! machine, with a home directory of its own. The setup finds `moebius` missing, installs a release (a script that
//! stands for it) by download and by upload, makes the configuration, starts it detached and reads its token; the tunnel
//! carries the desktop API to the server, comes back after a drop, and a password is asked through the app.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use moebius_desktop::client::Client;
use moebius_desktop::settings::Settings;
use moebius_desktop::ssh::Ssh;
use moebius_desktop::ssh::askpass::Askpass;
use moebius_desktop::ssh::remote::{self, Event, Install, Plan, Platform, Start};
use moebius_desktop::ssh::tunnel::{self, Tunnel, TunnelEvent};

use crate::fixtures;

/// A fake remote machine: its home directory, an `ssh` that runs there, and a release to install.
struct Remote {
    dir: tempfile::TempDir,
    ssh: PathBuf,
    release: String,
    drop_file: PathBuf,
}

impl Remote {
    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    /// `extra` is added to the environment of the fake `ssh`, as `NAME=value` words.
    fn new(token: &str, extra: &str) -> Remote {
        let dir = tempfile::tempdir().unwrap();
        let (home, bin, releases) = (
            dir.path().join("home"),
            dir.path().join("bin"),
            dir.path().join("releases"),
        );
        for path in [&home, &bin, &releases] {
            std::fs::create_dir_all(path).unwrap();
        }
        // No systemd here: the server must be started detached.
        script(&bin.join("systemctl"), "#!/bin/sh\nexit 1\n");
        let uname = std::process::Command::new("uname")
            .arg("-sm")
            .output()
            .unwrap();
        let platform = Platform::parse(&String::from_utf8_lossy(&uname.stdout)).unwrap();
        let target = platform.target().unwrap();
        script(
            &releases.join(format!("moebius-{target}")),
            &format!(
                "#!/bin/sh\ncase \"$1\" in\n  init) echo 'listen = \"127.0.0.1:7171\"' > moebius.toml ;;\n  \
                 serve) mkdir -p .moebius && printf '%s' '{token}' > .moebius/token && exec sleep 3600 ;;\nesac\n"
            ),
        );
        let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-core/testdata/fake-ssh");
        let drop_file = dir.path().join("drop");
        let ssh = dir.path().join("ssh");
        script(
            &ssh,
            &format!(
                "#!/bin/sh\nHOME='{}' PATH='{}':\"$PATH\" FAKE_SSH_DROP='{}' {extra} exec '{}' \"$@\"\n",
                home.display(),
                bin.display(),
                drop_file.display(),
                fake.display()
            ),
        );
        Remote {
            release: format!("file://{}/moebius-{{target}}", releases.display()),
            dir,
            ssh,
            drop_file,
        }
    }

    fn plan(&self, askpass: Option<Askpass>) -> Plan {
        let ssh = Ssh::new(&self.ssh);
        let ssh = match askpass {
            Some(askpass) => ssh.with_askpass(askpass),
            None => ssh,
        };
        let mut plan = Plan::new(ssh, "fakehost");
        plan.release = self.release.clone();
        plan
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        // The detached server of the fake remote.
        if let Ok(pid) = std::fs::read_to_string(self.home().join(".moebius/server/serve.pid")) {
            let _ = std::process::Command::new("kill").arg(pid.trim()).status();
        }
    }
}

fn script(path: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The events of a setup up to its last: `Ready`, `Missing` or `Failed`.
async fn setup(plan: Plan, start: Start, askpass: Option<Askpass>) -> Vec<Event> {
    let mut stream = Box::pin(remote::connect(plan, start));
    let mut events = Vec::new();
    while let Some(event) = next(&mut stream).await {
        if let (Event::Prompt(_), Some(askpass)) = (&event, &askpass) {
            askpass.answer(Some("secret".to_string()));
        }
        let last = matches!(
            event,
            Event::Ready { .. } | Event::Missing { .. } | Event::Failed(_)
        );
        events.push(event);
        if last {
            break;
        }
    }
    events
}

async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    tokio::time::timeout(Duration::from_secs(120), stream.next())
        .await
        .expect("nothing in two minutes")
}

fn ready(events: &[Event]) -> (String, u16) {
    match events.last() {
        Some(Event::Ready { token, port }) => (token.clone(), *port),
        other => panic!("not ready: {other:?}\n{events:#?}"),
    }
}

fn logged(events: &[Event], line: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, Event::Log(l) if l.contains(line)))
}

#[tokio::test]
async fn a_host_without_moebius_is_offered_the_release_and_set_up_after_the_install() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = fixtures::start(dir.path()).await;
    let remote = Remote::new(&settings.token, "");

    let events = setup(remote.plan(None), Start::Check, None).await;
    let Some(Event::Missing { system, url }) = events.last() else {
        panic!("{events:#?}");
    };
    assert!(
        system.starts_with("Linux") || system.starts_with("Darwin"),
        "{system}"
    );
    assert!(
        url.starts_with("file://") && url.contains("moebius-"),
        "{url}"
    );
    assert!(!remote.home().join(".moebius/server").exists());

    let events = setup(remote.plan(None), Start::Install(Install::Download), None).await;
    let (token, port) = ready(&events);
    assert_eq!(token, settings.token);
    assert_eq!(token, server.token.to_string());
    let steps: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Step(step) => Some(*step),
            _ => None,
        })
        .collect();
    assert!(steps.contains(&remote::Step::Install), "{steps:?}");
    assert!(remote.home().join(".moebius/bin/moebius").is_file());
    assert!(remote.home().join(".moebius/server/moebius.toml").is_file());
    assert!(remote.home().join(".moebius/server/serve.pid").is_file());
    // The token is read and never written to the log.
    assert!(!logged(&events, &token));

    // A second time: nothing is installed, configured or started again.
    let again = setup(remote.plan(None), Start::Check, None).await;
    assert_eq!(ready(&again).0, token);
    assert!(logged(&again, "The configuration exists."));
    assert!(logged(&again, "The server runs."));
    assert!(!again.contains(&Event::Step(remote::Step::Install)));

    // The tunnel carries the API to the server, and comes back after a drop.
    let ssh = Ssh::new(&remote.ssh);
    let tunnel = Tunnel {
        ssh,
        host: "fakehost".into(),
        local_port: port,
        remote_port: server_port(&settings),
    };
    let mut stream = Box::pin(tunnel::run(&tunnel));
    assert_eq!(next(&mut stream).await, Some(TunnelEvent::Up));
    let through = Settings {
        address: format!("http://127.0.0.1:{port}"),
        token: token.clone(),
    };
    let client = Client::new(through);
    assert!(client.snapshot().await.is_ok());
    std::fs::write(&remote.drop_file, "").unwrap();
    assert!(matches!(
        next(&mut stream).await,
        Some(TunnelEvent::Down(_))
    ));
    assert_eq!(next(&mut stream).await, Some(TunnelEvent::Up));
    assert!(client.snapshot().await.is_ok());
}

fn server_port(settings: &Settings) -> u16 {
    settings
        .address
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn a_remote_without_internet_gets_the_release_uploaded() {
    let remote = Remote::new("upload-token", "");
    let events = setup(remote.plan(None), Start::Install(Install::Upload), None).await;
    assert_eq!(ready(&events).0, "upload-token");
    assert!(logged(&events, "Uploading"));
    assert!(remote.home().join(".moebius/bin/moebius").is_file());
}

#[tokio::test]
async fn a_password_is_asked_through_the_app_and_a_wrong_one_fails_the_login() {
    let askpass = Askpass::start(env!("CARGO_BIN_EXE_moebius-desktop").into())
        .await
        .unwrap();
    let remote = Remote::new(
        "password-token",
        "FAKE_SSH_ASK='Password:' FAKE_SSH_PASSWORD=secret",
    );
    let events = setup(
        remote.plan(Some(askpass.clone())),
        Start::Check,
        Some(askpass.clone()),
    )
    .await;
    assert!(
        events.contains(&Event::Prompt("Password:".to_string())),
        "{events:#?}"
    );
    assert!(
        matches!(events.last(), Some(Event::Missing { .. })),
        "{events:#?}"
    );

    // Cancelling the question is a refused login, and the password is in no log.
    let mut stream = Box::pin(remote::connect(
        remote.plan(Some(askpass.clone())),
        Start::Check,
    ));
    let mut events = Vec::new();
    while let Some(event) = next(&mut stream).await {
        if matches!(event, Event::Prompt(_)) {
            askpass.answer(None);
        }
        let done = matches!(event, Event::Failed(_));
        events.push(event);
        if done {
            break;
        }
    }
    let Some(Event::Failed(error)) = events.last() else {
        panic!("{events:#?}");
    };
    assert!(error.contains("Permission denied"), "{error}");
    assert!(!format!("{events:?}").contains("secret"));
}
