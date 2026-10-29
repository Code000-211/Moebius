//! The hosts of an OpenSSH client configuration (`~/.ssh/config`): reading the `Host` names a person can pick, with
//! `Include` and wildcards, and writing the `Host` entry for a new host from `user@host` or a whole `ssh …` command.

use std::path::{Path, PathBuf};

/// How deep `Include` goes before the reader stops, so that a file including itself ends.
const INCLUDE_DEPTH: usize = 8;

/// A host a person can pick: the name to give `ssh`, and what the configuration says of where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<String>,
}

impl Host {
    /// `user@hostname:port` as far as the configuration says, for the line under the name in the list.
    pub fn detail(&self) -> String {
        let mut line = String::new();
        if let Some(user) = &self.user {
            line.push_str(user);
            line.push('@');
        }
        line.push_str(self.hostname.as_deref().unwrap_or(&self.name));
        if let Some(port) = &self.port {
            line.push(':');
            line.push_str(port);
        }
        line
    }
}

/// The configuration file `ssh` reads for a person: `~/.ssh/config`.
pub fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("config"))
}

/// Whether `pattern` names one host, not a set of them.
fn is_literal(pattern: &str) -> bool {
    !pattern.is_empty() && !pattern.contains(['*', '?', '!'])
}

/// The hosts `file` names, in order, with those of the files it includes; a file that cannot be read has none.
/// Relative `Include` paths are relative to the directory of the first file, which is `~/.ssh` for the user's own.
pub fn hosts(file: &Path) -> Vec<Host> {
    let base = file.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut found: Vec<Host> = Vec::new();
    read(file, &base, 0, &mut found);
    let mut seen = std::collections::HashSet::new();
    found.retain(|host| seen.insert(host.name.clone()));
    found
}

fn read(file: &Path, base: &Path, depth: usize, found: &mut Vec<Host>) {
    if depth > INCLUDE_DEPTH {
        return;
    }
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    // The names of the current `Host` line that are literal, and the index in `found` of the first of them.
    let mut current: Option<usize> = None;
    let mut count = 0;
    for line in text.lines() {
        let Some((key, value)) = split_line(line) else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                current = Some(found.len());
                count = 0;
                for name in value.split_whitespace().filter(|n| is_literal(n)) {
                    found.push(Host {
                        name: name.to_string(),
                        hostname: None,
                        user: None,
                        port: None,
                    });
                    count += 1;
                }
            }
            "match" => current = None,
            "include" => {
                for pattern in value.split_whitespace() {
                    for path in expand_include(pattern, base) {
                        read(&path, base, depth + 1, found);
                    }
                }
            }
            key @ ("hostname" | "user" | "port") => {
                if let Some(start) = current {
                    for host in &mut found[start..start + count] {
                        let slot = match key {
                            "hostname" => &mut host.hostname,
                            "user" => &mut host.user,
                            _ => &mut host.port,
                        };
                        slot.get_or_insert_with(|| value.trim_matches('"').to_string());
                    }
                }
            }
            _ => {}
        }
    }
}

/// `Key value` or `Key=value` of a line; none for a comment or an empty line.
fn split_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let at = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let (key, rest) = line.split_at(at);
    let value = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '=');
    let value = value.trim();
    (!value.is_empty()).then_some((key, value))
}

/// The files an `Include` pattern names: `~` is the home directory, a relative path is under `base`, and `*` and `?`
/// match within the last part of the path.
fn expand_include(pattern: &str, base: &Path) -> Vec<PathBuf> {
    let pattern = pattern.trim_matches('"');
    let path = match pattern.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().map(|home| home.join(rest)),
        None => Some(PathBuf::from(pattern)),
    };
    let Some(path) = path else {
        return Vec::new();
    };
    let path = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return Vec::new();
    };
    if !name.contains(['*', '?']) {
        return vec![path];
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut matches: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| glob(&name, &entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    matches.sort();
    matches
}

/// Whether `text` matches `pattern`, where `*` stands for any run of characters and `?` for one.
pub fn glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some(('*', rest)) => (0..=t.len()).any(|skip| go(rest, &t[skip..])),
            Some(('?', rest)) => !t.is_empty() && go(rest, &t[1..]),
            Some((c, rest)) => t.first() == Some(c) && go(rest, &t[1..]),
        }
    }
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    go(&p, &t)
}

/// What the person typed to add a host: `user@host`, or a whole `ssh` command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewHost {
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<String>,
    pub identity: Option<String>,
    pub jump: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddError {
    #[error("enter user@host or an ssh command")]
    Empty,
    #[error("{0} is already in the configuration")]
    Exists(String),
    #[error("cannot write {path}: {message}")]
    Write { path: String, message: String },
}

