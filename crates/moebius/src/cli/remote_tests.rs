//! Tests of the setup of a host over SSH, against a fake `ssh` that runs what it is given on this machine with a home
//! directory of its own, and fake `uname`, `curl`, `systemctl` and `loginctl` programs on the host's `PATH`.

// The fakes are shell scripts started by their `#!` line, which Windows does not run.
#![cfg(unix)]

use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::remote::{Options, set_up};

/// The shell command that writes the line of `SHA256SUMS` for the release: its real hash, or a wrong one.
fn sums_command(dir: &Path) -> String {
    format!(
        "{{ if [ -f '{bad}' ]; then printf '%064d' 0; else sha256sum '{release}' | cut -d' ' -f1 | tr -d '\\n'; fi; \
         echo '  moebius-x86_64-unknown-linux-gnu'; }}",
        bad = dir.join("badsum").display(),
        release = dir.join("moebius-release").display(),
    )
}

/// A host: its home directory, the `ssh` that logs in to it and the programs on its `PATH`.
struct Host {
    dir: tempfile::TempDir,
}

fn script(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

impl Host {
    fn new() -> Host {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("home")).unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        let bin = dir.path().join("bin");
        script(&bin.join("uname"), "#!/bin/sh\necho 'Linux x86_64'\n");
        // No systemd, so that the server is started detached; `with_systemd` replaces it.
        script(&bin.join("systemctl"), "#!/bin/sh\nexit 1\n");
        script(
            &bin.join("loginctl"),
            "#!/bin/sh\necho \"$@\" >> \"$HOME/loginctl.log\"\n",
        );
        // The program that downloads notes what it was asked for and writes what the URL stands for where `-o` says: the
        // list of sums (wrong if the file `badsum` is there), the answer of a server's health route (if the file
        // `health` is there) or the fake program.
        script(
            &bin.join("curl"),
            &format!(
                "#!/bin/sh\necho \"$@\" >> \"$HOME/curl.log\"\nurl=; out=\nwhile [ $# -gt 0 ]; do case \"$1\" in -o) out=$2; shift ;; http*) url=$1 ;; esac; shift; done\n\
                 case \"$url\" in\n\
                 */SHA256SUMS) {sums} > \"$out\" ;;\n\
                 */v1/health) if [ -f '{health}' ]; then printf 200; else printf 000; exit 7; fi ;;\n\
                 *) cp '{release}' \"$out\" ;;\nesac\n",
                sums = sums_command(dir.path()),
                health = dir.path().join("health").display(),
                release = dir.path().join("moebius-release").display(),
            ),
        );
        script(
            &dir.path().join("moebius-release"),
            &format!(
                "#!/bin/sh\ncase \"$1\" in\n--version) echo 'moebius {}' ;;\ninit) touch moebius.toml; echo init >> \"$HOME/calls\" ;;\nserve) echo serve >> \"$HOME/calls\"; mkdir -p .moebius; echo secret > .moebius/token ;;\nesac\n",
                env!("CARGO_PKG_VERSION")
            ),
        );
        // The login notes the options it was given (up to the host) in `ssh.log` next to the home, prints the banner in
        // the file `banner` first, and adds the directory in the file `path` to the host's `PATH`.
        script(
            &dir.path().join("ssh"),
            &format!(
                "#!/bin/sh\nfor last; do :; done\nfor a; do [ \"$a\" = -- ] && break; printf '%s ' \"$a\"; done >> '{log}'; echo >> '{log}'\n\
                 if [ -f '{down}' ]; then echo 'ssh: connect to host: Connection refused' >&2; exit 255; fi\n\
                 [ -f '{banner}' ] && cat '{banner}'\n\
                 extra=$(cat '{path}' 2>/dev/null)\n\
                 HOME='{home}' PATH=\"${{extra:+$extra:}}\"'{bin}':\"$PATH\" exec sh -c \"$last\"\n",
                log = dir.path().join("ssh.log").display(),
                down = dir.path().join("down").display(),
                banner = dir.path().join("banner").display(),
                path = dir.path().join("path").display(),
                home = dir.path().join("home").display(),
                bin = bin.display(),
            ),
        );
        Host { dir }
    }

    fn ssh(&self) -> String {
        self.dir.path().join("ssh").to_string_lossy().into_owned()
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn server(&self) -> PathBuf {
        self.home().join(".moebius/server")
    }

    /// `moebius` already installed in the place a setup would put it.
    fn install_moebius(&self) {
        std::fs::create_dir_all(self.home().join(".moebius/bin")).unwrap();
        std::fs::copy(
            self.dir.path().join("moebius-release"),
            self.home().join(".moebius/bin/moebius"),
        )
        .unwrap();
    }

    /// A host with systemd: the fake `systemctl` knows the units in the file `active-units`, and starts a unit that is
    /// enabled by running its `ExecStart` in the server's directory.
    fn with_systemd(&self) {
        script(
            &self.dir.path().join("bin/systemctl"),
            "#!/bin/sh\necho \"$@\" >> \"$HOME/systemctl.log\"\ncase \"$2\" in\n\
             is-active) grep -qx \"$4\" \"$HOME/active-units\" 2>/dev/null; exit $? ;;\n\
             enable) cmd=$(sed -n 's/^ExecStart=//p' \"$HOME/.config/systemd/user/$4\" | sed \"s|%h|$HOME|g\"); cd \"$HOME/.moebius/server\" && eval \"$cmd\" ;;\nesac\nexit 0\n",
        );
    }

    fn log(&self, name: &str) -> String {
        std::fs::read_to_string(self.home().join(name)).unwrap_or_default()
    }

    /// The options of each `ssh` call, one line per call.
    fn ssh_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("ssh.log"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.home().join("calls")).unwrap_or_default()
    }

    fn options(&self, yes: bool, interactive: bool) -> Options {
        Options {
            token_tries: 40,
            curl: self
                .dir
                .path()
                .join("bin/curl")
                .to_string_lossy()
                .into_owned(),
            ..Options::new(yes, interactive, false)
        }
    }
}

