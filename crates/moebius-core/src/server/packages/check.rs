//! The nightly look for newer versions: for every installed package, the newest version of its registry that works
//! with this Moebius. Each package with a newer one gets one item in the inbox (`package_update`) and the event
//! `package_update_available`, once for each version, until the package is updated or removed.

use std::sync::Arc;

use serde_json::json;

use super::version::Version;
use super::{Packages, Result, latest_compatible};
use crate::app::{App, AppError};
use crate::bus::Event;
use crate::harness::repo::HarnessRepo;
use crate::server::settings::Live;

/// The kind of the inbox item that offers an update.
pub const INBOX_KIND: &str = "package_update";

/// The minutes into the local day from which the check is due: three in the morning.
const DUE_FROM: u16 = 3 * 60;

impl Packages<'_> {
    /// Adds an inbox item and an event for each installed package with a newer version that works with this Moebius;
    /// the names and versions of those.
    pub fn check_updates(&self) -> Result<Vec<(String, String)>> {
        let _only = super::exclusive();
        let lock = self.read_lock()?;
        let mut found = Vec::new();
        for registry in self.live.get().registries.iter() {
            let installed: Vec<_> = lock
                .packages
                .iter()
                .filter(|p| p.registry == registry.name)
                .collect();
            if installed.is_empty() {
                continue;
            }
            let (catalogs, problems) = self.catalogs(Some(&registry.name));
            if let Some(problem) = problems.into_iter().next() {
                self.app.bus.publish(Event::PackageCheckError {
                    registry: problem.registry,
                    message: problem.message,
                });
                continue;
            }
            for locked in installed {
                let versions: Vec<_> = catalogs[0]
                    .packages
                    .iter()
                    .filter(|p| p.name == locked.name)
                    .collect();
                let Some(best) = latest_compatible(&versions, self.running()) else {
                    continue;
                };
                if Some(best.parsed_version()) > Version::parse(&locked.version)
                    && self.offer_update(&locked.name, &locked.version, &best.version)?
                {
                    found.push((locked.name.clone(), best.version.clone()));
                }
            }
        }
        Ok(found)
    }

    /// Puts the update in the inbox unless it is there open; whether it was put. The open items that offer another
    /// version of the package are resolved, so that there is one item for each package.
    fn offer_update(&self, name: &str, installed: &str, version: &str) -> Result<bool> {
        let open = self.app.store.inbox().map_err(AppError::from)?;
        let offers =
            |item: &&crate::store::InboxItem| item.kind == INBOX_KIND && item.data["name"] == name;
        if open
            .iter()
            .filter(offers)
            .any(|item| item.data["version"] == version)
        {
            return Ok(false);
        }
        for older in open.iter().filter(offers) {
            self.app
                .store
                .resolve_inbox_item(older.id)
                .map_err(AppError::from)?;
        }
        self.app
            .store
            .add_inbox_item(
                INBOX_KIND,
                None,
                &format!("Update the package {name} from {installed} to {version}"),
                &json!({ "name": name, "installed": installed, "version": version }),
            )
            .map_err(AppError::from)?;
        self.app.bus.publish(Event::PackageUpdateAvailable {
            name: name.to_string(),
            version: version.to_string(),
        });
        Ok(true)
    }

    /// Resolves the open update items of the package `name`, such as once it is updated or removed.
    pub(super) fn resolve_updates(&self, name: &str) -> Result<()> {
        let open = self.app.store.inbox().map_err(AppError::from)?;
        for item in open
            .iter()
            .filter(|item| item.kind == INBOX_KIND && item.data["name"] == name)
        {
            self.app
                .store
                .resolve_inbox_item(item.id)
                .map_err(AppError::from)?;
        }
        Ok(())
    }
}

/// Whether the check is due: from three in the morning, and not yet made on `day` (`seen` is the day of the last).
fn due(minutes: u16, day: &str, seen: &str) -> bool {
    minutes >= DUE_FROM && seen != day
}

/// The schedule's part: once a day, from three in the morning, looks for updates of the installed packages. Nothing
/// is fetched while nothing is installed. The day is kept in `<data_dir>/packages.checked`, written before the check
/// so that a check that fails is not made again until tomorrow.
pub fn check_if_due(
    app: &Arc<App>,
    live: &Arc<Live>,
    harness: Option<&Arc<HarnessRepo>>,
) -> Result<()> {
    let data_dir = live.get().data_dir.clone();
    let day = app.store.today().map_err(AppError::from)?;
    let minutes = app.store.minutes_into_the_day().map_err(AppError::from)?;
    let marker = data_dir.join("packages.checked");
    let seen = std::fs::read_to_string(&marker).unwrap_or_default();
    if !due(minutes, &day, seen.trim()) {
        return Ok(());
    }
    if super::Lock::read(&data_dir)?.packages.is_empty() {
        return Ok(());
    }
    std::fs::write(&marker, &day)?;
    let (app, live, harness) = (app.clone(), live.clone(), harness.cloned());
    std::thread::spawn(move || {
        let packages = Packages {
            app: &app,
            live: &live,
            harness: harness.as_deref(),
            moebius: crate::VERSION,
        };
        if let Err(e) = packages.check_updates() {
            app.bus.publish(Event::PackageCheckError {
                registry: String::new(),
                message: e.to_string(),
            });
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_check_is_made_once_a_day_from_three_in_the_morning() {
        assert!(!due(179, "2026-11-02", ""));
        assert!(due(180, "2026-11-02", ""));
        assert!(due(23 * 60, "2026-11-02", "2026-11-01"));
        assert!(!due(23 * 60, "2026-11-02", "2026-11-02"));
    }
}
