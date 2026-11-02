//! What a package has: its resources, read from the `pi` field of its `package.json` (`pi.extensions`, `pi.skills`,
//! `pi.prompts` and `pi.themes`, arrays of paths relative to the package with `*`, `**` and `?` patterns and `!pattern`
//! exclusions), or, for a kind the manifest does not mention, from the convention directories `extensions/`, `skills/`,
//! `prompts/` and `themes/`.

use std::path::Path;

use serde_json::Value;

use super::PackagesError;

/// The resources of a package, as paths relative to its directory written with `/`. A skill is the path of its
/// directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Resources {
    pub extensions: Vec<String>,
    pub skills: Vec<String>,
    /// Prompt templates and themes are listed and never loaded: they do not apply to RPC runs.
    pub prompts: Vec<String>,
    pub themes: Vec<String>,
}

/// The `package.json` of the package at `dir`; none when it has none.
pub fn package_json(dir: &Path) -> Result<Option<Value>, PackagesError> {
    match std::fs::read_to_string(dir.join("package.json")) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| PackagesError::Invalid(format!("package.json: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// The scripts of a `package.json` that npm runs when the package is installed. Moebius runs none of them.
pub fn install_scripts(json: &Value) -> Vec<String> {
    ["preinstall", "install", "postinstall", "prepare"]
        .into_iter()
        .filter(|script| json["scripts"][script].is_string())
        .map(String::from)
        .collect()
}

#[derive(Clone, Copy)]
enum What {
    Extensions,
    Skills,
    Prompts,
    Themes,
}

impl What {
    fn key(self) -> &'static str {
        match self {
            What::Extensions => "extensions",
            What::Skills => "skills",
            What::Prompts => "prompts",
            What::Themes => "themes",
        }
    }
}

/// The resources of the package at `dir`.
pub fn read_resources(dir: &Path) -> Result<Resources, PackagesError> {
    let json = package_json(dir)?;
    let manifest = json.as_ref().map(|json| &json["pi"]);
    let all = tree(dir, "", 0);
    let read = |what: What| -> Result<Vec<String>, PackagesError> {
        let patterns: Vec<String> = match manifest.map(|pi| &pi[what.key()]) {
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str().map(String::from).ok_or_else(|| {
                        PackagesError::Invalid(format!("pi.{} holds only paths", what.key()))
                    })
                })
                .collect::<Result<_, _>>()?,
            Some(Value::Null) | None => vec![what.key().to_string()],
            Some(_) => {
                return Err(PackagesError::Invalid(format!(
                    "pi.{} must be an array of paths",
                    what.key()
                )));
            }
        };
        expand(dir, &all, &patterns, what)
    };
    Ok(Resources {
        extensions: read(What::Extensions)?,
        skills: read(What::Skills)?,
        prompts: read(What::Prompts)?,
        themes: read(What::Themes)?,
    })
}

/// The paths below `dir` of every file and folder, except `node_modules` and `.git`, below `prefix`, to a depth of ten.
fn tree(dir: &Path, prefix: &str, depth: usize) -> Vec<String> {
    let mut found = vec![];
    if depth >= 10 {
        return found;
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir.join(prefix))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !matches!(name.as_str(), "node_modules" | ".git"))
        .collect();
    entries.sort();
    for name in entries {
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if dir.join(&path).is_dir() {
            found.push(path.clone());
            found.extend(tree(dir, &path, depth + 1));
        } else {
            found.push(path);
        }
    }
    found
}

/// A path of a manifest without its leading `./`, or why it cannot be used: it must stay inside the package.
fn normalize(path: &str) -> Result<String, PackagesError> {
    let path = path.trim().trim_start_matches("./").trim_end_matches('/');
    let outside = path.starts_with('/')
        || path.contains('\\')
        || path.split('/').any(|part| part == ".." || part.is_empty());
    if path.is_empty() || outside {
        return Err(PackagesError::Invalid(format!(
            "{path:?} is not a path inside the package"
        )));
    }
    Ok(path.to_string())
}

