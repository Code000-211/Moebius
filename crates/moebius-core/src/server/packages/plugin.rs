//! Installing, updating and removing a plugin package: its program is downloaded to
//! `<data_dir>/packages/<name>/<version>/<program>` after the sha256 is checked, its `[[plugins]]` entry is written
//! through the live settings, and the plugin host starts it, so that nothing needs a restart but a launcher, which the
//! server reads at its start. The program is not run before it is in place and its entry is written.

use std::path::{Path, PathBuf};

use moebius_plugin_api::{Process, ProcessStatus};
use serde_json::{Value, json};

use super::manifest::PluginSpec;
use super::{Chosen, Locked, Outcome, PackageError, Packages, Result, download, packages_dir};
use crate::bus::Event;
use crate::config::PluginConfig;
use crate::decide::Actor;
use crate::server::plugins;
use crate::server::settings::{Changes, read_raw};

/// The target triple of this build of Moebius, which names the program of a plugin to download.
pub fn current_target() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "linux" => format!("{arch}-unknown-linux-gnu"),
        "macos" => format!("{arch}-apple-darwin"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        other => format!("{arch}-{other}"),
    }
}

/// A plugin entry of the configuration file, as far as the package manager needs it.
struct Entry {
    /// Its position among the `[[plugins]]`.
    index: usize,
    args: Vec<String>,
    launches: bool,
}

