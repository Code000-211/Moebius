//! Finding things in a repository's default branch for the project page: the list of its paths, which the app
//! matches as the person types ("Go to file"), and a full-text search with `git grep` in Moebius's clone ("Search in
//! files"). Both read the same branch as [`crate::repos::files`], never the working tree.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::repos::{git, git_bytes};

/// How many hits a search answers when it is not told.
pub const DEFAULT_LIMIT: usize = 200;

/// The most hits a search answers, whatever it is told.
pub const MAX_LIMIT: usize = 1000;

/// The most characters of a matching line that are answered.
const LINE_CHARS: usize = 300;

/// The files of the default branch as `GET /v1/repositories/{id}/files` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paths {
    /// From the repository's root, sorted.
    pub paths: Vec<String>,
}

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    /// The line's number, from 1.
    pub line: u64,
    /// The line, cut after 300 characters.
    pub text: String,
}

/// The matching lines of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHits {
    pub path: String,
    pub hits: Vec<Hit>,
}

/// What `GET /v1/repositories/{id}/search` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    /// The hits per file, files in path order and hits in line order.
    pub files: Vec<FileHits>,
    /// How many hits there are in `files`.
    pub total: usize,
    /// Whether there are more hits than the limit let through.
    pub truncated: bool,
}

/// Whether the characters of `query` occur in `path` in that order, ignoring case.
fn is_subsequence(query: &str, path: &str) -> bool {
    let mut path = path.chars().flat_map(char::to_lowercase);
    query
        .chars()
        .flat_map(char::to_lowercase)
        .all(|wanted| path.any(|c| c == wanted))
}

/// Whether repository `id`'s default branch exists, which it does not in a clone without a commit.
fn has_branch(clone: &Path, base: &str) -> bool {
    git(
        clone,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{base}^{{commit}}"),
        ],
    )
    .is_ok()
}

/// The paths of the files of repository `id`'s default branch, those that contain the characters of `query` in that
/// order when it is not blank. Submodules are left out.
pub fn paths(app: &App, id: i64, query: &str) -> Result<Paths> {
    let repository = app.repository(id)?;
    let clone = Path::new(&repository.path);
    let base = repository.base_ref();
    if !has_branch(clone, &base) {
        return Ok(Paths { paths: Vec::new() });
    }
    let listing = git_bytes(clone, &["ls-tree", "-r", "-z", "--full-tree", &base])?;
    let query = query.trim();
    let mut paths: Vec<String> = listing
        .split(|b| *b == 0)
        .filter_map(|line| {
            let line = String::from_utf8_lossy(line);
            let (meta, path) = line.split_once('\t')?;
            (meta.split_whitespace().nth(1) == Some("blob")).then(|| path.to_string())
        })
        .filter(|path| is_subsequence(query, path))
        .collect();
    paths.sort();
    Ok(Paths { paths })
}

/// What a search looks for.
#[derive(Debug, Clone, Deserialize)]
pub struct Search {
    pub q: String,
    /// Whether `q` is an extended regular expression; otherwise it is the text itself.
    #[serde(default)]
    pub regex: bool,
    /// The most hits to answer; [`DEFAULT_LIMIT`] when it is left out, at most [`MAX_LIMIT`].
    #[serde(default)]
    pub limit: Option<usize>,
}

/// `text` cut to [`LINE_CHARS`] characters.
fn cut(text: &str) -> String {
    text.chars().take(LINE_CHARS).collect()
}

/// Searches the files of repository `id`'s default branch for `search.q`, ignoring case, and skips binary files.
/// Reading stops once there is a hit more than the limit, so that a search for something common stays cheap.
pub fn search(app: &App, id: i64, search: &Search) -> Result<Found> {
    if search.q.is_empty() {
        return Err(AppError::Invalid("a search needs a q".into()));
    }
    let limit = search.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let repository = app.repository(id)?;
    let clone = Path::new(&repository.path);
    let base = repository.base_ref();
    let mut found = Found {
        files: Vec::new(),
        total: 0,
        truncated: false,
    };
    if !has_branch(clone, &base) {
        return Ok(found);
    }
    let mut child = Command::new("git")
        .arg("-C")
        .arg(clone)
        .args(["grep", "-I", "-n", "-z", "-i", "--no-color"])
        .arg(if search.regex { "-E" } else { "-F" })
        .arg("-e")
        .arg(&search.q)
        .arg(&base)
        .arg("--")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(crate::repos::GitError::from)?;
    let mut lines = BufReader::new(child.stdout.take().expect("piped"));
    let prefix = format!("{base}:");
    let mut line = Vec::new();
    loop {
        line.clear();
        if lines
            .read_until(b'\n', &mut line)
            .map_err(crate::repos::GitError::from)?
            == 0
        {
            break;
        }
        // `<ref>:<path> NUL <line number> NUL <text>`
        let line = line.strip_suffix(b"\n").unwrap_or(&line);
        let mut parts = line.splitn(3, |b| *b == 0);
        let (Some(path), Some(number), Some(text)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let path = String::from_utf8_lossy(path);
        let path = path.strip_prefix(&prefix).unwrap_or(&path).to_string();
        let Ok(number) = String::from_utf8_lossy(number).parse() else {
            continue;
        };
        if found.total == limit {
            found.truncated = true;
            let _ = child.kill();
            break;
        }
        let hit = Hit {
            line: number,
            text: cut(&String::from_utf8_lossy(text)),
        };
        match found.files.last_mut() {
            Some(file) if file.path == path => file.hits.push(hit),
            _ => found.files.push(FileHits {
                path,
                hits: vec![hit],
            }),
        }
        found.total += 1;
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    let status = child.wait().map_err(crate::repos::GitError::from)?;
    // 0 is a match and 1 none; anything else (and a stop of ours) is git refusing, such as an invalid expression.
    if !found.truncated && status.code() != Some(0) && status.code() != Some(1) {
        let why = stderr.lines().next().unwrap_or("git grep failed");
        return Err(AppError::Invalid(redact(why)));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_matches_the_characters_of_a_path_in_order() {
        assert!(is_subsequence("", "src/main.rs"));
        assert!(is_subsequence("smr", "src/main.rs"));
        assert!(is_subsequence("MAIN", "src/main.rs"));
        assert!(!is_subsequence("rsm", "src/main.rs"));
        assert!(!is_subsequence("x", "src/main.rs"));
    }

    #[test]
    fn a_long_line_is_cut() {
        assert_eq!(cut(&"é".repeat(400)).chars().count(), LINE_CHARS);
    }
}
