//! Runners: the machines that run a card's harness over SSH (`[[runners]]`). `Runners` holds them and how many runs
//! each has going; `session` is one run on one of them: its branch pushed to a clone in the machine's working
//! directory, the harness started there over SSH, the router reached through a reverse tunnel, and the commits fetched
//! back when the run ends. Both harnesses (`pibolt`, `claude`) start their process through it when the run's agent
//! names a runner.

mod session;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::config::RunnerConfig;

pub(in crate::harness) use session::{Local, RemoteRun};

/// The runners of the configuration, and how many runs go on each.
#[derive(Default)]
pub struct Runners {
    configs: BTreeMap<String, RunnerConfig>,
    busy: Mutex<BTreeMap<String, u32>>,
}

/// A runner as the API lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunnerInfo {
    pub name: String,
    pub host: String,
    pub user: Option<String>,
    pub max_runs: u32,
    /// How many runs go on it now.
    pub busy: u32,
}

impl Runners {
    pub fn new(configs: Vec<RunnerConfig>) -> Self {
        Self {
            configs: configs
                .into_iter()
                .map(|config| (config.name.clone(), config))
                .collect(),
            busy: Mutex::new(BTreeMap::new()),
        }
    }

    /// The runners by name, with how many runs go on each.
    pub fn list(&self) -> Vec<RunnerInfo> {
        let busy = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        self.configs
            .values()
            .map(|config| RunnerInfo {
                name: config.name.clone(),
                host: config.host.clone(),
                user: config.user.clone(),
                max_runs: config.max_runs,
                busy: busy.get(&config.name).copied().unwrap_or(0),
            })
            .collect()
    }

    /// Whether a runner of this name is configured.
    pub fn has(&self, name: &str) -> bool {
        self.configs.contains_key(name)
    }

    /// Takes one of the runs `name` may have going, or says why it cannot: there is no such runner, or it is full.
    /// The slot is given back when it is dropped.
    pub(in crate::harness) fn claim(self: &Arc<Self>, name: &str) -> Result<Slot, String> {
        let config = self
            .configs
            .get(name)
            .ok_or_else(|| format!("there is no runner named {name}"))?;
        let mut busy = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        let count = busy.entry(name.to_string()).or_insert(0);
        if *count >= config.max_runs {
            return Err(format!(
                "the runner {name} is busy: {count} of {} runs are going",
                config.max_runs
            ));
        }
        *count += 1;
        Ok(Slot {
            runners: self.clone(),
            name: name.to_string(),
        })
    }

    fn config(&self, name: &str) -> Option<&RunnerConfig> {
        self.configs.get(name)
    }
}

/// One of a runner's runs, held while the run goes on.
pub(in crate::harness) struct Slot {
    runners: Arc<Runners>,
    name: String,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut busy = self.runners.busy.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = busy.get_mut(&self.name) {
            *count = count.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runner(name: &str, max_runs: u32) -> RunnerConfig {
        RunnerConfig {
            name: name.into(),
            host: "box".into(),
            user: None,
            port: None,
            key_file: None,
            working_dir: "/srv/moebius".into(),
            max_runs,
            bin_dir: None,
            ssh: "ssh".into(),
        }
    }

    #[test]
    fn a_runner_takes_only_as_many_runs_as_it_may_have() {
        let runners = Arc::new(Runners::new(vec![runner("box", 2)]));
        let first = runners.claim("box").unwrap();
        let _second = runners.claim("box").unwrap();
        let busy = runners.claim("box").err().unwrap();
        assert!(busy.contains("busy"), "{busy}");
        assert_eq!(runners.list()[0].busy, 2);
        drop(first);
        assert_eq!(runners.list()[0].busy, 1);
        assert!(runners.claim("box").is_ok());
        assert!(runners.claim("nobody").err().unwrap().contains("no runner"));
        assert!(runners.has("box") && !runners.has("nobody"));
    }
}
