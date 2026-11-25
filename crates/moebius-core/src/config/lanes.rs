//! The test lanes of the server (`[[lanes]]`, ADR 0035): named machines a change is built, tested and measured on, each
//! the server itself or one of the `[[runners]]`. The configuration says where a lane is, how its jobs are isolated and
//! what to say of the machine in a report; which repositories use which lane is a setting of each repository.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{ConfigError, RunnerConfig, valid_agent_name};

/// How a lane keeps what it runs away from the machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    /// Every job runs in a fresh container: no network, no credentials, an unprivileged user, a time limit and a read-only
    /// image, with the GPUs passed through.
    #[default]
    Always,
    /// A job for one of Moebius's own changes runs directly on the machine; a job for a pull request someone else opened
    /// always runs in the container.
    ExternalOnly,
    /// Every job runs directly on the machine.
    Off,
}

impl Isolation {
    /// The word the configuration and the API use.
    pub fn as_str(self) -> &'static str {
        match self {
            Isolation::Always => "always",
            Isolation::ExternalOnly => "external_only",
            Isolation::Off => "off",
        }
    }

    /// Whether a job runs in a container: `external` says the change is a pull request someone else opened.
    pub fn contained(self, external: bool) -> bool {
        match self {
            Isolation::Always => true,
            Isolation::ExternalOnly => external,
            Isolation::Off => false,
        }
    }

    /// The warning a lane that is not isolated everywhere shows to the owner, or none.
    pub fn warning(self) -> Option<&'static str> {
        match self {
            Isolation::Always => None,
            Isolation::ExternalOnly => Some(
                "Jobs for the server's own changes run directly on the machine, with the machine's files, network and \
                 credentials of the account they run as. Only jobs for pull requests someone else opened run in a container.",
            ),
            Isolation::Off => Some(
                "Every job runs directly on the machine, with the machine's files, network and credentials of the \
                 account they run as, including the code of pull requests someone else opened.",
            ),
        }
    }
}

/// A test lane, from `[[lanes]]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaneConfig {
    /// The name a repository's settings assign it by.
    pub name: String,
    /// The `[[runners]]` entry the lane is reached through (over SSH); without one the lane is the server itself.
    #[serde(default)]
    pub runner: Option<String>,
    /// Where the lane keeps its clones and the jobs' directories: absolute, on the machine. Without it, the runner's
    /// `working_dir` or the server's data directory, then `lanes/<name>`.
    #[serde(default)]
    pub work_dir: Option<String>,
    #[serde(default)]
    pub isolation: Isolation,
    /// The container program on the machine, `podman` or `docker` (or the path of one); looked up on its `PATH`.
    #[serde(default = "default_runtime")]
    pub runtime: String,
    /// The image a job runs in. Needed unless `isolation` is `off`.
    #[serde(default)]
    pub image: Option<String>,
    /// The device files passed through to the container; `/dev/kfd` and `/dev/dri` for ROCm.
    #[serde(default = "default_devices")]
    pub devices: Vec<String>,
    /// The groups the container's user is added to, which the devices need.
    #[serde(default = "default_groups")]
    pub groups: Vec<String>,
    /// The user a container runs as: unprivileged.
    #[serde(default = "default_user")]
    pub user: String,
    /// How long one job may take altogether, in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    /// What the report says of the machine.
    #[serde(default)]
    pub gpus: Option<String>,
    #[serde(default)]
    pub power_cap: Option<String>,
    #[serde(default)]
    pub rocm: Option<String>,
    #[serde(default)]
    pub driver: Option<String>,
}

/// The longest a job may be given, in seconds.
pub const LANE_TIMEOUT_AT_MOST: u64 = 86_400;

fn default_runtime() -> String {
    "podman".into()
}

fn default_devices() -> Vec<String> {
    vec!["/dev/kfd".into(), "/dev/dri".into()]
}

fn default_groups() -> Vec<String> {
    vec!["video".into(), "render".into()]
}

fn default_user() -> String {
    "65534:65534".into()
}

fn default_timeout() -> u64 {
    7200
}

/// A word that cannot be taken for an option or split by a shell.
fn is_word(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with('-')
        && !text.contains(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '\\' | '\0'))
}