impl Packages<'_> {
    fn spec<'p>(&self, chosen: &'p Chosen) -> Result<&'p PluginSpec> {
        chosen.package.plugin.as_ref().ok_or_else(|| {
            PackageError::Index(format!("{} has no [plugin] table", chosen.package.name))
        })
    }

    /// The launcher of the file that takes every agent (`launches` without `launch_agents`), if there is one: a launcher
    /// added after it would never be asked, and the file with both is refused.
    fn launcher_for_every_agent(&self) -> Result<Option<String>> {
        let (_, raw) = read_raw(&self.live.get().file)?;
        let plugins = raw.get("plugins").and_then(|plugins| plugins.as_array());
        Ok(plugins.into_iter().flatten().find_map(|plugin| {
            let launches = plugin
                .get("launches")
                .and_then(|launches| launches.as_bool());
            let agents = plugin
                .get("launch_agents")
                .and_then(|agents| agents.as_array());
            (launches == Some(true) && agents.is_none_or(|agents| agents.is_empty()))
                .then(|| {
                    plugin
                        .get("name")
                        .and_then(|name| name.as_str())
                        .map(str::to_string)
                })
                .flatten()
        }))
    }

    /// Where `name` is among the `[[plugins]]` of the file, if it is there, and how many entries there are.
    fn entry(&self, name: &str) -> Result<(Option<Entry>, usize)> {
        let (_, raw) = read_raw(&self.live.get().file)?;
        let plugins = raw
            .get("plugins")
            .and_then(|plugins| plugins.as_array())
            .cloned()
            .unwrap_or_default();
        let found = plugins
            .iter()
            .position(|plugin| plugin.get("name").and_then(|n| n.as_str()) == Some(name))
            .map(|index| Entry {
                index,
                args: plugins[index]
                    .get("args")
                    .and_then(|args| args.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|arg| arg.as_str().map(str::to_string))
                    .collect(),
                launches: plugins[index]
                    .get("launches")
                    .and_then(|launches| launches.as_bool())
                    .unwrap_or(false),
            });
        Ok((found, plugins.len()))
    }

    /// Downloads the program of `chosen` for this target to its folder and gives its path.
    fn download_program(
        &self,
        chosen: &Chosen,
        spec: &PluginSpec,
    ) -> Result<(PathBuf, String, String)> {
        let package = &chosen.package;
        let target = current_target();
        let download_for = package
            .downloads
            .iter()
            .find(|d| d.target == target)
            .ok_or_else(|| PackageError::NoBuild {
                name: package.name.clone(),
                version: package.version.clone(),
                target,
            })?;
        let bytes = download::fetch(&download_for.url, &download_for.sha256)?;
        let folder = packages_dir(&self.data_dir())
            .join(&package.name)
            .join(&package.version);
        let program = folder.join(&spec.program);
        std::fs::create_dir_all(&folder)?;
        let written = crate::server::settings::write_atomically(&program, &bytes, false)
            .and_then(|()| crate::platform::make_executable(&program));
        if let Err(e) = written {
            let _ = std::fs::remove_dir_all(&folder);
            return Err(e.into());
        }
        Ok((
            program,
            download_for.url.clone(),
            download_for.sha256.to_lowercase(),
        ))
    }

    fn start(
        &self,
        name: &str,
        program: &Path,
        spec: &PluginSpec,
        args: Vec<String>,
    ) -> Result<Process> {
        let plugin = PluginConfig {
            name: name.to_string(),
            path: program.to_path_buf(),
            args,
            decides: spec.decides.clone(),
            launches: spec.launches || spec.kind == "launcher",
            // Which agents it is asked about is the launcher's, not the host's, to decide.
            launch_agents: Vec::new(),
        };
        plugins::start(&self.live.get(), &plugin, self.app.decisions.host().limit())
            .map_err(PackageError::Refused)
    }

    /// Makes the host answer for `name` with `process`, and the deciders it names decide their kinds.
    fn put_in_service(&self, name: &str, process: Process, decides: &[String]) {
        let decisions = &self.app.decisions;
        decisions.host().set_process(process);
        decisions.unroute(name);
        for kind in decides {
            decisions.route(kind, name);
        }
    }

    /// The plugin's status once it is in the host.
    fn status(&self, name: &str) -> Option<ProcessStatus> {
        self.app.decisions.host().status(name)
    }

    pub(super) fn install_plugin(&self, by: &Actor, chosen: &Chosen) -> Result<Outcome> {
        let (package, spec) = (&chosen.package, self.spec(chosen)?);
        let name = &package.name;
        if self.entry(name)?.0.is_some() {
            return Err(PackageError::Refused(format!(
                "moebius.toml has a [[plugins]] entry named {name} already; remove it first"
            )));
        }
        if self
            .app
            .decisions
            .host()
            .manifests()
            .iter()
            .any(|held| held.name == *name)
        {
            return Err(PackageError::Refused(format!(
                "{name} is the name of a plugin that is part of Moebius; a package cannot take its place"
            )));
        }
        let launches = spec.launches || spec.kind == "launcher";
        // A package is added last, as a launcher for every agent; after another such launcher it would never be asked.
        if launches && let Some(other) = self.launcher_for_every_agent()? {
            return Err(PackageError::Refused(format!(
                "{name} is a launcher for every agent, but {other} already is one, and the second would never be asked; \
                 give {other} launch_agents in moebius.toml (or remove it) first"
            )));
        }
        let (program, url, sha256) = self.download_program(chosen, spec)?;
        let (_, count) = self.entry(name)?;
        let mut entry = json!({ "name": name, "path": program.display().to_string(), "args": [] });
        if !spec.decides.is_empty() {
            entry["decides"] = json!(spec.decides);
        }
        if launches {
            entry["launches"] = Value::Bool(true);
        }
        let changes = Changes {
            set: [(format!("plugins.{count}"), entry)].into(),
            remove: vec![],
        };
        if let Err(e) = self.live.save(self.app, by, &changes) {
            let _ = std::fs::remove_dir_all(packages_dir(&self.data_dir()).join(name));
            return Err(e.into());
        }
        let process = self.start(name, &program, spec, vec![])?;
        self.put_in_service(name, process, &spec.decides);
        let locked = Locked {
            name: name.clone(),
            version: package.version.clone(),
            kind: package.kind.clone(),
            registry: chosen.registry.clone(),
            url,
            sha256,
            program: Some(program.display().to_string()),
            repository: None,
            change: None,
            files: vec![],
            rules: vec![],
        };
        self.finish(by, "install_package", locked, "installed", launches)
    }

    /// Puts `locked` in the lock, records the decision and publishes the event.
    pub(super) fn finish(
        &self,
        by: &Actor,
        action: &str,
        locked: Locked,
        done: &'static str,
        restart_needed: bool,
    ) -> Result<Outcome> {
        let mut lock = self.read_lock()?;
        lock.put(locked.clone());
        lock.write(&self.data_dir())?;
        self.record(by, action, &locked)?;
        self.app.bus.publish(Event::PackageInstalled {
            name: locked.name.clone(),
            version: locked.version.clone(),
            kind: locked.kind.clone(),
        });
        self.resolve_updates(&locked.name)?;
        Ok(Outcome {
            plugin: self.status(&locked.name),
            leftover: None,
            change: locked.change,
            name: locked.name,
            version: locked.version,
            kind: locked.kind,
            action: done,
            restart_needed,
        })
    }

    pub(super) fn update_plugin(
        &self,
        by: &Actor,
        old: &Locked,
        chosen: &Chosen,
    ) -> Result<Outcome> {
        let (package, spec) = (&chosen.package, self.spec(chosen)?);
        let name = &package.name;
        let (program, url, sha256) = self.download_program(chosen, spec)?;
        let discard = |e: PackageError| {
            let _ = std::fs::remove_dir_all(
                packages_dir(&self.data_dir())
                    .join(name)
                    .join(&package.version),
            );
            e
        };
        let (entry, count) = self.entry(name).map_err(discard)?;
        let args = entry.as_ref().map(|e| e.args.clone()).unwrap_or_default();
        // The new version must run before it replaces the one that does.
        let process = self.start(name, &program, spec, args).map_err(discard)?;
        if let Some(status) = process
            .status()
            .error
            .filter(|_| process.status().state != moebius_plugin_api::State::Running)
        {
            drop(process);
            return Err(discard(PackageError::Refused(format!(
                "{name} {} does not start, so {} stays: {status}",
                package.version, old.version
            ))));
        }
        let launches = spec.launches || spec.kind == "launcher";
        let changes = match &entry {
            Some(entry) => {
                let at = |key: &str| format!("plugins.{}.{key}", entry.index);
                let mut changes = Changes {
                    set: [(at("path"), Value::String(program.display().to_string()))].into(),
                    remove: vec![],
                };
                // The new version decides the kinds and launches as it says, so a restart routes the same.
                if spec.decides.is_empty() {
                    changes.remove.push(at("decides"));
                } else {
                    changes.set.insert(at("decides"), json!(spec.decides));
                }
                if launches {
                    changes.set.insert(at("launches"), Value::Bool(true));
                } else {
                    changes.remove.push(at("launches"));
                }
                changes
            }
            None => Changes {
                set: [(
                    format!("plugins.{count}"),
                    json!({ "name": name, "path": program.display().to_string(), "args": [],
                            "decides": spec.decides, "launches": launches }),
                )]
                .into(),
                remove: vec![],
            },
        };
        self.live
            .save(self.app, by, &changes)
            .map_err(|e| discard(e.into()))?;
        self.put_in_service(name, process, &spec.decides);
        let _ =
            std::fs::remove_dir_all(packages_dir(&self.data_dir()).join(name).join(&old.version));
        let mut locked = old.clone();
        locked.version = package.version.clone();
        locked.registry = chosen.registry.clone();
        locked.url = url;
        locked.sha256 = sha256;
        locked.program = Some(program.display().to_string());
        self.finish(by, "update_package", locked, "updated", false)
    }

    pub(super) fn remove_plugin(&self, by: &Actor, locked: &Locked) -> Result<Outcome> {
        let name = &locked.name;
        let (entry, _) = self.entry(name)?;
        let launches = entry.as_ref().is_some_and(|entry| entry.launches);
        if let Some(entry) = entry {
            let changes = Changes {
                set: Default::default(),
                remove: vec![format!("plugins.{}", entry.index)],
            };
            self.live.save(self.app, by, &changes)?;
        }
        self.app.decisions.unroute(name);
        self.app.decisions.host().remove(name);
        let mut lock = self.read_lock()?;
        lock.remove(name);
        lock.write(&self.data_dir())?;
        self.record(by, "remove_package", locked)?;
        self.app.bus.publish(Event::PackageRemoved {
            name: name.clone(),
            kind: locked.kind.clone(),
        });
        self.resolve_updates(name)?;
        // The files go last and without failing the removal, which is done: a program that has not ended yet can
        // keep its folder on Windows.
        let folder = packages_dir(&self.data_dir()).join(name);
        let leftover = (folder.exists() && std::fs::remove_dir_all(&folder).is_err())
            .then(|| folder.display().to_string());
        Ok(Outcome {
            name: name.clone(),
            version: locked.version.clone(),
            kind: locked.kind.clone(),
            action: "removed",
            restart_needed: launches,
            change: None,
            plugin: None,
            leftover,
        })
    }
}
