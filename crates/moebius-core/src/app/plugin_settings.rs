//! The settings of plugins: each plugin may declare, in its manifest, settings a person gives for each repository
//! (`project_settings`, a sandbox's Dockerfile, say), which the web app shows on the repository's page, and settings for
//! the whole server (`app_settings`), which it shows on the Settings page. The values are kept in the store by plugin
//! name (`store::plugin_settings`) for the server, and for a repository as files of its project settings,
//! `repos/<repository name>/plugins/<plugin>/<key>` in the harness configuration repository, committed as the other
//! project settings are (`harness::repo`); the launcher gets its app settings and those of a run's repository in its launch
//! request (see `harness::launch`). Only a person changes them, and each save is recorded as an
//! `edit_project_settings` or `edit_app_settings` decision. A plugin may bring its own page for them (`settings_ui` of
//! its manifest), which the web app loads from [`App::settings_ui`] and draws in place of the fields; the values are
//! kept and checked here all the same.

use std::collections::BTreeMap;

use moebius_plugin_api::{Manifest, PluginSetting, SettingInput};
use serde::Serialize;

use super::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;

/// The longest value a setting takes, in bytes: room for any Dockerfile.
pub const MAX_VALUE: usize = 256 * 1024;

/// What a setting is set for: the whole server, or one repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    App,
    Project(i64),
}

impl Scope {
    /// The settings of this scope that the manifest declares.
    fn declared(self, manifest: &Manifest) -> &[PluginSetting] {
        match self {
            Scope::App => &manifest.app_settings,
            Scope::Project(_) => &manifest.project_settings,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Scope::App => "app",
            Scope::Project(_) => "project",
        }
    }
}

/// One setting of a plugin as a page shows it: what the manifest declares, and the value it has (empty when it has
/// none).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingView {
    #[serde(flatten)]
    pub setting: PluginSetting,
    pub value: String,
}

/// The settings of one plugin in one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginSettings {
    pub plugin: String,
    pub version: String,
    pub settings: Vec<SettingView>,
    /// Whether the plugin brings its own page for its settings, served by [`App::settings_ui`].
    pub ui: bool,
}

/// The answer of `GET /v1/repositories/{id}/plugin-settings`: every plugin that has project settings, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginProjectSettings {
    pub repository: i64,
    pub plugins: Vec<PluginSettings>,
}

/// The answer of `GET /v1/plugins/settings`: every plugin that has app settings, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppSettings {
    pub plugins: Vec<PluginSettings>,
}

impl App {
    /// The project settings of every plugin added to the host that declares some, with the repository's values.
    pub fn plugin_project_settings(&self, repository_id: i64) -> Result<PluginProjectSettings> {
        self.repository(repository_id)?;
        Ok(PluginProjectSettings {
            repository: repository_id,
            plugins: self.settings_of(Scope::Project(repository_id))?,
        })
    }

    /// The app settings of every plugin added to the host that declares some, with the server's values.
    pub fn app_settings(&self) -> Result<AppSettings> {
        Ok(AppSettings {
            plugins: self.settings_of(Scope::App)?,
        })
    }