/// The lanes, checked against the runners.
pub(super) fn lanes(
    lanes: Vec<LaneConfig>,
    runners: &[RunnerConfig],
) -> Result<Vec<LaneConfig>, ConfigError> {
    let mut names = BTreeSet::new();
    for lane in &lanes {
        let name = lane.name.clone();
        let bad = |what: &str| ConfigError::Lane {
            name: name.clone(),
            what: what.to_string(),
        };
        if !valid_agent_name(&name) || name.eq_ignore_ascii_case("local") {
            return Err(bad("is not a name (letters, digits, - and _; not `local`)"));
        }
        if !names.insert(name.to_lowercase()) {
            return Err(bad("is used by two lanes"));
        }
        if let Some(runner) = &lane.runner
            && !runners.iter().any(|r| &r.name == runner)
        {
            return Err(bad(&format!("runner {runner:?} is no `[[runners]]` name")));
        }
        if let Some(dir) = &lane.work_dir
            && (!dir.starts_with('/') || dir.contains(['\'', '"', '\n', '\0', '\\']))
        {
            return Err(bad(
                "work_dir must be an absolute path without quotes or line breaks",
            ));
        }
        if !is_word(&lane.runtime) {
            return Err(bad("runtime must be one word, `podman` or `docker`"));
        }
        if lane.isolation != Isolation::Off && lane.image.as_deref().is_none_or(|i| !is_word(i)) {
            return Err(bad(
                "image must name the container image of its jobs (one word), unless isolation is `off`",
            ));
        }
        if lane
            .devices
            .iter()
            .any(|d| !d.starts_with('/') || !is_word(d))
        {
            return Err(bad("devices are absolute device paths"));
        }
        if lane.groups.iter().any(|g| !is_word(g)) || !is_word(&lane.user) {
            return Err(bad("groups and user must be single words"));
        }
        if !(60..=LANE_TIMEOUT_AT_MOST).contains(&lane.timeout_seconds) {
            return Err(bad(&format!(
                "timeout_seconds must be from 60 to {LANE_TIMEOUT_AT_MOST}"
            )));
        }
    }
    Ok(lanes)
}

impl LaneConfig {
    /// What the report says of the machine's driver stack: `ROCm 6.2.1`, `Driver 24.30`, or both.
    pub fn stack(&self) -> String {
        let mut parts = Vec::new();
        if let Some(rocm) = &self.rocm {
            parts.push(format!("ROCm {rocm}"));
        }
        if let Some(driver) = &self.driver {
            parts.push(format!("Driver {driver}"));
        }
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::parse(
            text,
            std::path::Path::new("/srv"),
            std::path::Path::new("/srv/moebius.toml"),
        )
    }

    const RUNNER: &str =
        "[[runners]]\nname = \"box\"\nhost = \"gpu.test\"\nworking_dir = \"/srv/moebius\"\n";

    #[test]
    fn isolation_says_which_jobs_run_in_a_container() {
        for (isolation, own, outside) in [
            (Isolation::Always, true, true),
            (Isolation::ExternalOnly, false, true),
            (Isolation::Off, false, false),
        ] {
            assert_eq!(isolation.contained(false), own, "{isolation:?} own");
            assert_eq!(isolation.contained(true), outside, "{isolation:?} outside");
        }
        assert!(Isolation::Always.warning().is_none());
        assert!(Isolation::ExternalOnly.warning().is_some());
        assert!(
            Isolation::Off
                .warning()
                .unwrap()
                .contains("directly on the machine")
        );
    }

    #[test]
    fn a_lane_is_read_with_the_defaults_of_an_isolated_one() {
        let config = parse(&format!(
            "{RUNNER}[[lanes]]\nname = \"gpu1\"\nrunner = \"box\"\nimage = \"rocm/dev:6.2\"\ngpus = \"2x MI300X\"\nrocm = \"6.2.1\"\n[[lanes]]\nname = \"here\"\nisolation = \"off\"\n"
        ))
        .unwrap();
        let lane = &config.lanes[0];
        assert_eq!(lane.isolation, Isolation::Always);
        assert_eq!(lane.runtime, "podman");
        assert_eq!(lane.devices, ["/dev/kfd", "/dev/dri"]);
        assert_eq!(lane.user, "65534:65534");
        assert_eq!(lane.timeout_seconds, 7200);
        assert_eq!(lane.stack(), "ROCm 6.2.1");
        assert_eq!(config.lanes[1].runner, None);
        assert!(parse("").unwrap().lanes.is_empty());
    }

    #[test]
    fn a_lane_that_is_not_valid_is_refused_with_its_name() {
        let refused = |text: &str| parse(&format!("{RUNNER}{text}")).unwrap_err().to_string();
        let lane = |extra: &str| format!("[[lanes]]\nname = \"gpu1\"\nimage = \"x\"\n{extra}");
        assert!(refused(&lane("runner = \"nobody\"\n")).contains("no `[[runners]]` name"));
        assert!(refused(&lane("isolation = \"sometimes\"\n")).contains("isolation"));
        assert!(refused(&lane("timeout_seconds = 5\n")).contains("timeout_seconds"));
        assert!(refused(&lane("devices = [\"kfd\"]\n")).contains("devices"));
        assert!(refused(&lane("runtime = \"--privileged\"\n")).contains("runtime"));
        assert!(refused(&lane("typo = 1\n")).contains("typo"));
        assert!(refused("[[lanes]]\nname = \"gpu1\"\n").contains("image"));
        assert!(refused(&format!("{}{}", lane(""), lane(""))).contains("two lanes"));
        assert!(refused("[[lanes]]\nname = \"local\"\nimage = \"x\"\n").contains("not `local`"));
        // Without a container there is no image to name.
        assert!(parse("[[lanes]]\nname = \"gpu1\"\nisolation = \"off\"\n").is_ok());
    }
}
