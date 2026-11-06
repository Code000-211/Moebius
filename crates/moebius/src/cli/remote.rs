//! Setting a server up on the other side of an SSH connection, from the terminal: find out the system and whether a
//! server runs there already, find `moebius` (or install it from the release of this version, after asking, and check it
//! against the `SHA256SUMS` of that release), make the configuration when there is none, start the server, and wait for
//! its token. `moebius connect <host>` does this when no server answers there; the tunnel (`ssh`) is the way to the
//! server, which listens on `127.0.0.1` only.
//!
//! Every step is one `ssh` call that runs a POSIX shell script on the host; the system's `ssh` does the login (keys, the
//! agent, `~/.ssh/config`, jump hosts), and asks for a password or a yes on the terminal when it needs one. Where `ssh`
//! can share a connection (not on Windows) the steps share one, so that a password is asked once. The scripts print the
//! facts they report in lines that start with `MOEBIUS-<NAME> `, and only those lines are read, so that a banner or a
//! chatty shell profile on the host cannot shift anything. Nothing on the host changes before the person has said yes
//! (or `--yes` was given).

use std::io::{BufRead, Write};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

/// Where a release of `moebius` is: `{version}` is this program's version and `{target}` the Rust target triple of the
/// remote system. The one place that names it.
pub const RELEASE_URL: &str =
    "https://github.com/Code000-211/Moebius/releases/download/v{version}/moebius-{target}";
/// Where `moebius` is put on the host, and the directory of the configuration and data it sets up, under the host's
/// home directory.
const BINARY: &str = ".moebius/bin/moebius";
const SERVER_DIR: &str = ".moebius/server";
/// The name of the systemd user unit a setup installs: not `moebius.service`, which is the name of the unit that the
/// guide tells people to install by hand.
const UNIT: &str = "moebius-connect.service";
/// How often the token file is looked for, one second apart, in the one call that waits for it.
const TOKEN_TRIES: u32 = 20;
/// Where a server keeps its token: the place a setup leaves it, and the place of the manual install in the guide.
const TOKEN_FILES: [&str; 2] = [
    "~/.moebius/server/.moebius/token",
    "~/.config/moebius/.moebius/token",
];

/// What a setup is told.
#[derive(Debug, Clone)]
pub struct Options {
    /// Answer yes to the question: set up without asking (`--yes`).
    pub yes: bool,
    /// Whether the questions can be asked: the terminal is a person's.
    pub interactive: bool,
    /// Send the program from this machine instead of letting the host download it (`--upload`).
    pub upload: bool,
    /// The URL pattern of a release; [`RELEASE_URL`] unless a test says another.
    pub release: String,
    /// The program that downloads on this machine for `upload`.
    pub curl: String,
    /// How many times, a second apart, the token is looked for.
    pub token_tries: u32,
}

impl Options {
    pub fn new(yes: bool, interactive: bool, upload: bool) -> Options {
        Options {
            yes,
            interactive,
            upload,
            release: RELEASE_URL.to_string(),
            curl: "curl".to_string(),
            token_tries: TOKEN_TRIES,
        }
    }
}

/// What a command on the host printed, as text.
struct Output {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// The system of the host, as `uname -sm` says.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Platform {
    system: String,
    arch: String,
}

impl Platform {
    fn parse(uname: &str) -> Result<Platform, String> {
        let mut words = uname.split_whitespace();
        match (words.next(), words.next()) {
            (Some(system), Some(arch)) => Ok(Platform {
                system: system.to_string(),
                arch: arch.to_string(),
            }),
            _ => Err(format!("cannot tell the system from {:?}", uname.trim())),
        }
    }

    /// The Rust target triple of a release for this system: releases are built for Linux on x86_64 only.
    fn target(&self) -> Result<&'static str, String> {
        match (self.system.as_str(), self.arch.as_str()) {
            ("Linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
            (system, arch) => Err(format!(
                "no release of Moebius for {system} {arch} (releases are built for Linux x86_64): install \
                 moebius there yourself, in ~/.moebius/bin or on the PATH, and run connect again"
            )),
        }
    }
}

