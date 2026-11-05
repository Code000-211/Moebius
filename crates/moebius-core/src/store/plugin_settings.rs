//! The settings of plugins for the whole server: the values a person gave the `app_settings` a plugin's manifest
//! declares (see `app::plugin_settings`), kept by plugin name, so they stay while the plugin is not running and come
//! back with it; a value left empty is not stored. A repository's values are not here but files of its project settings
//! (`harness::repo`); the `project_settings` table is what an earlier version kept them in, no longer read.

use std::collections::BTreeMap;

use rusqlite::params;

use super::{Result, Store, now};

/// Kept so the migrations stay in order, and for the values an earlier version stored here, which nothing reads.
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
    fn the_servers_values_are_replaced_whole_per_plugin() {
        let (_dir, s) = store();
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
        s.set_app_settings("box", &BTreeMap::new()).unwrap();
        assert!(s.app_settings("box").unwrap().is_empty());
        assert_eq!(s.app_settings("other").unwrap().len(), 1);
    }
}