fn expand(
    dir: &Path,
    all: &[String],
    patterns: &[String],
    what: What,
) -> Result<Vec<String>, PackagesError> {
    let (mut found, mut excluded): (Vec<String>, Vec<String>) = (vec![], vec![]);
    for pattern in patterns {
        if let Some(pattern) = pattern.strip_prefix('!') {
            excluded.push(normalize(pattern)?);
            continue;
        }
        let pattern = normalize(pattern)?;
        let paths: Vec<String> = if pattern.contains(['*', '?']) {
            all.iter()
                .filter(|path| glob_match(&pattern, path))
                .cloned()
                .collect()
        } else if dir.join(&pattern).exists() {
            vec![pattern]
        } else {
            vec![]
        };
        for path in paths {
            found.extend(resources_at(dir, &path, what));
        }
    }
    let mut kept: Vec<String> = vec![];
    for path in found {
        let out = excluded
            .iter()
            .any(|e| glob_match(e, &path) || path.starts_with(&format!("{e}/")));
        if !out && !kept.contains(&path) {
            kept.push(path);
        }
    }
    Ok(kept)
}

fn extension_file(name: &str) -> bool {
    [".ts", ".js", ".mjs"].iter().any(|end| name.ends_with(end))
}

fn index_of(dir: &Path, folder: &str) -> Option<String> {
    ["index.ts", "index.js", "index.mjs"]
        .into_iter()
        .map(|file| format!("{folder}/{file}"))
        .find(|path| dir.join(path).is_file())
}

fn children(dir: &Path, folder: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join(folder))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "node_modules" && !name.starts_with('.'))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| format!("{folder}/{name}"))
        .collect()
}

/// The resources a path of the manifest stands for: a file, or the files of a folder.
fn resources_at(dir: &Path, path: &str, what: What) -> Vec<String> {
    let full = dir.join(path);
    match what {
        What::Extensions if full.is_file() => vec![path.to_string()]
            .into_iter()
            .filter(|p| extension_file(p))
            .collect(),
        What::Extensions => match index_of(dir, path) {
            Some(index) => vec![index],
            None => children(dir, path)
                .into_iter()
                .filter_map(|child| {
                    if dir.join(&child).is_file() {
                        extension_file(&child).then_some(child)
                    } else {
                        index_of(dir, &child)
                    }
                })
                .collect(),
        },
        What::Skills if full.is_file() => path
            .strip_suffix("/SKILL.md")
            .map(|folder| vec![folder.to_string()])
            .unwrap_or_default(),
        What::Skills => skills_in(dir, path, 0),
        What::Prompts | What::Themes => {
            let end = if matches!(what, What::Prompts) {
                ".md"
            } else {
                ".json"
            };
            if full.is_file() {
                vec![path.to_string()]
                    .into_iter()
                    .filter(|p| p.ends_with(end))
                    .collect()
            } else {
                children(dir, path)
                    .into_iter()
                    .filter(|child| child.ends_with(end) && dir.join(child).is_file())
                    .collect()
            }
        }
    }
}

/// `folder` if it holds a `SKILL.md`, else the folders below it that do, to a depth of three.
fn skills_in(dir: &Path, folder: &str, depth: usize) -> Vec<String> {
    if dir.join(folder).join("SKILL.md").is_file() {
        return vec![folder.to_string()];
    }
    if depth >= 3 {
        return vec![];
    }
    children(dir, folder)
        .into_iter()
        .filter(|child| dir.join(child).is_dir())
        .flat_map(|child| skills_in(dir, &child, depth + 1))
        .collect()
}