/// The URL of the release for `platform` out of the pattern.
fn release_url(pattern: &str, platform: &Platform) -> Result<String, String> {
    Ok(pattern
        .replace("{version}", env!("CARGO_PKG_VERSION"))
        .replace("{target}", platform.target()?))
}

/// `text` as one word for a POSIX shell.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Reads yes or no on the terminal: only `y` and `yes` are yes.
pub fn ask_on_terminal(question: &str) -> Result<bool, String> {
    eprint!("{question} [y/N] ");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|e| format!("cannot read the answer: {e}"))?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// A shell script out of a template: each `@{NAME}` is replaced by its value, which the caller has quoted for a shell.
fn template(text: &str, values: &[(&str, &str)]) -> String {
    values.iter().fold(text.to_string(), |text, (name, value)| {
        text.replace(&format!("@{{{name}}}"), value)
    })
}

/// What the facts about a host are looked up with: the system, `moebius` and its version, the port the server would
/// listen on and whether something answers there, the active systemd units, whether systemd can be used, and where a
/// token is. Each fact is a line `MOEBIUS-<NAME> <value>`.
const PROBE: &str = r#"echo "MOEBIUS-SYSTEM $(uname -sm)"
p=$(command -v moebius 2>/dev/null)
if [ -z "$p" ] && [ -x "$HOME/@{BINARY}" ]; then p="$HOME/@{BINARY}"; fi
if [ -n "$p" ]; then
  echo "MOEBIUS-PATH $p"
  echo "MOEBIUS-VERSION $("$p" --version 2>/dev/null | head -n 1)"
fi
port=7171
if [ -f "$HOME/@{SERVER}/moebius.toml" ]; then
  l=$(sed -n 's/^listen *= *"[^"]*:\([0-9][0-9]*\)".*/\1/p' "$HOME/@{SERVER}/moebius.toml" | head -n 1)
  if [ -n "$l" ]; then port=$l; fi
fi
echo "MOEBIUS-PORT $port"
code=
if command -v curl >/dev/null 2>&1; then
  code=$(curl -s -o /dev/null --max-time 5 -w '%{http_code}' "http://127.0.0.1:$port/v1/health" 2>/dev/null)
elif command -v wget >/dev/null 2>&1; then
  if wget -q -O /dev/null --timeout=5 "http://127.0.0.1:$port/v1/health" 2>/dev/null; then code=200; fi
fi
case "$code" in ""|000|5*) ;; *) echo "MOEBIUS-HEALTH $code" ;; esac
for u in moebius.service @{UNIT}; do
  if systemctl --user is-active --quiet "$u" 2>/dev/null; then echo "MOEBIUS-UNIT $u"; fi
done
if systemctl --user show-environment >/dev/null 2>&1; then echo "MOEBIUS-SYSTEMD yes"; fi
for t in "$HOME/@{SERVER}/.moebius/token" "$HOME/.config/moebius/.moebius/token"; do
  if [ -s "$t" ]; then echo "MOEBIUS-TOKEN-AT $t"; fi
done"#;

/// Checks a file against a hash with `sha256sum -c` (else `shasum -a 256 -c`), and stops the script, failing closed,
/// when neither exists or the hash differs. Used as `verify <hash> <file>`.
const VERIFY: &str = r#"verify() {
  if command -v sha256sum >/dev/null 2>&1; then printf '%s  %s\n' "$1" "$2" | sha256sum -c - >/dev/null 2>&1
  elif command -v shasum >/dev/null 2>&1; then printf '%s  %s\n' "$1" "$2" | shasum -a 256 -c - >/dev/null 2>&1
  else echo 'neither sha256sum nor shasum is installed, so the download cannot be checked' >&2; exit 1
  fi || { echo 'the moebius that was downloaded does not match the SHA256SUMS of its release' >&2; exit 1; }
}"#;

