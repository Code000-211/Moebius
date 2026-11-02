//! Whether a tool call can be replayed (see `replay`): run again in a copy of the worktree, it leaves everything
//! outside the copy as it was. The call is read as the built-in rules read it (`shell`), but more strictly: a call
//! that names a place beyond the worktree, the network, another repository or a program that can do anything is not
//! reversible.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::shell::{Simple, ToolCall, any_simple, git_subcommand, is_home, normalise, resolve};

/// `path` relative to the worktree, when it is inside it.
pub fn relative_to(worktree: &Path, path: &str) -> Option<PathBuf> {
    if is_home(path) {
        return None;
    }
    let root = normalise(worktree);
    let relative = resolve(&root, path).strip_prefix(&root).ok()?.to_path_buf();
    Some(relative)
}

/// The programs that reach beyond the worktree: the network, other users and processes, packages that are downloaded,
/// the environment, and interpreters that can do anything. They are looked for as words of the command line, in
/// quotes and nested shells too, so that a call that merely names one is not reversible either.
const BEYOND_THE_WORKTREE: &[&str] = &[
    "sudo", "su", "doas", "env", "printenv", "export", "declare", "curl", "wget", "ssh", "scp",
    "rsync", "sftp", "ftp", "nc", "ncat", "netcat", "telnet", "socat", "kill", "pkill", "killall",
    "setsid", "nohup", "npm", "npx", "pnpm", "yarn", "pip", "pip3", "pipx", "uv", "cargo", "go",
    "gem", "bundle", "apt", "apt-get", "brew", "dnf", "yum", "apk", "python", "python3", "node",
    "perl", "ruby", "php", "lua", "deno", "bun",
];

/// The git subcommands that talk to another repository.
const GIT_REMOTE: &[&str] = &[
    "push",
    "fetch",
    "pull",
    "clone",
    "remote",
    "ls-remote",
    "submodule",
    "send-email",
];

/// Whether running the call again in a copy of the worktree leaves everything outside the copy as it was: the file
/// tools (`read`, `write`, `edit`, `ls`, `grep`, `find`) on paths inside the worktree, and `bash` commands that, as the
/// patterns analyse them (nested shells, `cd`, redirects), name only paths inside the worktree and use neither the
/// network, nor other repositories, nor the environment, nor the programs in [`BEYOND_THE_WORKTREE`]. This reads the
/// command line; it does not sandbox it, and anything it cannot follow (a variable, a substitution, a shell nested
/// too deep) is not reversible. The error says why not.
pub fn reversible(tool: &str, input: &Value, worktree: &Path) -> Result<(), String> {
    let inside = |path: &str| {
        relative_to(worktree, path)
            .map(drop)
            .ok_or_else(|| format!("{path} is not inside the worktree"))
    };
    match tool {
        "read" | "write" | "edit" => {
            inside(input["path"].as_str().ok_or("the call names no path")?)
        }
        "ls" | "grep" | "find" => inside(input["path"].as_str().unwrap_or(".")),
        "bash" => {
            let call = ToolCall {
                tool,
                input: input.clone(),
                worktree: Some(worktree),
            };
            command_reversible(&call)
        }
        other => Err(format!(
            "{other} is not a tool whose effects stay in the worktree"
        )),
    }
}

/// The places a word of a command line may name: the word, the value after `=` (`--option=/path`, `NAME=/path`) and
/// the value attached to a short option (`-t/path`).
fn candidates(word: &str) -> Vec<&str> {
    let mut found = vec![word];
    if let Some((_, value)) = word.split_once('=') {
        found.push(value);
    }
    let bytes = word.as_bytes();
    if bytes.len() > 2 && bytes[0] == b'-' && bytes[1] != b'-' && word.is_char_boundary(2) {
        found.push(&word[2..]);
    }
    found
}

/// Whether `path`, taken from `cwd` (inside `root`), steps above `root` at any point, even if it comes back: the
/// kernel stops `..` at the top of the file system, so a path with more `..` than the worktree is deep can land on
/// the original worktree when the command runs in a copy elsewhere.
fn climbs_above(root: &Path, cwd: &Path, path: &str) -> bool {
    if path.starts_with('/') || !path.contains("..") {
        return false;
    }
    let mut depth = cwd.strip_prefix(root).map_or(0, |r| r.components().count()) as i64;
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => depth -= 1,
            _ => depth += 1,
        }
        if depth < 0 {
            return true;
        }
    }
    false
}

