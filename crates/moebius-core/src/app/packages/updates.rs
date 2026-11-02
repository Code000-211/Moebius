//! The nightly check of the installed Pi packages for newer versions: from `retro_at` on, once a day, a
//! `extension_update` item in the inbox for each package that has one (once per version), resolved when the package is
//! current again or an update is proposed.

use serde_json::json;

use super::{Available, UPDATE_ITEM};
use crate::app::{App, Result};
use crate::bus::Event;
use crate::config::TimeOfDay;
use crate::harness::packages::{self, Locked, Scratch};

impl App {
    /// Looks at every installed package for a newer version and puts one `extension_update` item in the inbox for each
    /// that has one (once per version); the item of a package that is current again is resolved. A package whose source
    /// cannot be reached is skipped and reported as a `harness_error`.
    pub fn check_package_updates(&self) -> Result<Vec<Available>> {
        let Some(repo) = self.harness_repo.get() else {
            return Ok(vec![]);
        };
        let lock = repo.package_lock()?;
        if lock.packages.is_empty() {
            return Ok(vec![]);
        }
        let registry = self.registry()?;
        let scratch = Scratch::new(&self.data_dir)?;
        let mut available = vec![];
        for locked in &lock.packages {
            match packages::latest(&registry, locked, &scratch) {
                Ok(Some(latest)) => {
                    self.put_update_item(locked, &latest.version)?;
                    available.push(Available {
                        package: locked.name.clone(),
                        from: locked.version.clone(),
                        to: latest.version,
                    });
                }
                Ok(None) => self.resolve_update_items(&locked.name)?,
                Err(e) => self.bus.publish(Event::HarnessError {
                    agent: None,
                    message: format!("cannot check the package {} for updates: {e}", locked.name),
                }),
            }
        }
        Ok(available)
    }

    /// The nightly check: from `at` on, once a day, the packages are checked for updates (see `check_package_updates`).
    /// The day is kept in a file of the data directory, so that a restart does not check again.
    pub fn on_package_schedule(&self, at: TimeOfDay) -> Result<()> {
        if self.harness_repo.get().is_none() {
            return Ok(());
        }
        let (day, now) = (self.store.today()?, self.store.minutes_into_the_day()?);
        let marker = self.data_dir.join("packages-checked");
        if now < at.0 || std::fs::read_to_string(&marker).is_ok_and(|seen| seen.trim() == day) {
            return Ok(());
        }
        // Written first: a check that fails is not tried again every minute.
        std::fs::write(&marker, &day)?;
        self.check_package_updates().map(drop)
    }

    fn put_update_item(&self, locked: &Locked, to: &str) -> Result<()> {
        for item in self.update_items(&locked.name)? {
            if item.data["to"] == to {
                return Ok(());
            }
            self.store.resolve_inbox_item(item.id)?;
        }
        self.store.add_inbox_item(
            UPDATE_ITEM,
            None,
            &format!("{} {} -> {to}", locked.name, locked.version),
            &json!({ "package": locked.name, "from": locked.version, "to": to, "source": locked.source }),
        )?;
        Ok(())
    }

    fn update_items(&self, package: &str) -> Result<Vec<crate::store::InboxItem>> {
        Ok(self
            .store
            .inbox()?
            .into_iter()
            .filter(|item| item.kind == UPDATE_ITEM && item.data["package"] == package)
            .collect())
    }

    pub(super) fn resolve_update_items(&self, package: &str) -> Result<()> {
        for item in self.update_items(package)? {
            self.store.resolve_inbox_item(item.id)?;
        }
        Ok(())
    }
}
