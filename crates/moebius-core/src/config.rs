//! The server's configuration, read from `moebius.toml`. Every key has a default, so an empty file is a valid one.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The commented example `moebius init` writes. Every value in it is the default.
pub const EXAMPLE: &str = r#"# Moebius server configuration. Every key is optional; the values below are the defaults.

# Where Moebius keeps its store, blobs, clones and worktrees.
data_dir = ".moebius"

# The address the desktop app connects to. Keep it on a private network (ADR 0003).
listen = "127.0.0.1:7171"

# The file holding the token the desktop app authenticates with; relative paths are inside data_dir.
token_file = "token"

# How many runs may work at the same time, across all boards.
run_limit = 3

# When the daily Retro runs, as HH:MM in the server's local time.
retro_at = "23:30"

# The daily stretch in which trials, replays and autoresearch may run without asking.
[trial_window]
start = "00:00"
end = "06:00"
"#;

/// Why a configuration could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a valid configuration: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{key} must be a time as HH:MM, not {value:?}")]
    Time { key: &'static str, value: String },
    #[error("run_limit must be at least 1")]
    RunLimit,
}

/// A time of day as minutes after midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfDay(pub u16);

impl TimeOfDay {
    fn parse(key: &'static str, value: &str) -> Result<Self, ConfigError> {
        let bad = || ConfigError::Time {
            key,
            value: value.to_string(),
        };
        let (h, m) = value.split_once(':').ok_or_else(bad)?;
        if h.len() != 2 || m.len() != 2 {
            return Err(bad());
        }
        let (h, m): (u16, u16) = (h.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
        if h > 23 || m > 59 {
            return Err(bad());
        }
        Ok(Self(h * 60 + m))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWindow {
    start: Option<String>,
    end: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    data_dir: Option<PathBuf>,
    listen: Option<SocketAddr>,
    token_file: Option<PathBuf>,
    run_limit: Option<u32>,
    retro_at: Option<String>,
    trial_window: Option<RawWindow>,
}

/// The server's configuration, with every default applied and every path made absolute against the file's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub data_dir: PathBuf,
    pub listen: SocketAddr,
    pub token_file: PathBuf,
    pub run_limit: u32,
    pub retro_at: TimeOfDay,
    pub trial_window: (TimeOfDay, TimeOfDay),
}

impl Config {
    /// Parses a configuration whose relative paths are resolved against `base`.
    pub fn parse(text: &str, base: &Path, origin: &Path) -> Result<Self, ConfigError> {
        let raw: Raw = toml::from_str(text).map_err(|source| ConfigError::Parse {
            path: origin.to_path_buf(),
            source,
        })?;
        let data_dir = base.join(raw.data_dir.unwrap_or_else(|| PathBuf::from(".moebius")));
        let token_file = data_dir.join(raw.token_file.unwrap_or_else(|| PathBuf::from("token")));
        let run_limit = raw.run_limit.unwrap_or(3);
        if run_limit == 0 {
            return Err(ConfigError::RunLimit);
        }
        let window = raw.trial_window.unwrap_or(RawWindow {
            start: None,
            end: None,
        });
        Ok(Self {
            listen: raw
                .listen
                .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 7171))),
            token_file,
            run_limit,
            retro_at: TimeOfDay::parse("retro_at", raw.retro_at.as_deref().unwrap_or("23:30"))?,
            trial_window: (
                TimeOfDay::parse(
                    "trial_window.start",
                    window.start.as_deref().unwrap_or("00:00"),
                )?,
                TimeOfDay::parse("trial_window.end", window.end.as_deref().unwrap_or("06:00"))?,
            ),
            data_dir,
        })
    }

    /// Reads `path`, or uses every default when the file does not exist.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, &base, path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::parse("", &base, path),
            Err(source) => Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::parse(text, Path::new("/srv"), Path::new("/srv/moebius.toml"))
    }

    #[test]
    fn an_empty_file_gives_every_default() {
        let c = parse("").unwrap();
        assert_eq!(c.data_dir, PathBuf::from("/srv/.moebius"));
        assert_eq!(c.listen, "127.0.0.1:7171".parse().unwrap());
        assert_eq!(c.token_file, PathBuf::from("/srv/.moebius/token"));
        assert_eq!(c.run_limit, 3);
        assert_eq!(c.retro_at, TimeOfDay(23 * 60 + 30));
        assert_eq!(c.trial_window, (TimeOfDay(0), TimeOfDay(6 * 60)));
    }

    #[test]
    fn the_example_is_exactly_the_defaults() {
        assert_eq!(parse(EXAMPLE).unwrap(), parse("").unwrap());
    }

    #[test]
    fn every_key_can_be_set() {
        let c = parse(
            "data_dir = \"/var/lib/moebius\"\nlisten = \"10.0.0.2:9000\"\ntoken_file = \"/etc/moebius/token\"\n\
             run_limit = 8\nretro_at = \"22:05\"\n[trial_window]\nstart = \"01:00\"\nend = \"05:30\"\n",
        )
        .unwrap();
        assert_eq!(c.data_dir, PathBuf::from("/var/lib/moebius"));
        assert_eq!(c.listen, "10.0.0.2:9000".parse().unwrap());
        assert_eq!(c.token_file, PathBuf::from("/etc/moebius/token"));
        assert_eq!(c.run_limit, 8);
        assert_eq!(c.retro_at, TimeOfDay(22 * 60 + 5));
        assert_eq!(c.trial_window, (TimeOfDay(60), TimeOfDay(5 * 60 + 30)));
    }

    #[test]
    fn bad_values_are_refused_with_the_key_named() {
        assert!(matches!(
            parse("retro_at = \"25:00\""),
            Err(ConfigError::Time {
                key: "retro_at",
                ..
            })
        ));
        assert!(matches!(parse("run_limit = 0"), Err(ConfigError::RunLimit)));
        assert!(matches!(
            parse("unknown = 1"),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn a_missing_file_means_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::load(&dir.path().join("moebius.toml")).unwrap();
        assert_eq!(c.data_dir, dir.path().join(".moebius"));
    }
}
