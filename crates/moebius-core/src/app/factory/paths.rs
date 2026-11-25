//! Path globs, for the paths of a repository that never merge without a person. A pattern is read like a line of a
//! `.gitignore` in the parts that matter here: `*` is any run of characters inside one name, `?` is one character,
//! `**` as a whole name is any number of directories, a pattern without a `/` matches a name at any depth (and so
//! everything under a directory of that name), and a pattern with one is matched from the repository's root. A trailing
//! `/` means the directory and all that is under it.

/// The globs that are always locked: the files that run something on a forge, the build files, the package manifests
/// and the files that tell agents how to behave. The owner's own globs are added to these, never instead of them.
pub const DEFAULT_LOCKED: &[&str] = &[
    ".github/**",
    ".forgejo/**",
    ".gitea/**",
    ".gitlab-ci.yml",
    "CMakeLists.txt",
    "*.cmake",
    "build.sh",
    "Dockerfile",
    "Dockerfile.*",
    "Makefile",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "Cargo.toml",
    "Cargo.lock",
    "pyproject.toml",
    "requirements*.txt",
    "go.mod",
    "go.sum",
    "AGENTS.md",
    "CLAUDE.md",
];

/// Whether `text` matches `pattern`, where `*` is any run of characters and `?` is one, inside one name.
fn name_matches(pattern: &[char], text: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some(('*', rest)) => (0..=text.len()).any(|skip| name_matches(rest, &text[skip..])),
        Some(('?', rest)) => !text.is_empty() && name_matches(rest, &text[1..]),
        Some((c, rest)) => text.first() == Some(c) && name_matches(rest, &text[1..]),
    }
}

/// Whether the names of a pattern match the names of a path, where the name `**` is any number of names.
fn names_match(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| names_match(rest, &path[skip..])),
        Some((first, rest)) => {
            let first: Vec<char> = first.chars().collect();
            path.split_first().is_some_and(|(name, more)| {
                name_matches(&first, &name.chars().collect::<Vec<_>>()) && names_match(rest, more)
            })
        }
    }
}

/// Whether the `path` (relative to the repository's root, with `/` between names) is locked by `pattern`.
pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim().trim_start_matches("./");
    let path = path.trim_start_matches("./").trim_start_matches('/');
    if pattern.is_empty() || path.is_empty() {
        return false;
    }
    let leading = pattern.starts_with('/');
    let (pattern, whole_directory) = match pattern.strip_suffix('/') {
        Some(directory) => (directory.trim_start_matches('/'), true),
        None => (pattern.trim_start_matches('/'), false),
    };
    let rooted = leading || pattern.contains('/');
    let names: Vec<&str> = path.split('/').collect();
    let wanted: Vec<&str> = pattern.split('/').collect();
    // Under the pattern's directory too: `build/` and `build` lock `build/a/b.txt`.
    let under = |wanted: &[&str], names: &[&str]| {
        (1..=names.len()).any(|end| {
            // The whole path matches, or a directory above it does (which is not the file itself when it must be one).
            names_match(wanted, &names[..end]) && (end < names.len() || !whole_directory)
        })
    };
    if rooted {
        return under(&wanted, &names);
    }
    // A pattern of one name matches at any depth.
    (0..names.len()).any(|start| under(&wanted, &names[start..]))
}

/// Whether any of `patterns` locks `path`.
pub fn locked_by<'a>(patterns: impl IntoIterator<Item = &'a str>, path: &str) -> Option<&'a str> {
    patterns.into_iter().find(|pattern| matches(pattern, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pattern_without_a_slash_matches_a_name_at_any_depth() {
        assert!(matches("CMakeLists.txt", "CMakeLists.txt"));
        assert!(matches("CMakeLists.txt", "src/kernels/CMakeLists.txt"));
        assert!(matches("*.cmake", "cmake/Find.cmake"));
        assert!(!matches("*.cmake", "cmake/Find.cmake.bak"));
        assert!(!matches("CMakeLists.txt", "src/NotCMakeLists.txt"));
        // And what is under a directory of that name.
        assert!(matches("vendor", "a/vendor/x/y.c"));
    }

    #[test]
    fn a_pattern_with_a_slash_is_matched_from_the_root() {
        assert!(matches(".github/**", ".github/workflows/ci.yml"));
        assert!(!matches(".github/**", "docs/.github/workflows/ci.yml"));
        assert!(matches("notes/*.md", "notes/a.md"));
        assert!(!matches("notes/*.md", "notes/sub/a.md"));
        assert!(matches("notes/**/*.md", "notes/sub/deeper/a.md"));
        assert!(matches("/AGENTS.md", "AGENTS.md"));
        assert!(!matches("/AGENTS.md", "docs/AGENTS.md"));
    }

    #[test]
    fn a_trailing_slash_is_a_directory_and_all_under_it() {
        assert!(matches("build/", "build/out/a.o"));
        assert!(!matches("build/", "build"));
        assert!(matches("build", "build"));
    }

    #[test]
    fn the_defaults_lock_the_files_the_owner_named() {
        for path in [
            ".github/workflows/ci.yml",
            "CMakeLists.txt",
            "src/a/CMakeLists.txt",
            "cmake/x.cmake",
            "build.sh",
            "Dockerfile",
            "docker/Dockerfile.cuda",
            "package.json",
            "AGENTS.md",
            "CLAUDE.md",
        ] {
            assert!(
                locked_by(DEFAULT_LOCKED.iter().copied(), path).is_some(),
                "{path}"
            );
        }
        assert!(locked_by(DEFAULT_LOCKED.iter().copied(), "src/kernel.cu").is_none());
    }
}
