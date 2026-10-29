//! The search index of a repository: the text files of its default branch, read from git and made into words for the
//! store's FTS5 table (see `store::localize`). It is made again only once the default branch has moved.

use std::path::Path;

use super::text::index_words;
use crate::repos::{git, git_with_input};
use crate::store::{IndexedFile, Repository, Store};

/// The largest file, in bytes, that is indexed; larger ones are generated or data.
const FILE_LIMIT: usize = 200_000;

/// The most files an index holds.
const FILES_LIMIT: usize = 50_000;

/// Directories whose files are somebody else's or generated.
const SKIPPED_DIRECTORIES: &[&str] = &["node_modules", "target", "vendor", ".git", "dist"];

/// Whether the file at `path` is indexed, by where it is and what it is called.
fn wanted(path: &str) -> bool {
    let mut parts = path.split('/');
    let name = parts.next_back().unwrap_or_default();
    !parts.any(|directory| SKIPPED_DIRECTORIES.contains(&directory))
        && !name.ends_with(".lock")
        && !name.ends_with("-lock.json")
        && !name.ends_with(".min.js")
        && !name.ends_with(".map")
}

/// Makes the repository's index from the commit of its default branch, unless it was made from that commit already.
/// Returns how many files it holds.
pub fn refresh(store: &Store, repository: &Repository) -> Result<usize, String> {
    let clone = Path::new(&repository.path);
    let commit = git(clone, &["rev-parse", &repository.base_ref()]).map_err(|e| e.to_string())?;
    if store
        .localize_index_commit(repository.id)
        .map_err(|e| e.to_string())?
        .as_deref()
        == Some(&commit)
    {
        return Ok(store
            .localize_paths(repository.id)
            .map_err(|e| e.to_string())?
            .len());
    }
    let files = read(clone, &commit)?;
    store
        .replace_localize_index(repository.id, &commit, &files)
        .map_err(|e| e.to_string())?;
    Ok(files.len())
}

/// The indexed files of `commit`: its text files that are not too large.
fn read(clone: &Path, commit: &str) -> Result<Vec<IndexedFile>, String> {
    let listing = crate::repos::git_bytes(clone, &["ls-tree", "-r", "-z", "--long", commit])
        .map_err(|e| e.to_string())?;
    // `<mode> <type> <object> <size>\t<path>`, one per NUL.
    let mut wanted_files = Vec::new();
    for entry in listing.split(|&b| b == 0).filter(|entry| !entry.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        let mut meta = meta.split_whitespace();
        let (_mode, kind, object, size) = (meta.next(), meta.next(), meta.next(), meta.next());
        let size = size.and_then(|size| size.parse::<usize>().ok());
        if let (Some("blob"), Some(object), Some(size)) = (kind, object, size)
            && size > 0
            && size <= FILE_LIMIT
            && wanted(path)
        {
            wanted_files.push((path.to_string(), object.to_string()));
        }
    }
    wanted_files.truncate(FILES_LIMIT);
    let input: String = wanted_files
        .iter()
        .map(|(_, object)| format!("{object}\n"))
        .collect();
    let contents = git_with_input(clone, &["cat-file", "--batch"], input.into_bytes())
        .map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    let mut rest = contents.as_slice();
    for (path, _) in wanted_files {
        // `<object> <type> <size>\n<content>\n`
        let Some(end) = rest.iter().position(|&b| b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&rest[..end]).into_owned();
        if header.ends_with(" missing") {
            rest = &rest[end + 1..];
            continue;
        }
        let size = header
            .split_whitespace()
            .nth(2)
            .and_then(|size| size.parse::<usize>().ok())
            .ok_or_else(|| format!("cannot read git's answer: {header}"))?;
        let body = &rest[end + 1..(end + 1 + size).min(rest.len())];
        rest = rest.get(end + 1 + size + 1..).unwrap_or_default();
        if body.iter().take(8000).any(|&b| b == 0) {
            continue;
        }
        files.push(IndexedFile {
            path_words: index_words(&path),
            words: index_words(&String::from_utf8_lossy(body)),
            path,
        });
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_and_foreign_files_are_left_out() {
        for path in ["src/main.rs", "docs/a.md", "README"] {
            assert!(wanted(path), "{path}");
        }
        for path in [
            "node_modules/x/index.js",
            "a/vendor/b.go",
            "Cargo.lock",
            "web/package-lock.json",
            "static/app.min.js",
            "dist/a.js",
        ] {
            assert!(!wanted(path), "{path}");
        }
    }
}