/// Downloads the release and `SHA256SUMS` next to it on the host, checks the one against the other and puts the program
/// in place. The partial files are removed whatever happens.
const DOWNLOAD: &str = r#"set -e
umask 077
mkdir -p "$HOME/.moebius/bin"
t="$HOME/@{BINARY}"
trap 'rm -f "$t.part" "$t.sums"' EXIT
fetch() {
  if command -v curl >/dev/null 2>&1; then curl --proto '=https' --tlsv1.2 -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then wget -q --https-only -O "$2" "$1"
  else echo 'neither curl nor wget is installed' >&2; exit 1
  fi
}
@{VERIFY}
fetch @{URL} "$t.part"
fetch @{SUMS} "$t.sums"
expected=$(awk -v f=@{NAME} '$2 == f || $2 == "*" f { print $1; exit }' "$t.sums")
if [ -z "$expected" ]; then echo 'the SHA256SUMS of the release do not list the program' >&2; exit 1; fi
verify "$expected" "$t.part"
chmod +x "$t.part"
mv "$t.part" "$t"
echo "MOEBIUS-PATH $t""#;

/// Takes the program on its standard input, checks it against the hash this machine worked out, and puts it in place.
const UPLOAD: &str = r#"set -e
umask 077
mkdir -p "$HOME/.moebius/bin"
t="$HOME/@{BINARY}"
trap 'rm -f "$t.part"' EXIT
@{VERIFY}
cat > "$t.part"
verify @{HASH} "$t.part"
chmod +x "$t.part"
mv "$t.part" "$t"
echo "MOEBIUS-PATH $t""#;

/// Makes the server's directory (private: the log holds a login link) and `moebius init` in it unless it has a
/// configuration.
const CONFIGURE: &str = r#"umask 077
mkdir -p "$HOME/@{SERVER}" && chmod 700 "$HOME/@{SERVER}" && cd "$HOME/@{SERVER}" || exit 1
if [ -f moebius.toml ]; then echo 'The configuration exists.'; else @{BIN} init; fi"#;

/// Starts the server: nothing if one runs, else as a systemd user service where there is systemd and the program's path
/// is one a unit can name, else detached. The unit's text is a quoted here-document; only the program's path and the
/// `PATH`, both checked for characters a unit cannot take, are put into it.
const START: &str = r#"umask 077
cd "$HOME/@{SERVER}" || exit 1
for u in moebius.service @{UNIT}; do
  if systemctl --user is-active --quiet "$u" 2>/dev/null; then echo "MOEBIUS-RUNS $u"; exit 0; fi
done
if [ -f serve.pid ] && kill -0 "$(cat serve.pid)" 2>/dev/null; then echo 'MOEBIUS-RUNS serve.pid'; exit 0; fi
bin=@{BIN}
case "$bin$HOME" in
  *[!A-Za-z0-9._/+@:,=-]*) unit=no; echo 'MOEBIUS-NOTE the path of the program has characters a systemd unit cannot take' ;;
  *) unit=yes ;;