/// Runs a setup that is asked `answer` and returns the result with the questions put.
fn go(host: &Host, options: &Options, answer: bool) -> (Result<(), String>, Vec<String>) {
    let asked = RefCell::new(Vec::new());
    let result = set_up(&host.ssh(), "build", options, &mut |question| {
        asked.borrow_mut().push(question.to_string());
        Ok(answer)
    });
    (result, asked.into_inner())
}

#[test]
fn a_host_that_has_moebius_gets_a_configuration_and_a_server_after_one_question() {
    let host = Host::new();
    host.install_moebius();
    let (result, asked) = go(&host, &host.options(false, true), true);
    result.unwrap();
    assert_eq!(asked.len(), 1);
    assert!(
        asked[0].contains("No Moebius server answers on build"),
        "{asked:?}"
    );
    assert!(!asked[0].contains("Install"), "{asked:?}");
    assert_eq!(host.calls(), "init\nserve\n");
    assert!(host.server().join("moebius.toml").is_file());
    assert_eq!(
        std::fs::read_to_string(host.server().join(".moebius/token")).unwrap(),
        "secret\n"
    );
}

#[test]
fn a_configuration_that_exists_is_kept_and_a_server_that_runs_is_not_started_twice() {
    let host = Host::new();
    host.install_moebius();
    std::fs::create_dir_all(host.server().join(".moebius")).unwrap();
    std::fs::write(host.server().join("moebius.toml"), "mine").unwrap();
    std::fs::write(host.server().join(".moebius/token"), "old\n").unwrap();
    // A process that is alive: this one.
    std::fs::write(
        host.server().join("serve.pid"),
        std::process::id().to_string(),
    )
    .unwrap();
    let (result, _) = go(&host, &host.options(true, false), false);
    result.unwrap();
    assert_eq!(host.calls(), "");
    assert_eq!(
        std::fs::read_to_string(host.server().join("moebius.toml")).unwrap(),
        "mine"
    );
}

#[test]
fn moebius_is_installed_from_the_release_of_this_version_after_a_yes() {
    let host = Host::new();
    let (result, asked) = go(&host, &host.options(false, true), true);
    result.unwrap();
    let question = &asked[0];
    let url = format!(
        "https://github.com/Code000-211/Moebius/releases/download/v{}/moebius-x86_64-unknown-linux-gnu",
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        question.contains("Linux x86_64") && question.contains(&url),
        "{question}"
    );
    // The host downloaded it, into the place `find` looks in, and the server started from it.
    let log = std::fs::read_to_string(host.home().join("curl.log")).unwrap();
    assert!(log.contains(&url), "{log}");
    assert!(host.home().join(".moebius/bin/moebius").is_file());
    assert_eq!(host.calls(), "init\nserve\n");
}

