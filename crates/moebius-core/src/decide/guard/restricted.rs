//! The restricted mode of a review run of a pull request someone else opened (ADR 0029). What such a pull request holds
//! is written by a person the server does not trust, so its review run has **no tools at all**: it is started without
//! any, with no checkout, and given the pull request as text (see `app::review_context`). [`no_tools_refusal`] is what
//! the router answers to *any* call such a run makes, whatever the call and whatever the rules in prose say; the run is
//! marked restricted once, when it starts, and nothing that happens later lifts it.
//!
//! [`restricted_refusal`] is the narrower mode, for a restricted use that does have a worktree and may read it: the
//! tools that read and list files, and nothing else. A path is inside the worktree when what it resolves to is: symlinks
//! are followed before the test, a leading `@` (which Pi strips from a path) is refused, and a pattern that names a place
//! (a glob) may be neither absolute nor go up with `..`. Arguments that are no JSON object are refused: the check cannot
//! read what a harness may repair and read.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::shell::normalise_call;

/// The name the rule is refused under, and the value of `restricted` in the question's context.
pub const RESTRICTED: &str = "restricted-review";

/// What the rule says, as it is given as the reason of a refusal.
const PROSE: &str = "a review of a pull request someone else opened only reads files of its worktree: no shell, no network, no writes and no MCP tools";

/// The tools of a restricted run: Pi's tools that read and list files and Claude Code's.
const READ_TOOLS: [&str; 8] = ["read", "grep", "find", "ls", "Read", "Grep", "Glob", "LS"];

/// The arguments that name a file or a directory.
const PATH_KEYS: [&str; 5] = ["path", "file_path", "notebook_path", "directory", "cwd"];

/// The arguments that are a pattern for file names, whatever the tool.
const GLOB_KEYS: [&str; 4] = ["glob", "include", "exclude", "ignore"];

/// The tools whose `pattern` is a pattern for file names; for the others it is text to look for.
const GLOB_PATTERN_TOOLS: [&str; 4] = ["find", "ls", "Glob", "LS"];

/// What is answered to any call of a run started restricted: it has no tools, so there is nothing to allow.
pub fn no_tools_refusal(tool: &str) -> String {
    format!(
        "{RESTRICTED}: {tool} is refused: a review of a pull request someone else opened has no tools; everything it may read is in its prompt"
    )
}

/// The reason `tool` may not be called by a restricted run with `input`, none when it may. A path it names must lead
/// inside `worktree`, as the filesystem resolves it; a run without a worktree, or one that cannot be read, reads nothing.
pub fn restricted_refusal(tool: &str, input: &Value, worktree: Option<&Path>) -> Option<String> {
    if !input.is_object() {
        return Some(format!(
            "{RESTRICTED}: the arguments of {tool} cannot be read, so the call is refused: {PROSE}"
        ));
    }
    let (tool, input) = normalise_call(tool, input.clone());
    if !READ_TOOLS.contains(&tool.as_str()) {
        return Some(format!("{RESTRICTED}: {tool} is refused: {PROSE}"));
    }
    let Some(root) = worktree.and_then(|w| w.canonicalize().ok()) else {
        return Some(format!(
            "{RESTRICTED}: the run has no worktree to read: {PROSE}"
        ));
    };
    let outside = |what: &str, value: &str| {
        Some(format!(
            "{RESTRICTED}: {what} {value} is outside the worktree: {PROSE}"
        ))
    };
    for key in PATH_KEYS {
        if let Some(value) = input[key].as_str()
            && !path_inside(&root, value)
        {
            return outside("the path", value);
        }
    }
    let mut patterns: Vec<&str> = GLOB_KEYS
        .iter()
        .filter_map(|k| input[*k].as_str())
        .collect();
    if GLOB_PATTERN_TOOLS.contains(&tool.as_str())
        && let Some(pattern) = input["pattern"].as_str()
    {
        patterns.push(pattern);
    }
    patterns.into_iter().find_map(|pattern| {
        (!pattern_inside(pattern)).then(|| {
            format!(
                "{RESTRICTED}: the pattern {pattern} leaves the worktree (it is absolute or goes up): {PROSE}"
            )
        })
    })
}