esac
if [ "$unit" = yes ] && systemctl --user show-environment >/dev/null 2>&1; then
  case "$bin" in "$HOME"/*) unit_bin="%h/${bin#"$HOME"/}" ;; *) unit_bin=$bin ;; esac
  unit_path="$HOME/.local/bin:$PATH"
  case "$unit_path" in
    *[!A-Za-z0-9._/+@:,=-]*) unit_path="$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin" ;;
  esac
  mkdir -p "$HOME/.config/systemd/user"
  sed -e "s|@UNIT_BIN@|$unit_bin|" -e "s|@UNIT_PATH@|$unit_path|" > "$HOME/.config/systemd/user/@{UNIT}" <<'UNIT'
[Unit]
Description=Moebius server (set up by moebius connect)
After=network-online.target
[Service]
WorkingDirectory=%h/@{SERVER}
Environment=PATH=@UNIT_PATH@
ExecStart=@UNIT_BIN@ serve
Restart=on-failure
RestartSec=5
[Install]
WantedBy=default.target
UNIT
  if systemctl --user daemon-reload && systemctl --user enable --now @{UNIT}; then
    loginctl enable-linger "${USER:-$(id -un)}" 2>/dev/null || true
    echo 'MOEBIUS-STARTED systemd'
    exit 0
  fi
  echo 'systemd could not start the unit @{UNIT}' >&2
  exit 1
fi
touch serve.log && chmod 600 serve.log
PATH="$HOME/.local/bin:$PATH" nohup "$bin" serve </dev/null >serve.log 2>&1 &
echo $! > serve.pid
echo 'MOEBIUS-STARTED detached'"#;

/// Looks for the token for a number of seconds, in this one call, and says what the server logged when there is none.
const TOKEN: &str = r#"f="$HOME/@{SERVER}/.moebius/token"
i=0
while :; do
  if [ -s "$f" ]; then echo 'MOEBIUS-TOKEN ready'; exit 0; fi
  i=$((i + 1))
  if [ "$i" -ge @{TRIES} ]; then break; fi
  sleep 1
done
{ tail -n 20 "$HOME/@{SERVER}/serve.log"; journalctl --user -u @{UNIT} -n 20 --no-pager; } 2>/dev/null | grep -v 'login=' | sed 's/^/MOEBIUS-LOG /'
exit 1"#;

/// The lines `MOEBIUS-<name> <value>` of what a script printed, as their values, in order.
fn marked<'a>(stdout: &'a str, name: &str) -> Vec<&'a str> {
    let prefix = format!("MOEBIUS-{name} ");
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix(prefix.as_str()))
        .map(str::trim)
        .collect()
}

/// What the first look at the host found.
#[derive(Debug, Default)]
struct Found {
    uname: String,
    binary: Option<String>,
    /// What `moebius --version` said, such as `moebius 0.7.0`.
    version: Option<String>,
    /// The port the server would listen on: the one of its configuration, else the usual one.
    port: u16,
    /// Something answers `GET /v1/health` on that port of the host.
    answers: bool,
    /// The active systemd user units that run a server.
    units: Vec<String>,
    /// Systemd can be used by this user.
    systemd: bool,
    /// The token files that exist.
    tokens: Vec<String>,
}

impl Found {
    fn parse(stdout: &str) -> Found {
        let first = |name: &str| marked(stdout, name).first().map(|v| v.to_string());
        Found {
            uname: first("SYSTEM").unwrap_or_default(),
            binary: first("PATH").filter(|path| !path.is_empty()),
            version: first("VERSION").filter(|version| !version.is_empty()),
            port: first("PORT")
                .and_then(|port| port.parse().ok())
                .unwrap_or(super::ssh::REMOTE_PORT),
            answers: first("HEALTH").is_some(),
            units: marked(stdout, "UNIT")
                .into_iter()
                .map(String::from)
                .collect(),
            systemd: first("SYSTEMD").is_some(),
            tokens: marked(stdout, "TOKEN-AT")
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }

    /// Whether the version of the program found is older than `ours`. A version that cannot be read is not older.
    fn is_older_than(&self, ours: &str) -> bool {
        match (
            self.version.as_deref().and_then(version_numbers),
            version_numbers(ours),
        ) {
            (Some(theirs), Some(ours)) => theirs < ours,
            _ => false,
        }
    }
}

/// The three numbers of the last word of `text` (`moebius 0.7.0` or `0.7.0-rc1`), if it has them.
fn version_numbers(text: &str) -> Option<(u64, u64, u64)> {
    let word = text.split_whitespace().last()?;
    let word = word.split(['-', '+']).next()?;
    let mut numbers = word.split('.').map(|part| part.parse::<u64>().ok());
    Some((numbers.next()??, numbers.next()??, numbers.next()??))
}

/// The hash of the program in the text of a `SHA256SUMS` file, by the name of the file.
fn sum_of(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let (hash, file) = (words.next()?, words.next()?);
        let hash_ok = hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit());
        (hash_ok && file.trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// What an install is told: the release's URL, its checksum list next to it and the file's name in that list.
struct Release {
    url: String,
    sums: String,
    name: String,
}

impl Release {
    fn of(url: String) -> Result<Release, String> {
        let (base, name) = url
            .rsplit_once('/')
            .ok_or_else(|| format!("{url} is not a URL of a release"))?;
        Ok(Release {
            sums: format!("{base}/SHA256SUMS"),
            name: name.to_string(),
            url,
        })
    }
}

/// A setup of one host.
struct Setup<'a> {
    program: &'a str,
    host: &'a str,
    options: &'a Options,
    /// A private directory for the socket that the steps share one connection through, where `ssh` can.
    #[cfg(not(windows))]
    control: Option<tempfile::TempDir>,
}

/// Sets the server up on `host`, asking `ask` (yes or no) before anything is changed there, unless `options.yes`.
/// Ends when the server runs and has written its token; the tunnel follows.
pub fn set_up(
    program: &str,
    host: &str,
    options: &Options,
    ask: &mut dyn FnMut(&str) -> Result<bool, String>,
) -> Result<(), String> {
    let setup = Setup {
        program,
        host,
        options,
        // Short, because a socket's path has a limit. Windows' `ssh` cannot share a connection.
        #[cfg(not(windows))]
        control: tempfile::Builder::new()
            .prefix("mb-")
            .tempdir_in("/tmp")
            .or_else(|_| tempfile::Builder::new().prefix("mb-").tempdir())
            .ok(),
    };
    let result = setup.go(ask);
    setup.close_connection();
    result
}

impl Setup<'_> {
    /// The options every `ssh` call gets.
    fn ssh_options(&self) -> Vec<String> {
        let mut args: Vec<String> = ["-T", "-o", "ConnectTimeout=30"].map(String::from).into();
        // Nobody can answer a question from `ssh` without a terminal: it fails instead of waiting.
        if !self.options.interactive {
            args.extend(["-o".into(), "BatchMode=yes".into()]);
        }
        #[cfg(not(windows))]
        if let Some(dir) = &self.control {
            args.extend([
                "-o".into(),
                "ControlMaster=auto".into(),
                "-o".into(),
                format!("ControlPath={}", dir.path().join("%C").display()),
                "-o".into(),
                "ControlPersist=60".into(),
            ]);
        }
        args
    }

    /// Ends the shared connection, which `ssh` keeps for a minute otherwise.
    fn close_connection(&self) {
        #[cfg(not(windows))]
        if self.control.is_some() {
            let _ = Command::new(self.program)
                .args(self.ssh_options())
                .args(["-O", "exit", "--", self.host])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    /// Runs `script` in a POSIX shell on the host, with `stdin` for it to read, and waits for it to end.
    fn run(&self, script: &str, stdin: Option<&[u8]>) -> Result<Output, String> {
        let mut child = Command::new(self.program)
            .args(self.ssh_options())
            .args([
                // A host is never read as an option, whatever its name.
                "--",
                self.host,
                &format!("sh -c {}", quote(script)),
            ])
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", self.program))?;
        let feeder = stdin.zip(child.stdin.take()).map(|(bytes, mut pipe)| {
            let bytes = bytes.to_vec();
            std::thread::spawn(move || {
                let _ = pipe.write_all(&bytes);
            })
        });
        let output = child
            .wait_with_output()
            .map_err(|e| format!("cannot wait for {}: {e}", self.program))?;
        if let Some(feeder) = feeder {
            let _ = feeder.join();
        }
        Ok(Output {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn say(&self, line: &str) {
        eprintln!("{}: {line}", self.host);
    }

    fn go(&self, ask: &mut dyn FnMut(&str) -> Result<bool, String>) -> Result<(), String> {
        self.say("looking for Moebius");
        let found = self.find()?;
        let platform = Platform::parse(&found.uname)?;
        if found.answers || !found.units.is_empty() {
            return Err(self.already_runs(&found));
        }
        let ours = env!("CARGO_PKG_VERSION");
        // Install ours where there is no moebius, or one older than this program; never replace a newer one.
        let install = match (&found.binary, found.is_older_than(ours)) {
            (Some(_), false) => None,
            _ => Some(Release::of(release_url(&self.options.release, &platform)?)?),
        };
        self.confirm(&self.question(&found, &platform, install.as_ref()), ask)?;
        let binary = match (install, found.binary) {
            (Some(release), _) => {
                self.say("installing Moebius");
                self.install(&release)?
            }
            (None, Some(binary)) => binary,
            (None, None) => unreachable!("an install is made when there is no program"),
        };
        self.say("setting up the configuration");
        self.configure(&binary)?;
        self.say("starting the server");
        self.start_server(&binary)?;
        self.say("waiting for the token");
        self.token()
    }

    /// What the person is asked: everything that will change on the host.
    fn question(&self, found: &Found, platform: &Platform, install: Option<&Release>) -> String {
        let mut text = format!(
            "No Moebius server answers on {host} ({system}, 127.0.0.1:{port}).",
            host = self.host,
            system = format_args!("{} {}", platform.system, platform.arch),
            port = found.port,
        );
        let ours = env!("CARGO_PKG_VERSION");
        if let Some(release) = install {
            let why = match &found.version {
                Some(version) if found.binary.is_some() => {
                    format!(" The moebius there ({version}) is older than this program ({ours}).")
                }
                _ => " moebius is not installed there.".to_string(),
            };
            text += &format!(
                "{why} Install moebius {ours} from {url} into ~/{BINARY}, checked against the SHA256SUMS \
                 next to it.",
                url = release.url
            );
        }
        text += &format!(
            " Set a server up in ~/{SERVER_DIR} (a configuration made by `moebius init` unless there is one, then \
             `moebius serve`)"
        );
        text += &if found.systemd {
            format!(
                ", run by the systemd user service {UNIT}, which is installed and enabled (`loginctl \
                 enable-linger` is tried too, so that it keeps running after you log out)."
            )
        } else {
            format!(
                ", started detached (its output, with a login link, goes to ~/{SERVER_DIR}/serve.log)."
            )
        };
        text
    }

    /// The error for a host where a server runs already, wherever its token is.
    fn already_runs(&self, found: &Found) -> String {
        let how = if found.answers {
            format!("a server answers on its port {}", found.port)
        } else {
            format!("the systemd unit {} is active", found.units.join(", "))
        };
        let token = match found.tokens.first() {
            Some(file) => format!("a token is in {file}"),
            None => format!(
                "its token is not in {}: it is in the `.moebius` folder of its data directory",
                TOKEN_FILES.join(" or ")
            ),
        };
        format!(
            "a Moebius server runs on {host} already ({how}), so nothing was set up; {token}. Connect with the \
             token yourself: `ssh -L {port}:127.0.0.1:{port} {host}`, then `moebius --server http://127.0.0.1:{port} \
             --token <token>`; or copy the token to {first} and run `moebius connect {host}` again",
            host = self.host,
            port = found.port,
            first = TOKEN_FILES[0],
        )
    }

    /// Asks, unless `--yes`; refuses clearly when there is nobody to ask.
    fn confirm(
        &self,
        question: &str,
        ask: &mut dyn FnMut(&str) -> Result<bool, String>,
    ) -> Result<(), String> {
        if self.options.yes {
            return Ok(());
        }
        if !self.options.interactive {
            return Err(format!(
                "no Moebius server answers on {host}, and this is not a terminal to ask in: run `moebius connect \
                 {host} --yes` to set one up there without asking",
                host = self.host
            ));
        }
        if ask(question)? {
            Ok(())
        } else {
            Err(format!("nothing was set up on {}", self.host))
        }
    }

    /// The facts about the host, in one login.
    fn find(&self) -> Result<Found, String> {
        let script = template(
            PROBE,
            &[("BINARY", BINARY), ("SERVER", SERVER_DIR), ("UNIT", UNIT)],
        );
        let done = self.run(&script, None)?;
        if !done.ok {
            return Err(failure("cannot connect", &done));
        }
        Ok(Found::parse(&done.stdout))
    }

    /// Puts the program of this version in `~/.moebius/bin` and returns its path on the host.
    fn install(&self, release: &Release) -> Result<String, String> {
        let done = if self.options.upload {
            self.say(&format!("downloading {} here", release.url));
            let bytes = self.download_here(&release.url)?;
            let sums = self.download_here(&release.sums)?;
            let hash = sum_of(&String::from_utf8_lossy(&sums), &release.name)
                .ok_or_else(|| format!("{} does not list {}", release.sums, release.name))?;
            if hex_sha256(&bytes) != hash {
                return Err(format!(
                    "the program downloaded from {} does not match {}",
                    release.url, release.sums
                ));
            }
            self.say(&format!("uploading {} bytes", bytes.len()));
            let script = template(
                UPLOAD,
                &[
                    ("BINARY", BINARY),
                    ("VERIFY", VERIFY),
                    ("HASH", &quote(&hash)),
                ],
            );
            let done = self.run(&script, Some(&bytes))?;
            if !done.ok {
                return Err(failure("cannot upload Moebius", &done));
            }
            done
        } else {
            self.say(&format!("downloading {} there", release.url));
            let script = template(
                DOWNLOAD,
                &[
                    ("BINARY", BINARY),
                    ("VERIFY", VERIFY),
                    ("URL", &quote(&release.url)),
                    ("SUMS", &quote(&release.sums)),
                    ("NAME", &quote(&release.name)),
                ],
            );
            let done = self.run(&script, None)?;
            if !done.ok {
                return Err(format!(
                    "{} (if the host has no internet, try --upload)",
                    failure("the host cannot download Moebius", &done)
                ));
            }
            done
        };
        marked(&done.stdout, "PATH")
            .first()
            .map(|path| path.to_string())
            .ok_or_else(|| "Moebius is not on the host after the install".to_string())
    }

    /// The bytes at `url`, fetched here with `curl` (only over HTTPS), which Linux, macOS and Windows 10 and later all
    /// have.
    fn download_here(&self, url: &str) -> Result<Vec<u8>, String> {
        let output = Command::new(&self.options.curl)
            .args(["--proto", "=https", "--tlsv1.2", "-fsSL", url])
            .output()
            .map_err(|e| format!("cannot run {}: {e}", self.options.curl))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(format!(
                "cannot download {url}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    /// `moebius init` in the server's directory, unless it has a configuration.
    fn configure(&self, binary: &str) -> Result<(), String> {
        let script = template(
            CONFIGURE,
            &[("SERVER", SERVER_DIR), ("BIN", &quote(binary))],
        );
        let done = self.run(&script, None)?;
        if done.ok {
            Ok(())
        } else {
            Err(failure("moebius init failed", &done))
        }
    }

    /// Starts `moebius serve` as a systemd user service where there is one, else detached; nothing if it runs.
    fn start_server(&self, binary: &str) -> Result<(), String> {
        let script = template(
            START,
            &[
                ("SERVER", SERVER_DIR),
                ("UNIT", UNIT),
                ("BIN", &quote(binary)),
            ],
        );
        let done = self.run(&script, None)?;
        for note in marked(&done.stdout, "NOTE") {
            self.say(note);
        }
        if done.ok {
            Ok(())
        } else {
            Err(failure("cannot start the server", &done))
        }
    }

    /// Waits for the token the server writes when it starts, in one call.
    fn token(&self) -> Result<(), String> {
        let script = template(
            TOKEN,
            &[
                ("SERVER", SERVER_DIR),
                ("UNIT", UNIT),
                ("TRIES", &self.options.token_tries.max(1).to_string()),
            ],
        );
        let done = self.run(&script, None)?;
        if !marked(&done.stdout, "TOKEN").is_empty() {
            return Ok(());
        }
        let log = marked(&done.stdout, "LOG").join("\n");
        Err(format!(
            "the server on {} wrote no token{}",
            self.host,
            if log.is_empty() {
                String::new()
            } else {
                format!("; its log says:\n{log}")
            }
        ))
    }
}

/// The reason a command failed: what it said last.
fn failure(what: &str, output: &Output) -> String {
    let said = output.stderr.trim();
    let said = if said.is_empty() {
        output.stdout.trim()
    } else {
        said
    };
    match said.lines().last() {
        Some(line) => format!("{what}: {line}"),
        None => what.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_picks_the_release() {
        let linux = Platform::parse("Linux x86_64\n").unwrap();
        assert_eq!(
            release_url("https://x/v{version}/moebius-{target}", &linux).unwrap(),
            format!(
                "https://x/v{}/moebius-x86_64-unknown-linux-gnu",
                env!("CARGO_PKG_VERSION")
            )
        );
        let other = Platform::parse("FreeBSD amd64").unwrap();
        assert!(other.target().unwrap_err().contains("FreeBSD amd64"));
        assert!(Platform::parse("").is_err());
    }

    #[test]
    fn a_failure_names_the_last_line_it_said() {
        let output = Output {
            ok: false,
            stdout: String::new(),
            stderr: "warning\nPermission denied\n".into(),
        };
        assert_eq!(
            failure("cannot connect", &output),
            "cannot connect: Permission denied"
        );
    }

    #[test]
    fn a_word_is_quoted_for_a_posix_shell() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn only_the_marked_lines_of_a_chatty_host_are_read() {
        let found = Found::parse(
            "Welcome to build\nLinux 6.1 not the system\nMOEBIUS-SYSTEM Linux x86_64\nMOEBIUS-PATH /usr/bin/moebius\n\
             MOEBIUS-VERSION moebius 0.7.0\nMOEBIUS-PORT 7200\nMOEBIUS-HEALTH 200\nMOEBIUS-UNIT moebius.service\n\
             MOEBIUS-SYSTEMD yes\nMOEBIUS-TOKEN-AT /h/.moebius/server/.moebius/token\nMOEBIUS-PATHX nothing\n",
        );
        assert_eq!(found.uname, "Linux x86_64");
        assert_eq!(found.binary.as_deref(), Some("/usr/bin/moebius"));
        assert_eq!(found.port, 7200);
        assert!(found.answers && found.systemd);
        assert_eq!(found.units, ["moebius.service"]);
        assert_eq!(found.tokens, ["/h/.moebius/server/.moebius/token"]);
        // Nothing marked: nothing found.
        let nothing = Found::parse("hello\n");
        assert!(nothing.uname.is_empty() && nothing.binary.is_none() && !nothing.answers);
        assert_eq!(nothing.port, 7171);
    }

    #[test]
    fn an_older_program_is_told_from_a_newer_one() {
        let found = |version: &str| Found {
            version: Some(version.to_string()),
            ..Found::default()
        };
        assert!(found("moebius 0.6.9").is_older_than("0.7.0"));
        assert!(found("moebius 0.7.0-rc1").is_older_than("0.7.1"));
        assert!(!found("moebius 0.7.0").is_older_than("0.7.0"));
        assert!(!found("moebius 0.10.0").is_older_than("0.9.0"));
        assert!(!found("moebius").is_older_than("0.7.0"));
        assert!(!Found::default().is_older_than("0.7.0"));
    }

    #[test]
    fn the_hash_of_a_release_is_found_in_the_list_by_the_name_of_the_file() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!("{a}  moebius-1.tar.gz\n{b} *moebius-x86_64\nshort  moebius-y\n");
        assert_eq!(sum_of(&sums, "moebius-x86_64"), Some("b".repeat(64)));
        assert_eq!(sum_of(&sums, "moebius-1.tar.gz"), Some(a));
        assert_eq!(sum_of(&sums, "moebius-y"), None);
        assert_eq!(sum_of(&sums, "moebius"), None);
        assert_eq!(
            hex_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