#[test]
fn the_program_can_be_sent_from_this_machine_for_a_host_without_internet() {
    let host = Host::new();
    // The program that downloads here prints the sums or the bytes of the release, and notes its arguments.
    let curl = host.dir.path().join("local-curl");
    script(
        &curl,
        &format!(
            "#!/bin/sh\necho \"$@\" >> '{log}'\nfor last; do :; done\ncase \"$last\" in\n*/SHA256SUMS) {sums} ;;\n*) cat '{release}' ;;\nesac\n",
            log = host.dir.path().join("local-curl.log").display(),
            sums = sums_command(host.dir.path()),
            release = host.dir.path().join("moebius-release").display(),
        ),
    );
    // The host has no curl and no wget of its own that works.
    script(&host.dir.path().join("bin/curl"), "#!/bin/sh\nexit 1\n");
    let options = Options {
        upload: true,
        curl: curl.to_string_lossy().into_owned(),
        ..host.options(true, false)
    };
    let (result, _) = go(&host, &options, false);
    result.unwrap();
    assert!(host.home().join(".moebius/bin/moebius").is_file());
    assert_eq!(host.calls(), "init\nserve\n");
    // Fetched here over HTTPS only, the sums as well.
    let log = std::fs::read_to_string(host.dir.path().join("local-curl.log")).unwrap();
    assert_eq!(log.lines().count(), 2, "{log}");
    assert!(
        log.lines()
            .all(|line| line.contains("--proto =https --tlsv1.2")),
        "{log}"
    );
    assert!(log.contains("/SHA256SUMS"), "{log}");
}

#[test]
fn a_program_that_does_not_match_the_sums_is_not_sent_nor_installed() {
    let host = Host::new();
    let curl = host.dir.path().join("local-curl");
    script(
        &curl,
        &format!(
            "#!/bin/sh\nfor last; do :; done\ncase \"$last\" in\n*/SHA256SUMS) {sums} ;;\n*) cat '{release}' ;;\nesac\n",
            sums = sums_command(host.dir.path()),
            release = host.dir.path().join("moebius-release").display(),
        ),
    );
    std::fs::write(host.dir.path().join("badsum"), "").unwrap();
    let options = Options {
        upload: true,
        curl: curl.to_string_lossy().into_owned(),
        ..host.options(true, false)
    };
    let (result, _) = go(&host, &options, false);
    let error = result.unwrap_err();
    assert!(error.contains("does not match"), "{error}");
    assert!(!host.home().join(".moebius/bin").exists());
    assert_eq!(host.calls(), "");
}

#[test]
fn an_upload_is_checked_on_the_host_too() {
    // The host's `sha256sum` refuses the bytes: they are not installed, whatever this machine thought.
    let host = Host::new();
    script(
        &host.dir.path().join("bin/sha256sum"),
        "#!/bin/sh\ncat >/dev/null\nexit 1\n",
    );
    let curl = host.dir.path().join("local-curl");
    script(
        &curl,
        &format!(
            "#!/bin/sh\nfor last; do :; done\ncase \"$last\" in\n*/SHA256SUMS) {sums} ;;\n*) cat '{release}' ;;\nesac\n",
            sums = sums_command(host.dir.path()),
            release = host.dir.path().join("moebius-release").display(),
        ),
    );
    let options = Options {
        upload: true,
        curl: curl.to_string_lossy().into_owned(),
        ..host.options(true, false)
    };
    let error = go(&host, &options, false).0.unwrap_err();
    assert!(error.contains("does not match"), "{error}");
    assert!(!host.home().join(".moebius/bin/moebius").exists());
    assert!(!host.home().join(".moebius/bin/moebius.part").exists());
    assert_eq!(host.calls(), "");
}

