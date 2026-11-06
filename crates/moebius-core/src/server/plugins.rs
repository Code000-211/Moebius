//! The plugins in their own process that `[[plugins]]` names: each is started at the server's start, added to the plugin
//! host and kept running by it (see `moebius_plugin_api::Process`). Its standard error, and a line each time it stops,
//! go with secrets redacted to its log, `<data_dir>/plugins/<name>.log`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use moebius_plugin_api::{Process, ProcessSpec};

use crate::blobs::redact;
use crate::config::{Config, PluginConfig};

/// The log of the plugin `name`.
pub fn log_path(data_dir: &Path, name: &str) -> PathBuf {
    data_dir.join("plugins").join(format!("{name}.log"))
}

/// Starts `plugin` and waits until it sent its manifest or failed to, at most `limit`. A plugin that fails is still
/// returned: the host refuses calls into it while it is started again.
pub fn start(config: &Config, plugin: &PluginConfig, limit: Duration) -> Result<Process, String> {
    let path = log_path(&config.data_dir, &plugin.name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let file = Mutex::new(file);
    let spec = ProcessSpec {
        name: plugin.name.clone(),
        program: plugin.path.clone(),
        args: plugin.args.clone(),
        dir: config.dir().to_path_buf(),
    };
    Ok(Process::start(spec, limit, move |line| {
        let stamp = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        let mut file = file.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = writeln!(file, "{stamp} {}", redact(line));
    }))
}

/// The `launch_agents` of the launchers that name no agent in `known`, each as "plugin: agent", for a warning at the
/// server's start: the match is by exact name, so a misspelt agent is wrapped by no launcher but the last.
pub fn unknown_launch_agents(
    plugins: &[PluginConfig],
    known: &std::collections::BTreeSet<String>,
) -> Vec<String> {
    plugins
        .iter()
        .flat_map(|plugin| {
            plugin
                .launch_agents
                .iter()
                .filter(|agent| !known.contains(*agent))
                .map(|agent| format!("{}: {agent}", plugin.name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launcher(name: &str, agents: &[&str]) -> PluginConfig {
        PluginConfig {
            name: name.into(),
            path: "p".into(),
            args: vec![],
            decides: vec![],
            launches: true,
            launch_agents: agents.iter().map(|agent| agent.to_string()).collect(),
        }
    }

    #[test]
    fn a_launch_agent_that_is_no_agent_is_named_exactly_as_it_is_written() {
        let known = ["Moebius", "Coder"].map(String::from).into();
        let plugins = [
            launcher("nono", &["Moebius", "moebius"]),
            launcher("podman", &[]),
        ];
        assert_eq!(unknown_launch_agents(&plugins, &known), ["nono: moebius"]);
        assert!(unknown_launch_agents(&plugins[1..], &known).is_empty());
    }
}
