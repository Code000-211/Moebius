//! The throwaway copy a call is replayed in, and its files: the copy filled from the run's worktree or a checkout,
//! the files listed with their sizes and hashes, and what a call created, changed and deleted between two listings.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

use super::call::Call;
use crate::app::App;

/// The files of a directory: path relative to it, size and SHA-256 of the content (of the target, for a link).
pub type Listing = BTreeMap<String, (u64, String)>;

/// What a call did to the files of the copy.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub created: Vec<String>,
    pub changed: Vec<String>,
    pub deleted: Vec<String>,
}

/// Lists the files under `dir`, without following links.
pub fn listing(dir: &Path) -> std::io::Result<Listing> {
    fn walk(root: &Path, dir: &Path, into: &mut Listing) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                walk(root, &path, into)?;
                continue;
            }
            let (size, bytes) = if kind.is_symlink() {
                let target = std::fs::read_link(&path)?;
                (0, target.to_string_lossy().into_owned().into_bytes())
            } else {
                let bytes = std::fs::read(&path)?;
                (bytes.len() as u64, bytes)
            };
            let name = path.strip_prefix(root).unwrap_or(&path);
            into.insert(
                name.to_string_lossy().into_owned(),
                (size, hex::encode(Sha256::digest(bytes))),
            );
        }
        Ok(())
    }
    let mut into = Listing::new();
    walk(dir, dir, &mut into)?;
    Ok(into)
}

/// What happened to the files between two listings.
pub fn compare(before: &Listing, after: &Listing) -> Changes {
    let mut changes = Changes::default();
    for (name, file) in after {
        match before.get(name) {
            None => changes.created.push(name.clone()),
            Some(was) if was != file => changes.changed.push(name.clone()),
            Some(_) => {}
        }
    }
    changes.deleted = before
        .keys()
        .filter(|name| !after.contains_key(*name))
        .cloned()
        .collect();
    changes
}

/// Copies the files of `from` into `to`: not `.git` and not links, which could lead out of the copy.
fn copy_files(from: &Path, to: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (path, name) = (entry.path(), entry.file_name());
        let kind = entry.file_type()?;
        if name == ".git" {
            continue;
        } else if kind.is_dir() {
            std::fs::create_dir_all(to.join(&name))?;
            copy_files(&path, &to.join(&name))?;
        } else if kind.is_file() {
            std::fs::copy(&path, to.join(&name))?;
        }
    }
    Ok(())
}

/// Fills `copy` with the files of the call's worktree, or else with a checkout of the card's change branch, or else of
/// the repository's base.
pub(super) fn fill(
    app: &App,
    call: &Call,
    copy: &Path,
    tarball: &Path,
) -> std::result::Result<(), String> {
    if call.worktree.is_dir() {
        return copy_files(&call.worktree, copy)
            .map_err(|e| format!("cannot copy the worktree: {e}"));
    }
    let clone = Path::new(&call.repository.path);
    let branch = call.run.card_id.and_then(|card| {
        let changes = app.store.changes(call.repository.id).ok()?;
        changes
            .into_iter()
            .rfind(|change| change.card_id == Some(card))
            .map(|change| change.branch)
    });
    let base = call.repository.base_ref();
    let mut failure = String::new();
    for rev in branch.iter().chain([&base]) {
        let archived = crate::repos::git(
            clone,
            &[
                "archive",
                "--format=tar",
                "-o",
                &tarball.to_string_lossy(),
                rev,
            ],
        )
        .map_err(|e| e.to_string())
        .and_then(|_| {
            let status = Command::new("tar")
                .arg("-xf")
                .arg(tarball)
                .arg("-C")
                .arg(copy)
                .status()
                .map_err(|e| e.to_string())?;
            status.success().then_some(()).ok_or("tar failed".into())
        });
        match archived {
            Ok(()) => return Ok(()),
            Err(e) => failure = format!("cannot check out {rev}: {e}"),
        }
    }
    Err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn listed(files: &[(&str, &str)]) -> Listing {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            write_file(&dir.path().join(name), text);
        }
        listing(dir.path()).unwrap()
    }

    #[test]
    fn a_listing_has_every_file_with_its_size_and_hash_but_no_directories() {
        let list = listed(&[("a.txt", "abc"), ("src/b.txt", ""), ("src/deep/c.txt", "x")]);
        let names: Vec<_> = list.keys().map(String::as_str).collect();
        assert_eq!(names, ["a.txt", "src/b.txt", "src/deep/c.txt"]);
        assert_eq!(list["a.txt"].0, 3);
        assert_eq!(
            list["a.txt"].1,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(list["src/b.txt"].0, 0);
    }

    #[test]
    fn the_difference_of_two_listings_names_created_changed_and_deleted_files() {
        let before = listed(&[
            ("keep", "1"),
            ("edit", "1"),
            ("gone", "1"),
            ("src/gone", "2"),
        ]);
        let after = listed(&[("keep", "1"), ("edit", "2"), ("new", "1"), ("src/new", "2")]);
        assert_eq!(
            compare(&before, &after),
            Changes {
                created: vec!["new".into(), "src/new".into()],
                changed: vec!["edit".into()],
                deleted: vec!["gone".into(), "src/gone".into()],
            }
        );
        assert_eq!(compare(&before, &before), Changes::default());
    }

    #[test]
    fn a_file_with_the_same_size_and_other_content_is_changed_and_one_moved_is_created_and_deleted()
    {
        let before = listed(&[("a", "x"), ("b", "y")]);
        let after = listed(&[("a", "z"), ("c", "y")]);
        let changes = compare(&before, &after);
        assert_eq!(changes.changed, ["a"]);
        assert_eq!(
            (changes.created, changes.deleted),
            (vec!["c".to_string()], vec!["b".to_string()])
        );
    }

    #[test]
    fn a_copy_has_the_files_of_the_worktree_but_not_its_git_or_its_links() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        write_file(&from.join("src/a.rs"), "a");
        write_file(&from.join(".git"), "gitdir: /elsewhere");
        write_file(&from.join("sub/.git/config"), "x");
        std::os::unix::fs::symlink("/etc", from.join("link")).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        copy_files(&from, &to).unwrap();
        let copied: Vec<_> = listing(&to).unwrap().into_keys().collect();
        assert_eq!(copied, ["src/a.rs"]);
    }
}