#[test]
fn a_download_is_checked_against_the_sums_next_to_it_over_https() {
    let host = Host::new();
    let (result, _) = go(&host, &host.options(true, false), false);
    result.unwrap();
    let log = host.log("curl.log");
    let release = format!(
        "https://github.com/Code000-211/Moebius/releases/download/v{}",
        env!("CARGO_PKG_VERSION")
    );
    assert!(log.contains(&format!("{release}/SHA256SUMS")), "{log}");
    assert!(
        log.lines()
            .filter(|l| l.contains("--proto =https --tlsv1.2"))
            .count()
            >= 2,
        "{log}"
    );
    // Nothing is left beside the program.
    let names: Vec<_> = std::fs::read_dir(host.home().join(".moebius/bin"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["moebius"]);
}

#[test]
fn a_download_that_does_not_match_the_sums_is_removed_and_not_installed() {
    let host = Host::new();
    std::fs::write(host.dir.path().join("badsum"), "").unwrap();
    let (result, _) = go(&host, &host.options(true, false), false);
    let error = result.unwrap_err();
    assert!(error.contains("does not match the SHA256SUMS"), "{error}");
    let bin = host.home().join(".moebius/bin");
    assert!(std::fs::read_dir(&bin).unwrap().next().is_none());
    assert_eq!(host.calls(), "");
    assert!(!host.server().exists());
}

#[test]
fn without_a_terminal_and_without_yes_nothing_is_changed() {
    let host = Host::new();
    host.install_moebius();
    let (result, asked) = go(&host, &host.options(false, false), true);
    let error = result.unwrap_err();
    assert!(
        error.contains("not a terminal") && error.contains("--yes"),
        "{error}"
    );
    assert!(asked.is_empty());
    assert_eq!(host.calls(), "");
    assert!(!host.server().exists());
}

#[test]
fn a_no_answer_changes_nothing_and_starts_no_install() {
    let host = Host::new();
    let (result, asked) = go(&host, &host.options(false, true), false);
    assert!(result.unwrap_err().contains("nothing was set up on build"));
    assert_eq!(asked.len(), 1);
    assert!(!host.home().join(".moebius").exists());
}

#[test]
fn a_host_that_cannot_be_reached_and_a_system_without_a_release_are_named() {
    let host = Host::new();
    std::fs::write(host.dir.path().join("down"), "").unwrap();
    let (result, asked) = go(&host, &host.options(true, false), true);
    let error = result.unwrap_err();
    assert!(
        error.contains("cannot connect") && error.contains("Connection refused"),
        "{error}"
    );
    assert!(asked.is_empty());

    std::fs::remove_file(host.dir.path().join("down")).unwrap();
    script(
        &host.dir.path().join("bin/uname"),
        "#!/bin/sh\necho 'FreeBSD amd64'\n",
    );
    let (result, asked) = go(&host, &host.options(true, true), true);
    assert!(
        result
            .unwrap_err()
            .contains("no release of Moebius for FreeBSD amd64")
    );
    assert!(asked.is_empty());
}

#[test]
fn a_server_that_writes_no_token_is_reported_with_its_log() {
    let host = Host::new();
    host.install_moebius();
    script(
        &host.home().join(".moebius/bin/moebius"),
        "#!/bin/sh\ncase \"$1\" in\ninit) touch moebius.toml ;;\nserve) echo 'address in use' >&2 ;;\nesac\n",
    );
    let options = Options {
        token_tries: 1,
        ..host.options(true, false)
    };
    let (result, _) = go(&host, &options, true);
    let error = result.unwrap_err();
    assert!(
        error.contains("wrote no token") && error.contains("address in use"),
        "{error}"
    );
}

#[test]
fn a_server_that_runs_elsewhere_is_not_set_up_over() {
    let host = Host::new();
    host.install_moebius();
    // Something answers on the port, though its token is not where `connect` reads it.
    std::fs::write(host.dir.path().join("health"), "").unwrap();
    let (result, asked) = go(&host, &host.options(true, true), true);
    let error = result.unwrap_err();
    assert!(
        error.contains("runs on build already") && error.contains("port 7171"),
        "{error}"
    );
    assert!(
        error.contains("~/.moebius/server/.moebius/token") && error.contains("--token"),
        "{error}"
    );
    assert!(asked.is_empty());
    assert_eq!(host.calls(), "");
    assert!(!host.server().exists());
}

#[test]
fn an_active_unit_of_a_server_is_one_that_runs_and_its_token_is_named() {
    for unit in ["moebius.service", "moebius-connect.service"] {
        let host = Host::new();
        host.install_moebius();
        host.with_systemd();
        std::fs::write(host.home().join("active-units"), format!("{unit}\n")).unwrap();
        std::fs::create_dir_all(host.home().join(".config/moebius/.moebius")).unwrap();
        std::fs::write(host.home().join(".config/moebius/.moebius/token"), "t").unwrap();
        let (result, asked) = go(&host, &host.options(true, true), true);
        let error = result.unwrap_err();
        assert!(
            error.contains(&format!("the systemd unit {unit} is active"))
                && error.contains(".config/moebius/.moebius/token"),
            "{error}"
        );
        assert!(asked.is_empty());
        assert!(!host.server().exists());
        assert!(!host.home().join(".config/systemd").exists());
    }
}

#[test]
fn the_port_of_the_configuration_is_the_one_asked() {
    let host = Host::new();
    host.install_moebius();
    std::fs::create_dir_all(host.server()).unwrap();
    std::fs::write(
        host.server().join("moebius.toml"),
        "# listen = \"127.0.0.1:1\"\nlisten = \"127.0.0.1:7300\"\n",
    )
    .unwrap();
    std::fs::write(host.dir.path().join("health"), "").unwrap();
    let error = go(&host, &host.options(true, true), true).0.unwrap_err();
    assert!(error.contains("port 7300"), "{error}");
    assert!(host.log("curl.log").contains("127.0.0.1:7300/v1/health"));
}

#[test]
fn with_systemd_a_unit_of_its_own_is_installed_enabled_and_lingered() {
    let host = Host::new();
    host.install_moebius();
    host.with_systemd();
    // The unit of the guide is there, and is not touched.
    let units = host.home().join(".config/systemd/user");
    std::fs::create_dir_all(&units).unwrap();
    std::fs::write(units.join("moebius.service"), "mine").unwrap();
    let (result, asked) = go(&host, &host.options(false, true), true);
    result.unwrap();
    // The person is told about the unit and the linger before saying yes.
    assert!(
        asked[0].contains("systemd user service moebius-connect.service")
            && asked[0].contains("installed and enabled")
            && asked[0].contains("enable-linger"),
        "{asked:?}"
    );
    assert_eq!(
        std::fs::read_to_string(units.join("moebius.service")).unwrap(),
        "mine"
    );
    let unit = std::fs::read_to_string(units.join("moebius-connect.service")).unwrap();
    assert!(
        unit.contains("ExecStart=%h/.moebius/bin/moebius serve\n")
            && unit.contains("WorkingDirectory=%h/.moebius/server\n")
            && unit.contains("Environment=PATH=")
            && !unit.contains('@'),
        "{unit}"
    );
    let path = unit
        .lines()
        .find_map(|l| l.strip_prefix("Environment=PATH="))
        .unwrap();
    assert!(
        path.starts_with(&format!("{}/.local/bin:", host.home().display())),
        "{path}"
    );
    let systemctl = host.log("systemctl.log");
    assert!(
        systemctl.contains("--user daemon-reload")
            && systemctl.contains("--user enable --now moebius-connect.service"),
        "{systemctl}"
    );
    assert!(
        host.log("loginctl.log").starts_with("enable-linger "),
        "{}",
        host.log("loginctl.log")
    );
    // The fake systemd started it, so it is not also started detached.
    assert_eq!(host.calls(), "init\nserve\n");
    assert!(!host.server().join("serve.pid").exists());
}

#[test]
fn a_program_whose_path_a_unit_cannot_take_is_started_detached() {
    let host = Host::new();
    host.with_systemd();
    // `moebius` is on the host's `PATH`, in a directory with a space in its name.
    let dir = host.dir.path().join("my bin");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(host.dir.path().join("moebius-release"), dir.join("moebius")).unwrap();
    std::fs::write(
        host.dir.path().join("path"),
        dir.to_string_lossy().as_bytes(),
    )
    .unwrap();
    let (result, asked) = go(&host, &host.options(false, true), true);
    result.unwrap();
    assert!(asked[0].contains("moebius-connect.service"), "{asked:?}");
    assert!(!host.home().join(".config/systemd").exists());
    assert!(!host.log("systemctl.log").contains("enable"));
    assert_eq!(host.calls(), "init\nserve\n");
    assert!(host.server().join("serve.pid").is_file());
}

#[test]
fn without_systemd_the_question_says_it_starts_detached() {
    let host = Host::new();
    host.install_moebius();
    let (_, asked) = go(&host, &host.options(false, true), true);
    assert!(
        asked[0].contains("started detached") && !asked[0].contains("systemd"),
        "{asked:?}"
    );
    assert!(host.log("loginctl.log").is_empty());
}

#[test]
fn what_a_setup_writes_is_private_to_the_person() {
    use std::os::unix::fs::MetadataExt;
    let host = Host::new();
    host.install_moebius();
    // A log from before, readable by all.
    std::fs::create_dir_all(host.server()).unwrap();
    std::fs::set_permissions(host.server(), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(host.server().join("serve.log"), "old").unwrap();
    std::fs::set_permissions(
        host.server().join("serve.log"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    go(&host, &host.options(true, false), true).0.unwrap();
    let mode = |path: PathBuf| std::fs::metadata(path).unwrap().mode() & 0o777;
    assert_eq!(mode(host.server()), 0o700);
    assert_eq!(mode(host.server().join("serve.log")), 0o600);
    assert_eq!(mode(host.server().join(".moebius")), 0o700);
}

#[test]
fn a_chatty_login_cannot_shift_what_is_read() {
    // Moebius is not installed, so the system decides the release: a banner that looks like `uname` and a path must
    // not be taken for it.
    let host = Host::new();
    std::fs::write(
        host.dir.path().join("banner"),
        "Last login: yesterday\nFreeBSD amd64\n/usr/bin/other\n",
    )
    .unwrap();
    let (result, asked) = go(&host, &host.options(false, true), true);
    result.unwrap();
    assert!(asked[0].contains("Linux x86_64"), "{asked:?}");
    assert_eq!(host.calls(), "init\nserve\n");
    assert!(host.home().join(".moebius/bin/moebius").is_file());
}

#[test]
fn an_older_moebius_is_replaced_after_a_yes_and_a_newer_one_never() {
    let host = Host::new();
    host.install_moebius();
    let installed = host.home().join(".moebius/bin/moebius");
    script(
        &installed,
        "#!/bin/sh\ncase \"$1\" in --version) echo 'moebius 0.0.1' ;; esac\n",
    );
    let (result, asked) = go(&host, &host.options(false, true), true);
    result.unwrap();
    assert!(
        asked[0].contains("(moebius 0.0.1) is older than this program")
            && asked[0].contains("Install moebius"),
        "{asked:?}"
    );
    // Ours now: the fake release started the server.
    assert_eq!(host.calls(), "init\nserve\n");
    assert!(host.log("curl.log").contains("/SHA256SUMS"));

    let host = Host::new();
    host.install_moebius();
    let installed = host.home().join(".moebius/bin/moebius");
    script(
        &installed,
        "#!/bin/sh\ncase \"$1\" in --version) echo 'moebius 99.0.0' ;; init) touch moebius.toml ;; serve) mkdir -p .moebius; echo t > .moebius/token ;; esac\n",
    );
    let (result, asked) = go(&host, &host.options(false, true), true);
    result.unwrap();
    assert!(!asked[0].contains("Install"), "{asked:?}");
    assert_eq!(
        host.log("curl.log")
            .lines()
            .filter(|l| !l.contains("/v1/health"))
            .count(),
        0
    );
}

#[test]
fn the_steps_share_one_connection_and_never_ask_without_a_terminal() {
    let host = Host::new();
    host.install_moebius();
    go(&host, &host.options(true, false), true).0.unwrap();
    let calls = host.ssh_calls();
    // Look, configure, start, one wait for the token, and the end of the shared connection.
    assert_eq!(calls.len(), 5, "{calls:?}");
    assert!(
        calls.iter().all(|c| c.contains("BatchMode=yes")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|c| c.contains("ControlMaster=auto") && c.contains("ControlPersist=60")),
        "{calls:?}"
    );
    assert!(calls.last().unwrap().contains("-O exit"), "{calls:?}");
    // One private directory for the sockets, which is gone.
    let socket = calls[0]
        .split_whitespace()
        .find_map(|word| word.strip_prefix("ControlPath="))
        .unwrap();
    let dir = Path::new(socket).parent().unwrap();
    assert!(!dir.exists(), "{dir:?}");

    // With a terminal ssh may ask.
    let host = Host::new();
    host.install_moebius();
    go(&host, &host.options(false, true), true).0.unwrap();
    assert!(host.ssh_calls().iter().all(|c| !c.contains("BatchMode")));
}
