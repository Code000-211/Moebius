//! The settings of plugins: the values a person gave the settings a plugin's manifest declares (see
//! `app::plugin_settings`), for each repository and plugin (`project_settings`) and for each plugin on the whole server
//! (`app_settings`). They are kept by plugin name, so they stay while the plugin is not running and come back with it;
//! a value left empty is not stored.

use std::collections::BTreeMap;

use rusqlite::params;

use super::{Result, Store, now};

pub(super) const PROJECT_SCHEMA: &str = r#"
CREATE TABLE project_settings (
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    plugin TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    at INTEGER NOT NULL,
    PRIMARY KEY (repository_id, plugin, key)
);
"#;

pub(super) const APP_SCHEMA: &str = r#"
CREATE TABLE app_settings (
    plugin TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    at INTEGER NOT NULL,
    PRIMARY KEY (plugin, key)
);
"#;

impl Store {
    /// The values the plugin `plugin` has for the repository, by key.
    pub fn project_settings(
        &self,
        repository_id: i64,
        plugin: &str,
    ) -> Result<BTreeMap<String, String>> {
        Ok(self
            .rows(
                "SELECT key, value FROM project_settings WHERE repository_id = ?1 AND plugin = ?2",
                params![repository_id, plugin],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?
            .into_iter()
            .collect())
    }

    /// Makes `values` the plugin's values for the repository, in place of all it had; an empty value is removed.
    pub fn set_project_settings(
        &self,
        repository_id: i64,
        plugin: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM project_settings WHERE repository_id = ?1 AND plugin = ?2",
            params![repository_id, plugin],
        )?;
        let at = now();
        for (key, value) in values.iter().filter(|(_, value)| !value.is_empty()) {
            tx.execute(
                "INSERT INTO project_settings (repository_id, plugin, key, value, at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![repository_id, plugin, key, value, at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The values the plugin `plugin` has for the whole server, by key.
    pub fn app_settings(&self, plugin: &str) -> Result<BTreeMap<String, String>> {
        Ok(self
            .rows(
                "SELECT key, value FROM app_settings WHERE plugin = ?1",
                [plugin],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?
            .into_iter()
            .collect())
    }

    /// Makes `values` the plugin's values for the whole server, in place of all it had; an empty value is removed.
    pub fn set_app_settings(&self, plugin: &str, values: &BTreeMap<String, String>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM app_settings WHERE plugin = ?1", [plugin])?;
        let at = now();
        for (key, value) in values.iter().filter(|(_, value)| !value.is_empty()) {
            tx.execute(
                "INSERT INTO app_settings (plugin, key, value, at) VALUES (?1, ?2, ?3, ?4)",
                params![plugin, key, value, at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_store as store;

    #[test]
    fn values_are_replaced_whole_per_repository_and_plugin() {
        let (_dir, s) = store();
        let demo = s
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let other = s
            .add_repository("other", "/srv/other", "none", "main")
            .unwrap();
        assert!(s.project_settings(demo.id, "box").unwrap().is_empty());

        let values = BTreeMap::from([
            ("dockerfile".to_string(), "FROM debian\n".to_string()),
            ("blank".to_string(), String::new()),
        ]);
        s.set_project_settings(demo.id, "box", &values).unwrap();
        s.set_project_settings(
            other.id,
            "box",
            &BTreeMap::from([("dockerfile".into(), "FROM alpine\n".into())]),
        )
        .unwrap();
        assert_eq!(
            s.project_settings(demo.id, "box").unwrap(),
            BTreeMap::from([("dockerfile".to_string(), "FROM debian\n".to_string())])
        );
        assert!(
            s.project_settings(demo.id, "other-plugin")
                .unwrap()
                .is_empty()
        );

        s.set_project_settings(demo.id, "box", &BTreeMap::new())
            .unwrap();
        assert!(s.project_settings(demo.id, "box").unwrap().is_empty());
        assert_eq!(s.project_settings(other.id, "box").unwrap().len(), 1);

        // Removing a repository removes its values.
        assert!(s.delete_repository(other.id).unwrap());
        assert!(s.project_settings(other.id, "box").unwrap().is_empty());
    }

    #[test]
    fn the_servers_values_are_replaced_whole_per_plugin_and_apart_from_a_repositorys() {
        let (_dir, s) = store();
        let demo = s
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let values = BTreeMap::from([
            ("network".to_string(), "internet".to_string()),
            ("blank".to_string(), String::new()),
        ]);
        s.set_app_settings("box", &values).unwrap();
        s.set_app_settings("other", &BTreeMap::from([("x".into(), "y".into())]))
            .unwrap();
        assert_eq!(
            s.app_settings("box").unwrap(),
            BTreeMap::from([("network".to_string(), "internet".to_string())])
        );
        assert!(s.project_settings(demo.id, "box").unwrap().is_empty());
        s.set_app_settings("box", &BTreeMap::new()).unwrap();
        assert!(s.app_settings("box").unwrap().is_empty());
        assert_eq!(s.app_settings("other").unwrap().len(), 1);
    }
}