    /// Makes `values` the plugin's settings in `scope`: a key the plugin does not declare there, a value too long or a
    /// line with a line break is refused, and a key left out is emptied. Only a person may change them, since a
    /// launcher's settings say what a run is let do.
    pub fn save_plugin_settings(
        &self,
        by: &Actor,
        scope: Scope,
        plugin: &str,
        values: &BTreeMap<String, String>,
        reason: &str,
    ) -> Result<PluginSettings> {
        if !matches!(by, Actor::Person) {
            return Err(AppError::Forbidden(
                "only a person changes the settings of a plugin".into(),
            ));
        }
        let repository = match scope {
            Scope::App => None,
            Scope::Project(id) => Some(self.repository(id)?),
        };
        let manifest = self
            .decisions
            .host()
            .manifests()
            .into_iter()
            .find(|manifest| manifest.name == plugin && !scope.declared(manifest).is_empty())
            .ok_or_else(|| {
                AppError::NotFound(format!("a plugin {plugin} with {} settings", scope.name()))
            })?;
        for (key, value) in values {
            let setting = scope
                .declared(&manifest)
                .iter()
                .find(|setting| &setting.key == key)
                .ok_or_else(|| AppError::Invalid(format!("{plugin} has no setting {key:?}")))?;
            if value.len() > MAX_VALUE {
                return Err(AppError::Invalid(format!(
                    "{} is over {} KiB",
                    setting.label,
                    MAX_VALUE / 1024
                )));
            }
            if setting.input == SettingInput::Line && value.contains(['\n', '\r']) {
                return Err(AppError::Invalid(format!("{} is one line", setting.label)));
            }
        }
        let kept: BTreeMap<String, String> = values
            .iter()
            .filter(|(_, value)| !value.is_empty())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if self.stored(scope, plugin)? != kept {
            if let Some(repository) = &repository {
                let context = BTreeMap::from([
                    ("plugin".into(), plugin.to_string()),
                    (
                        "keys".into(),
                        kept.keys().cloned().collect::<Vec<_>>().join(", "),
                    ),
                    ("reason".into(), reason.trim().to_string()),
                ]);
                let message = format!("Update the {plugin} settings of {}", repository.name);
                let commit = self.project_harness_repo()?.save_project_plugin_settings(
                    self,
                    by,
                    (&repository.name, plugin),
                    &kept,
                    ("edit_project_settings", &message),
                    context,
                )?;
                if commit.is_some() {
                    self.bus.publish(Event::ProjectSettingsChanged {
                        repository: repository.id,
                        file: format!("plugins/{plugin}"),
                    });
                }
                return self.plugin_settings(scope, manifest);
            }
            let context = BTreeMap::from([
                ("plugin".into(), plugin.to_string()),
                (
                    "keys".into(),
                    kept.keys().cloned().collect::<Vec<_>>().join(", "),
                ),
            ]);
            self.record_action(by, "edit_app_settings", None, None, context, reason)?;
            self.store.set_app_settings(plugin, &kept)?;
        }
        self.plugin_settings(scope, manifest)
    }

    /// The source of the JavaScript module of the plugin's own page for its settings, for the web app to load.
    pub fn settings_ui(&self, plugin: &str) -> Result<String> {
        self.decisions
            .host()
            .manifests()
            .into_iter()
            .find(|manifest| manifest.name == plugin)
            .and_then(|manifest| manifest.settings_ui)
            .ok_or_else(|| AppError::NotFound(format!("a plugin {plugin} with a settings page")))
    }

    /// The plugins that declare settings in `scope`, by name, with their values.
    fn settings_of(&self, scope: Scope) -> Result<Vec<PluginSettings>> {
        let mut plugins = Vec::new();
        for manifest in self.decisions.host().manifests() {
            if !scope.declared(&manifest).is_empty() {
                plugins.push(self.plugin_settings(scope, manifest)?);
            }
        }
        plugins.sort_by(|a, b| a.plugin.cmp(&b.plugin));
        Ok(plugins)
    }

    fn stored(&self, scope: Scope, plugin: &str) -> Result<BTreeMap<String, String>> {
        Ok(match scope {
            Scope::App => self.store.app_settings(plugin)?,
            Scope::Project(id) => self.project_plugin_settings(&self.repository(id)?, plugin),
        })
    }