/// Whether the call can be replayed: it is [`reversible`], and a `bash` command names no way to the original
/// worktree. A replay runs the command unchanged in a copy of the worktree, so an absolute path (also as the value of
/// an option or of a `NAME=value` prefix, or in a brace expansion), or more `..` than the worktree is deep, even when
/// it names a place inside the original worktree, would act on the original and not on the copy.
pub fn replayable(tool: &str, input: &Value, worktree: &Path) -> Result<(), String> {
    reversible(tool, input, worktree)?;
    if tool != "bash" {
        return Ok(());
    }
    let call = ToolCall {
        tool,
        input: input.clone(),
        worktree: Some(worktree),
    };
    static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
        // `NAME=value` prefixes are not among the words of a command, so the text is read for them.
        Regex::new(r"(?:^|[\s;&|(){}`'\x22])[A-Za-z_][A-Za-z0-9_]*=[^\s;&|(){}`'\x22]*(?:/|\.\.)")
            .expect("pattern compiles")
    });
    let allowed = ["/dev/null", "/dev/stdout", "/dev/stderr"];
    let command = call.command();
    let assignment = ASSIGNMENT
        .find_iter(command)
        .map(|m| m.as_str())
        .find(|m| !allowed.iter().any(|a| m.ends_with(a)));
    if let Some(found) = assignment {
        return Err(format!(
            "{} is an absolute path, which a replay would not keep in the copy",
            found.trim_start_matches(|c: char| !c.is_alphanumeric() && c != '_')
        ));
    }
    let root = normalise(worktree);
    let found = std::cell::RefCell::new(None);
    any_simple(&call, &|simple, cwd| {
        for word in simple.words.iter().chain(&simple.writes) {
            for path in candidates(word) {
                let absolute = path.starts_with('/') && !allowed.contains(&path);
                let braced = path.contains('{') && path.contains('/');
                if absolute || braced {
                    found.borrow_mut().get_or_insert(format!(
                        "{word} is an absolute path, which a replay would not keep in the copy"
                    ));
                } else if climbs_above(&root, cwd, path) {
                    found.borrow_mut().get_or_insert(format!(
                        "{word} is an absolute path in disguise: it climbs above the worktree, which a replay would not keep in the copy"
                    ));
                }
            }
        }
        found.borrow().is_some()
    });
    match found.into_inner() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn command_reversible(call: &ToolCall) -> Result<(), String> {
    static BEYOND: LazyLock<Regex> = LazyLock::new(|| {
        let names = BEYOND_THE_WORKTREE.join("|");
        Regex::new(&format!(
            r#"(?:^|[\s;&|(){{}}`'"=<>])({names})(?:$|[\s;&|(){{}}`'"<>])"#
        ))
        .expect("pattern compiles")
    });
    let command = call.command();
    if let Some(found) = BEYOND.captures(command) {
        return Err(format!("it uses {}", &found[1]));
    }
    if command.contains('$') {
        return Err("it uses a variable or a substitution".into());
    }
    let root = normalise(call.worktree.unwrap_or(Path::new("/")));
    let stray = |cwd: &Path, word: &str| -> Option<String> {
        if word.contains("://") {
            return Some(format!("{word} is on the network"));
        }
        // The value of `--option=/path` is a path too.
        let path = word
            .strip_prefix('-')
            .and_then(|option| option.split_once('='))
            .map_or(word, |(_, value)| value);
        let looks_like_a_path = path.starts_with('/')
            || is_home(path)
            || path == ".."
            || path.contains("../")
            || path.ends_with("/..");
        let allowed = ["/dev/null", "/dev/stdout", "/dev/stderr"].contains(&path);
        (looks_like_a_path && !allowed && (is_home(path) || !resolve(cwd, path).starts_with(&root)))
            .then(|| format!("{path} is not inside the worktree"))
    };
    let problem = |simple: &Simple, cwd: &Path| -> Option<String> {
        if !cwd.starts_with(&root) {
            return Some("it leaves the worktree".to_string());
        }
        if let Some((name, args)) = simple.words.split_first()
            && name == "git"
            && let Some((subcommand, _)) = git_subcommand(args)
            && GIT_REMOTE.contains(&subcommand)
        {
            return Some(format!("git {subcommand} reaches another repository"));
        }
        simple
            .words
            .iter()
            .chain(&simple.writes)
            .find_map(|word| stray(cwd, word))
    };
    let reason = std::cell::RefCell::new(None);
    let reversible = !any_simple(call, &|simple, cwd| match problem(simple, cwd) {
        Some(why) => {
            reason.borrow_mut().get_or_insert(why);
            true
        }
        None => false,
    });
    match (reversible, reason.into_inner()) {
        (true, _) => Ok(()),
        (false, Some(reason)) => Err(reason),
        (false, None) => Err("it nests shells too deeply to follow".into()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::decide::guard::WORKTREE;
    use crate::decide::guard::shell::MAX_NESTING;

    fn reversible_bash(command: &str) -> Result<(), String> {
        reversible("bash", &json!({ "command": command }), Path::new(WORKTREE))
    }

    #[test]
    fn commands_that_stay_in_the_worktree_are_reversible() {
        for command in [
            "rm -rf src",
            "rm -rf ./src ./target",
            "echo hi > out.txt",
            "echo hi >> a/../out.txt",
            "echo hi > /srv/wt/1/out.txt",
            "echo hi > /dev/null 2>&1",
            "cat a.txt | wc -l | tee count.txt",
            "mkdir -p build/x && touch build/x/y",
            "cd src && rm -rf ../build",
            "cd src; ls; cd ..; rm -r build",
            "bash -c 'rm -rf src'",
            "sh -c \"bash -c 'cd src; rm x'\"",
            "git status",
            "git -C /srv/wt/1 log --oneline",
            "grep -rn TODO . | sort > todos.txt",
            "sed -i s/a/b/ src/main.rs",
            "make clean",
        ] {
            assert_eq!(reversible_bash(command), Ok(()), "{command}");
        }
    }

    #[test]
    fn a_replay_refuses_commands_with_absolute_paths_even_inside_the_worktree() {
        let replayable =
            |command: &str| replayable("bash", &json!({ "command": command }), Path::new(WORKTREE));
        for command in [
            "cd /srv/wt/1 && git reset --hard HEAD~2 && rm -rf src",
            "rm -rf /srv/wt/1/src",
            "echo hi > /srv/wt/1/out.txt",
            "git -C /srv/wt/1 log",
            "bash -c 'cd /srv/wt/1/a/..; rm x'",
            "cp --target-directory=/srv/wt/1/x a",
            "cd ../../../../../../../srv/wt/1 && git reset --hard HEAD~2 && rm -rf src",
            "rm -rf ../../../../../../../srv/wt/1/src",
            "cd src && rm -rf ../../../../../../srv/wt/1/src",
            "GIT_DIR=/srv/wt/1/.git GIT_WORK_TREE=/srv/wt/1 git reset --hard HEAD~2",
            "FOO=../../../../../../srv/wt/1 rm x",
            "mv -t/srv/wt/1 src",
            "sort -o/srv/wt/1/Cargo.toml x",
            "rm -rf {/srv/wt/1/src,}",
            "bash -c 'GIT_DIR=/srv/wt/1/.git git clean -fdx'",
        ] {
            let error = replayable(command).expect_err(command);
            assert!(error.contains("absolute"), "{command}: {error}");
        }
        assert_eq!(replayable("rm -rf src && echo hi > /dev/null 2>&1"), Ok(()));
        assert_eq!(replayable("echo hi > out.txt"), Ok(()));
        assert_eq!(replayable("cd src && rm -rf ../build"), Ok(()));
        assert_eq!(replayable("sed -i s/a/b/ src/main.rs"), Ok(()));
        // What reversible refuses is refused still.
        assert!(replayable("rm -rf /etc").is_err());
    }

    #[test]
    fn commands_that_reach_beyond_the_worktree_are_not_reversible() {
        for (command, why) in [
            (
                "git push origin main",
                "git push reaches another repository",
            ),
            ("git -C . fetch", "git fetch reaches another repository"),
            ("git clone /srv/other x", "git clone"),
            ("git remote add o /srv/x", "git remote"),
            ("curl https://example.com", "it uses curl"),
            ("wget -q http://x/y", "it uses wget"),
            ("ssh host ls", "it uses ssh"),
            ("scp a host:b", "it uses scp"),
            ("rsync -a . host:/x", "it uses rsync"),
            ("nc -l 9", "it uses nc"),
            ("npm install", "it uses npm"),
            ("pip install x", "it uses pip"),
            ("cargo install ripgrep", "it uses cargo"),
            ("sudo rm -rf src", "it uses sudo"),
            ("kill -9 1", "it uses kill"),
            ("pkill -f x", "it uses pkill"),
            ("python3 -c 'import os'", "it uses python3"),
            ("env", "it uses env"),
            ("make test && curl -d @out x", "it uses curl"),
            ("echo done; sudo reboot", "it uses sudo"),
            ("echo $API_KEY", "it uses a variable"),
            ("cat ~/.ssh/id_rsa", "is not inside the worktree"),
            ("rm -rf /", "/ is not inside the worktree"),
            ("cd / && rm -rf x", "/ is not inside the worktree"),
            ("cd .. && ls", ".. is not inside the worktree"),
            ("cd; rm -rf x", "it leaves the worktree"),
            ("cat ../other/file", "is not inside the worktree"),
            (
                "echo x > /etc/passwd",
                "/etc/passwd is not inside the worktree",
            ),
            ("echo x > /tmp/y", "/tmp/y is not inside the worktree"),
            ("echo x 2>> ../log", "../log is not inside the worktree"),
            ("bash -c 'cd /; rm -rf x'", "/ is not inside the worktree"),
            ("sh -c \"bash -c 'git push'\"", "git push"),
            ("eval 'curl x'", "it uses curl"),
            (
                "cp --target-directory=/etc a",
                "/etc is not inside the worktree",
            ),
            ("echo $(curl x)", "it uses"),
            ("xargs -a /etc/passwd ls", "/etc/passwd"),
        ] {
            let error = reversible_bash(command).expect_err(command);
            assert!(error.contains(why), "{command}: {error}");
        }
        let mut deep = "echo hi".to_string();
        for _ in 0..=MAX_NESTING {
            deep = format!("sh -c '{}'", deep.replace('\'', "'\"'\"'"));
        }
        assert!(reversible_bash(&deep).unwrap_err().contains("too deeply"));
    }

    #[test]
    fn the_file_tools_are_reversible_inside_the_worktree_only() {
        let check = |tool: &str, input: Value| reversible(tool, &input, Path::new(WORKTREE));
        for (tool, input) in [
            ("read", json!({"path": "src/main.rs"})),
            ("write", json!({"path": "/srv/wt/1/a.txt", "content": "x"})),
            ("edit", json!({"path": "./a/../b.txt"})),
            ("ls", json!({})),
            ("grep", json!({"pattern": "x", "path": "src"})),
            ("find", json!({"pattern": "*.rs"})),
        ] {
            assert_eq!(check(tool, input), Ok(()), "{tool}");
        }
        for (tool, input) in [
            ("read", json!({"path": "/etc/passwd"})),
            ("write", json!({"path": "../x", "content": "x"})),
            ("edit", json!({"path": "~/x"})),
            ("ls", json!({"path": "/"})),
            ("grep", json!({"pattern": "x", "path": "/srv/wt/10"})),
            ("write", json!({"content": "x"})),
            ("browser", json!({"path": "a"})),
        ] {
            assert!(check(tool, input).is_err(), "{tool}");
        }
        assert_eq!(
            relative_to(Path::new(WORKTREE), "/srv/wt/1/a/../b.txt"),
            Some(PathBuf::from("b.txt"))
        );
        assert_eq!(relative_to(Path::new(WORKTREE), "/srv/wt/10/b"), None);
    }
}