impl NewHost {
    /// Reads `user@host`, `host`, or `ssh [-p port] [-i key] [-J jump] [-l user] [user@]host`.
    pub fn parse(typed: &str) -> Result<NewHost, AddError> {
        let mut words = typed.split_whitespace().peekable();
        if words.peek() == Some(&"ssh") {
            words.next();
        }
        let mut host = NewHost::default();
        let mut target: Option<&str> = None;
        while let Some(word) = words.next() {
            let value = |words: &mut std::iter::Peekable<std::str::SplitWhitespace<'_>>| {
                words.next().map(str::to_string)
            };
            match word {
                "-p" => host.port = value(&mut words),
                "-i" => host.identity = value(&mut words),
                "-J" => host.jump = value(&mut words),
                "-l" => host.user = value(&mut words),
                flag if flag.starts_with('-') => {}
                _ if target.is_none() => target = Some(word),
                // What follows the host is a command to run there; a host is added, not a command.
                _ => break,
            }
        }
        let target = target.ok_or(AddError::Empty)?;
        match target.rsplit_once('@') {
            Some((user, name)) => {
                host.user = Some(user.to_string());
                host.hostname = name.to_string();
            }
            None => host.hostname = target.to_string(),
        }
        if host.hostname.is_empty() {
            return Err(AddError::Empty);
        }
        Ok(host)
    }

    /// The `Host` block, named by the host as typed.
    pub fn entry(&self) -> String {
        let mut text = format!("Host {}\n    HostName {}\n", self.hostname, self.hostname);
        for (key, value) in [
            ("User", &self.user),
            ("Port", &self.port),
            ("IdentityFile", &self.identity),
            ("ProxyJump", &self.jump),
        ] {
            if let Some(value) = value {
                text.push_str(&format!("    {key} {value}\n"));
            }
        }
        text
    }
}

/// Adds the host to the end of the configuration `file` as VS Code does, and returns the name to connect with.
pub fn add_host(file: &Path, typed: &str) -> Result<String, AddError> {
    let new = NewHost::parse(typed)?;
    if hosts(file).iter().any(|host| host.name == new.hostname) {
        return Err(AddError::Exists(new.hostname));
    }
    let write = |e: std::io::Error| AddError::Write {
        path: file.display().to_string(),
        message: e.to_string(),
    };
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(write)?;
    }
    let mut text = std::fs::read_to_string(file).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&new.entry());
    std::fs::write(file, text).map_err(write)?;
    Ok(new.hostname)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(hosts: &[Host]) -> Vec<&str> {
        hosts.iter().map(|h| h.name.as_str()).collect()
    }

    #[test]
    fn hosts_are_the_literal_names_of_host_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config");
        std::fs::write(
            &file,
            "# mine\nHost *\n  ServerAliveInterval 30\n\nHost build ci.example !bad *.internal\n  HostName 10.0.0.7\n  User deploy\n  Port=2222\n\
             host lower\n  hostname lower.example\nMatch host x\n  User nobody\nHost build\n  HostName ignored\n",
        )
        .unwrap();
        let found = hosts(&file);
        assert_eq!(names(&found), ["build", "ci.example", "lower"]);
        assert_eq!(found[0].detail(), "deploy@10.0.0.7:2222");
        assert_eq!(found[1].detail(), "deploy@10.0.0.7:2222");
        assert_eq!(found[2].detail(), "lower.example");
    }

    #[test]
    fn include_reads_files_by_path_and_wildcard_and_ends_on_a_loop() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("config.d");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("a.conf"), "Host from-a\n").unwrap();
        std::fs::write(sub.join("b.conf"), "Host from-b\n").unwrap();
        std::fs::write(sub.join("skip.txt"), "Host skipped\n").unwrap();
        std::fs::write(dir.path().join("one"), "Host single\nInclude config\n").unwrap();
        let file = dir.path().join("config");
        std::fs::write(&file, "Include config.d/*.conf one missing\nHost last\n").unwrap();
        assert_eq!(names(&hosts(&file)), ["from-a", "from-b", "single", "last"]);
    }

    #[test]
    fn a_missing_file_has_no_hosts() {
        assert!(hosts(Path::new("/nonexistent/ssh/config")).is_empty());
    }

    #[test]
    fn wildcards_match_runs_and_single_characters() {
        assert!(glob("*.conf", "a.conf"));
        assert!(glob("a?c", "abc"));
        assert!(!glob("a?c", "ac"));
        assert!(!glob("*.conf", "a.txt"));
    }

    #[test]
    fn a_host_is_read_from_user_at_host_or_a_whole_command() {
        let plain = NewHost::parse("leon@build.example").unwrap();
        assert_eq!(plain.hostname, "build.example");
        assert_eq!(plain.user.as_deref(), Some("leon"));
        let command =
            NewHost::parse("ssh -p 2222 -i ~/.ssh/key -J jump.example leon@10.0.0.7 uptime")
                .unwrap();
        assert_eq!(command.hostname, "10.0.0.7");
        assert_eq!(command.port.as_deref(), Some("2222"));
        assert_eq!(command.identity.as_deref(), Some("~/.ssh/key"));
        assert_eq!(command.jump.as_deref(), Some("jump.example"));
        assert_eq!(NewHost::parse("  "), Err(AddError::Empty));
        assert_eq!(NewHost::parse("ssh -p 22"), Err(AddError::Empty));
    }

    #[test]
    fn adding_a_host_appends_an_entry_and_does_not_repeat_one() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".ssh").join("config");
        let name = add_host(&file, "ssh -p 2200 leon@build.example").unwrap();
        assert_eq!(name, "build.example");
        let written = std::fs::read_to_string(&file).unwrap();
        assert_eq!(
            written,
            "Host build.example\n    HostName build.example\n    User leon\n    Port 2200\n"
        );
        add_host(&file, "other").unwrap();
        let found = hosts(&file);
        assert_eq!(names(&found), ["build.example", "other"]);
        assert_eq!(found[0].detail(), "leon@build.example:2200");
        assert_eq!(
            add_host(&file, "leon@other"),
            Err(AddError::Exists("other".into()))
        );
    }
}