    fn plugin_settings(&self, scope: Scope, manifest: Manifest) -> Result<PluginSettings> {
        let mut values = self.stored(scope, &manifest.name)?;
        Ok(PluginSettings {
            settings: scope
                .declared(&manifest)
                .iter()
                .cloned()
                .map(|setting| SettingView {
                    value: values.remove(&setting.key).unwrap_or_default(),
                    setting,
                })
                .collect(),
            ui: manifest.settings_ui.is_some(),
            plugin: manifest.name,
            version: manifest.version,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::{Call, Host, Kind, Plugin, PluginError};
    use serde_json::Value;

    use super::*;
    use crate::store::Store;

    /// A launcher that asks each repository for a Dockerfile and a network, and the server for a registry.
    struct Boxed;

    impl Plugin for Boxed {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "box".into(),
                version: "1.2".into(),
                kinds: vec![Kind::Launcher],
                project_settings: vec![
                    PluginSetting {
                        key: "dockerfile".into(),
                        label: "Dockerfile".into(),
                        description: "The image of the runs.".into(),
                        input: SettingInput::Code,
                        language: Some("dockerfile".into()),
                    },
                    PluginSetting {
                        key: "network".into(),
                        label: "Network".into(),
                        description: String::new(),
                        input: SettingInput::Line,
                        language: None,
                    },
                ],
                app_settings: vec![PluginSetting {
                    key: "registry".into(),
                    label: "Registry".into(),
                    description: String::new(),
                    input: SettingInput::Line,
                    language: None,
                }],
                settings_ui: None,
            }
        }

        fn call(&self, _: Call) -> std::result::Result<Value, PluginError> {
            Err(PluginError::Failed("not called".into()))
        }
    }

    /// A launcher that draws its own page for its one setting of the server.
    struct Drawn;

    const PAGE: &str = "export default function mount() {}";

    impl Plugin for Drawn {
        fn manifest(&self) -> Manifest {
            Manifest {
                name: "drawn".into(),
                version: "0.1".into(),
                kinds: vec![Kind::Launcher],
                project_settings: Vec::new(),
                app_settings: vec![PluginSetting {
                    key: "base".into(),
                    label: "Base".into(),
                    description: String::new(),
                    input: SettingInput::Code,
                    language: None,
                }],
                settings_ui: Some(PAGE.into()),
            }
        }

        fn call(&self, _: Call) -> std::result::Result<Value, PluginError> {
            Err(PluginError::Failed("not called".into()))
        }
    }

    fn app() -> (tempfile::TempDir, App, i64) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let repo = store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Boxed);
        host.add(Drawn);
        let app = App::new(store, Arc::new(host), &dir.path().join("data"));
        crate::harness::repo::testing::use_harness(&app, dir.path());
        (dir, app, repo.id)
    }

    fn stored(app: &App, repo: i64) -> BTreeMap<String, String> {
        app.project_plugin_settings(&app.repository(repo).unwrap(), "box")
    }

    #[test]
    fn a_person_saves_what_the_plugin_declares_and_it_is_recorded() {
        let (_dir, app, repo) = app();
        let empty = app.plugin_project_settings(repo).unwrap();
        assert_eq!(empty.plugins.len(), 1);
        assert_eq!(empty.plugins[0].plugin, "box");
        assert!(empty.plugins[0].settings.iter().all(|s| s.value.is_empty()));

        let values = BTreeMap::from([("dockerfile".to_string(), "FROM debian\n".to_string())]);
        let saved = app
            .save_plugin_settings(
                &Actor::Person,
                Scope::Project(repo),
                "box",
                &values,
                "an image for the tests",
            )
            .unwrap();
        assert_eq!(saved.settings[0].value, "FROM debian\n");
        assert_eq!(saved.settings[1].value, "");
        assert_eq!(stored(&app, repo), values);
        // It is a file of the project's settings, a commit of the harness configuration repository.
        let file = app.harness_repo().unwrap().project_dir("demo");
        assert_eq!(
            std::fs::read_to_string(file.join("plugins/box/dockerfile")).unwrap(),
            "FROM debian\n"
        );
        let kinds: Vec<String> = app
            .store
            .judgements("edit_project_settings")
            .unwrap()
            .into_iter()
            .map(|j| j.decider)
            .collect();
        assert_eq!(kinds, ["person"]);

        // Saving the same values again records nothing more, an empty one being no value.
        let same = BTreeMap::from([
            ("dockerfile".to_string(), "FROM debian\n".to_string()),
            ("network".to_string(), String::new()),
        ]);
        app.save_plugin_settings(&Actor::Person, Scope::Project(repo), "box", &same, "")
            .unwrap();
        assert_eq!(
            app.store.judgements("edit_project_settings").unwrap().len(),
            1
        );
    }

    #[test]
    fn a_server_without_a_harness_repository_has_no_project_settings_of_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let repo = store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Boxed);
        let app = App::new(store, Arc::new(host), dir.path());
        assert!(
            app.plugin_project_settings(repo.id).unwrap().plugins[0]
                .settings
                .iter()
                .all(|s| s.value.is_empty())
        );
        let values = BTreeMap::from([("dockerfile".to_string(), "FROM debian\n".to_string())]);
        assert!(matches!(
            app.save_plugin_settings(&Actor::Person, Scope::Project(repo.id), "box", &values, ""),
            Err(AppError::Conflict(_))
        ));
        // The server's own settings do not need it.
        let registry = BTreeMap::from([("registry".to_string(), "quay.io".to_string())]);
        app.save_plugin_settings(&Actor::Person, Scope::App, "box", &registry, "")
            .unwrap();
    }

    #[test]
    fn what_the_plugin_does_not_declare_or_an_agent_is_refused() {
        let (_dir, app, repo) = app();
        let save = |by: &Actor, plugin: &str, key: &str, value: &str| {
            app.save_plugin_settings(
                by,
                Scope::Project(repo),
                plugin,
                &BTreeMap::from([(key.to_string(), value.to_string())]),
                "",
            )
        };
        assert!(matches!(
            save(&Actor::Person, "box", "volume", "x"),
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            save(&Actor::Person, "box", "network", "a\nb"),
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            save(
                &Actor::Person,
                "box",
                "dockerfile",
                &"x".repeat(MAX_VALUE + 1)
            ),
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            save(&Actor::Person, "nobody", "dockerfile", "x"),
            Err(AppError::NotFound(_))
        ));
        let agent = Actor::Agent {
            name: "Moebius".into(),
            run: 1,
        };
        assert!(matches!(
            save(&agent, "box", "dockerfile", "FROM scratch"),
            Err(AppError::Forbidden(_))
        ));
        assert!(stored(&app, repo).is_empty());
        assert!(matches!(
            app.plugin_project_settings(repo + 1),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn the_servers_settings_are_kept_apart_from_a_repositorys_and_recorded_as_their_own_kind() {
        let (_dir, app, repo) = app();
        let listed = app.app_settings().unwrap();
        assert_eq!(listed.plugins.len(), 2);
        assert_eq!(listed.plugins[0].settings.len(), 1);
        assert_eq!(listed.plugins[0].settings[0].setting.key, "registry");

        let values = BTreeMap::from([("registry".to_string(), "quay.io".to_string())]);
        let saved = app
            .save_plugin_settings(&Actor::Person, Scope::App, "box", &values, "")
            .unwrap();
        assert_eq!(saved.settings[0].value, "quay.io");
        assert_eq!(app.store.app_settings("box").unwrap(), values);
        assert!(stored(&app, repo).is_empty());
        assert_eq!(app.store.judgements("edit_app_settings").unwrap().len(), 1);
        assert!(
            app.store
                .judgements("edit_project_settings")
                .unwrap()
                .is_empty()
        );

        // A key of the other scope is not one of this scope's.
        let dockerfile = BTreeMap::from([("dockerfile".to_string(), "FROM x".to_string())]);
        assert!(matches!(
            app.save_plugin_settings(&Actor::Person, Scope::App, "box", &dockerfile, ""),
            Err(AppError::Invalid(_))
        ));
        let agent = Actor::Agent {
            name: "Moebius".into(),
            run: 1,
        };
        assert!(matches!(
            app.save_plugin_settings(&agent, Scope::App, "box", &values, ""),
            Err(AppError::Forbidden(_))
        ));
    }

    #[test]
    fn a_plugin_that_brings_its_own_page_says_so_and_it_is_served() {
        let (_dir, app, repo) = app();
        let listed = app.app_settings().unwrap();
        let ui: Vec<(&str, bool)> = listed
            .plugins
            .iter()
            .map(|plugin| (plugin.plugin.as_str(), plugin.ui))
            .collect();
        assert_eq!(ui, [("box", false), ("drawn", true)]);
        assert_eq!(app.settings_ui("drawn").unwrap(), PAGE);
        assert!(matches!(app.settings_ui("box"), Err(AppError::NotFound(_))));
        assert!(matches!(
            app.settings_ui("nobody"),
            Err(AppError::NotFound(_))
        ));

        // Its values are kept and checked as any plugin's are.
        let values = BTreeMap::from([("base".to_string(), "FROM debian\n".to_string())]);
        app.save_plugin_settings(&Actor::Person, Scope::App, "drawn", &values, "")
            .unwrap();
        assert_eq!(app.store.app_settings("drawn").unwrap(), values);
        let stray = BTreeMap::from([("other".to_string(), "x".to_string())]);
        assert!(matches!(
            app.save_plugin_settings(&Actor::Person, Scope::App, "drawn", &stray, ""),
            Err(AppError::Invalid(_))
        ));
        // It has no project settings, so it is not among a repository's.
        assert_eq!(app.plugin_project_settings(repo).unwrap().plugins.len(), 1);
    }
}