/// Whether `path` fits `pattern`: `*` is any run of characters and `?` one, inside a name, and `**` any number of names.
pub(super) fn glob_match(pattern: &str, path: &str) -> bool {
    fn name(pattern: &[char], text: &[char]) -> bool {
        match pattern.split_first() {
            None => text.is_empty(),
            Some(('*', rest)) => (0..=text.len()).any(|skip| name(rest, &text[skip..])),
            Some(('?', rest)) => !text.is_empty() && name(rest, &text[1..]),
            Some((c, rest)) => text.first() == Some(c) && name(rest, &text[1..]),
        }
    }
    fn names(pattern: &[&str], path: &[&str]) -> bool {
        match pattern.split_first() {
            None => path.is_empty(),
            Some((&"**", rest)) => {
                names(rest, path) || (!path.is_empty() && names(pattern, &path[1..]))
            }
            Some((first, rest)) => {
                !path.is_empty()
                    && name(
                        &first.chars().collect::<Vec<_>>(),
                        &path[0].chars().collect::<Vec<_>>(),
                    )
                    && names(rest, &path[1..])
            }
        }
    }
    let (pattern, path): (Vec<&str>, Vec<&str>) =
        (pattern.split('/').collect(), path.split('/').collect());
    names(&pattern, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, file: &str, text: &str) {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn patterns_match_names_and_folders() {
        assert!(glob_match("extensions/*.ts", "extensions/a.ts"));
        assert!(!glob_match("extensions/*.ts", "extensions/a/b.ts"));
        assert!(glob_match("src/**/*.ts", "src/a/b/c.ts"));
        assert!(glob_match("src/**/*.ts", "src/c.ts"));
        assert!(glob_match("a?c", "abc") && !glob_match("a?c", "ac"));
        assert!(!glob_match("skills", "skills/x"));
    }

    #[test]
    fn the_convention_directories_are_read_without_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "package.json", "{\"name\": \"p\"}");
        write(root, "extensions/one.ts", "");
        write(root, "extensions/two.js", "");
        write(root, "extensions/notes.txt", "");
        write(root, "extensions/folder/index.ts", "");
        write(root, "extensions/node_modules/dep/index.js", "");
        write(root, "skills/review/SKILL.md", "---\nname: review\n---\n");
        write(root, "skills/plain/README.md", "");
        write(root, "prompts/fix.md", "");
        write(root, "themes/dark.json", "{}");
        let found = read_resources(root).unwrap();
        assert_eq!(
            found.extensions,
            [
                "extensions/folder/index.ts",
                "extensions/one.ts",
                "extensions/two.js"
            ]
        );
        assert_eq!(found.skills, ["skills/review"]);
        assert_eq!(found.prompts, ["prompts/fix.md"]);
        assert_eq!(found.themes, ["themes/dark.json"]);
    }

    #[test]
    fn the_pi_manifest_names_the_resources_and_excludes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "package.json",
            r#"{"name": "p", "pi": {"extensions": ["./src/extension.ts", "src/more/*.ts", "!src/more/legacy.ts"],
                "skills": ["./resources/skills"], "prompts": []}}"#,
        );
        write(root, "src/extension.ts", "");
        write(root, "src/more/a.ts", "");
        write(root, "src/more/legacy.ts", "");
        write(root, "resources/skills/deep/SKILL.md", "");
        write(root, "resources/skills/deep/helper.md", "");
        // The manifest does not name themes, so the convention applies to them, and it names no prompts at all.
        write(root, "themes/light.json", "{}");
        write(root, "prompts/fix.md", "");
        write(root, "extensions/ignored.ts", "");
        let found = read_resources(root).unwrap();
        assert_eq!(found.extensions, ["src/extension.ts", "src/more/a.ts"]);
        assert_eq!(found.skills, ["resources/skills/deep"]);
        assert!(found.prompts.is_empty());
        assert_eq!(found.themes, ["themes/light.json"]);
    }

    #[test]
    fn a_manifest_that_leaves_the_package_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "package.json",
            r#"{"pi": {"extensions": ["../outside.ts"]}}"#,
        );
        assert!(read_resources(dir.path()).is_err());
        write(
            dir.path(),
            "package.json",
            r#"{"pi": {"extensions": "x.ts"}}"#,
        );
        assert!(read_resources(dir.path()).is_err());
    }

    #[test]
    fn the_install_scripts_are_named() {
        let json =
            serde_json::json!({"scripts": {"postinstall": "x", "test": "y", "prepare": "z"}});
        assert_eq!(install_scripts(&json), ["postinstall", "prepare"]);
    }
}