/// Whether a pattern for file names stays below where it is applied: not absolute, not from the home directory, not a
/// variable, and with no `..` part.
fn pattern_inside(pattern: &str) -> bool {
    let starts_outside = pattern.starts_with(['/', '\\', '~', '$', '@'])
        || pattern.as_bytes().get(1) == Some(&b':')
        // A pattern in braces may hold alternatives that start outside: `{/etc/*,src/*}`.
        || pattern.contains(['{', ',']) && pattern.split(['{', ',']).any(|part| part.starts_with(['/', '~', '$']));
    !starts_outside && !pattern.split(['/', '\\']).any(|part| part == "..")
}

/// Whether `value`, read from `root` (canonical), leads to a place inside it: the longest part of the path that exists is
/// resolved through the filesystem, so a symlink on the way is followed; what does not exist yet cannot be a link, and
/// may not go up.
fn path_inside(root: &Path, value: &str) -> bool {
    // Pi strips a leading `@` from a path, so `@/etc/passwd` is `/etc/passwd` to it, and `@~/.ssh` is the home directory.
    if value.starts_with(['~', '$', '@']) {
        return false;
    }
    let given = Path::new(value);
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        root.join(given)
    };
    let mut existing = joined.clone();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let base = loop {
        if let Ok(real) = existing.canonicalize() {
            break real;
        }
        match existing.components().next_back() {
            Some(Component::Normal(name)) => rest.push(name.to_os_string()),
            // `..`, a root or a prefix that does not exist: not a place to read.
            _ => return false,
        }
        existing.pop();
    };
    let mut real: PathBuf = base;
    real.extend(rest.iter().rev());
    real.starts_with(root)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A worktree with `src/lib.rs` in it, and a directory outside it with a secret.
    fn tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("worktree");
        let outside = dir.path().join("secrets");
        std::fs::create_dir_all(worktree.join("src")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(worktree.join("src/lib.rs"), "x\n").unwrap();
        std::fs::write(outside.join("key"), "secret\n").unwrap();
        (dir, worktree, outside)
    }

    #[test]
    fn only_the_tools_that_read_files_of_the_worktree_are_allowed() {
        let (_dir, tree, _) = tree();
        for (tool, input) in [
            ("read", json!({"path": "src/lib.rs"})),
            ("read", json!({"path": tree.join("src/lib.rs")})),
            ("read", json!({"path": "src/not-yet.rs"})),
            ("grep", json!({"pattern": "/etc/passwd", "glob": "*.rs"})),
            ("ls", json!({})),
            ("Read", json!({"file_path": tree.join("src/lib.rs")})),
            ("Glob", json!({"pattern": "**/*.rs"})),
            ("find", json!({"pattern": "src/*.rs"})),
        ] {
            assert_eq!(
                restricted_refusal(tool, &input, Some(&tree)),
                None,
                "{tool} {input}"
            );
        }
    }

    #[test]
    fn a_leading_at_sign_is_refused_because_pi_strips_it_and_reads_what_is_left() {
        let (_dir, tree, _) = tree();
        for path in ["@/etc/passwd", "@~/.ssh/id_rsa", "@/", "@src/lib.rs"] {
            for tool in ["read", "ls", "grep"] {
                assert!(
                    restricted_refusal(tool, &json!({ "path": path }), Some(&tree)).is_some(),
                    "{tool} {path}"
                );
            }
        }
    }

    #[test]
    fn arguments_that_are_no_object_are_refused() {
        let (_dir, tree, _) = tree();
        for input in [
            json!("{\"path\": \"/etc/passwd\"\n"),
            json!("not json"),
            json!(["read"]),
            json!(null),
            json!(3),
        ] {
            assert!(
                restricted_refusal("read", &input, Some(&tree)).is_some(),
                "{input}"
            );
        }
    }

    #[test]
    fn a_run_with_no_tools_is_refused_every_call() {
        for tool in ["read", "bash", "ls", "lessons", "search__query", "Read"] {
            assert!(no_tools_refusal(tool).starts_with(RESTRICTED), "{tool}");
        }
    }

    #[test]
    fn a_shell_the_network_writes_and_mcp_tools_are_refused() {
        let (_dir, tree, _) = tree();
        for (tool, input) in [
            ("bash", json!({"command": "ls"})),
            ("bash", json!({"command": "cargo test"})),
            ("Bash", json!({"command": "curl https://example.com"})),
            ("write", json!({"path": "a.txt", "content": "x"})),
            ("edit", json!({"path": "a.txt"})),
            ("WebFetch", json!({"url": "https://example.com"})),
            ("WebSearch", json!({"query": "x"})),
            ("fetch", json!({"url": "https://example.com"})),
            ("search__query", json!({"q": "x"})),
        ] {
            let refusal = restricted_refusal(tool, &input, Some(&tree))
                .unwrap_or_else(|| panic!("{tool} was allowed"));
            assert!(refusal.starts_with(RESTRICTED), "{refusal}");
        }
    }

    #[test]
    fn a_read_outside_the_worktree_or_without_one_is_refused() {
        let (_dir, tree, outside) = tree();
        for path in [
            "/etc/passwd",
            "../secrets/key",
            "src/../../secrets/key",
            "~/.ssh/id_rsa",
            "$HOME/.ssh",
        ] {
            assert!(
                restricted_refusal("read", &json!({ "path": path }), Some(&tree)).is_some(),
                "{path}"
            );
        }
        assert!(
            restricted_refusal("read", &json!({ "path": outside.join("key") }), Some(&tree))
                .is_some()
        );
        assert!(restricted_refusal("read", &json!({"path": "a.txt"}), None).is_some());
        assert!(
            restricted_refusal(
                "read",
                &json!({"path": "a.txt"}),
                Some(Path::new("/no/such/tree"))
            )
            .is_some()
        );
    }

    // A symlink needs Unix; the worktree of a pull request is made without them (`core.symlinks=false`) as well.
    #[cfg(unix)]
    #[test]
    fn a_symlink_the_pull_request_holds_does_not_lead_out_of_the_worktree() {
        let (_dir, tree, outside) = tree();
        std::os::unix::fs::symlink(&outside, tree.join("link")).unwrap();
        std::os::unix::fs::symlink(outside.join("key"), tree.join("src/key-link")).unwrap();
        for input in [
            json!({"path": "link/key"}),
            json!({"path": "link"}),
            json!({"path": "src/key-link"}),
            json!({"path": tree.join("link/key")}),
            json!({"path": "link/not-there-yet"}),
        ] {
            assert!(
                restricted_refusal("read", &input, Some(&tree)).is_some(),
                "{input}"
            );
        }
        // A link that stays inside is no harm.
        std::os::unix::fs::symlink(tree.join("src"), tree.join("inside")).unwrap();
        assert_eq!(
            restricted_refusal("read", &json!({"path": "inside/lib.rs"}), Some(&tree)),
            None
        );
    }

    #[test]
    fn a_pattern_that_names_a_place_may_not_leave_the_worktree() {
        let (_dir, tree, _) = tree();
        for (tool, input) in [
            ("Glob", json!({"pattern": "/etc/*"})),
            ("Glob", json!({"pattern": "../**/*.pem"})),
            ("Glob", json!({"pattern": "src/../../x"})),
            ("find", json!({"pattern": "~/.ssh/*"})),
            ("find", json!({"pattern": "{/etc/*,src/*}"})),
            ("ls", json!({"pattern": "$HOME/*"})),
            ("find", json!({"pattern": "@/etc/*"})),
            ("Grep", json!({"pattern": "x", "glob": "/etc/*"})),
            ("grep", json!({"pattern": "x", "include": "../*"})),
        ] {
            assert!(
                restricted_refusal(tool, &input, Some(&tree)).is_some(),
                "{tool} {input}"
            );
        }
    }
}
