//! The guard's built-in rules (see [`BUILT_IN`]): fixed patterns over a tool call, tested before any rule in prose. A
//! call that breaks one is refused with the rule's name and what it says. The command line is read as `shell` reads
//! it, nested shells and `cd` included.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::shell::{ToolCall, any_simple, git_subcommand, resolve};

/// A rule checked by a pattern: its name, what it says (which is the reason given when it is broken) and whether a
/// call breaks it.
struct BuiltIn {
    name: &'static str,
    prose: &'static str,
    broken_by: fn(&ToolCall) -> bool,
}

const BUILT_IN: [BuiltIn; 5] = [
    BuiltIn {
        name: "no-destructive-commands",
        prose: "commands that delete recursively outside the worktree, format a disk, overwrite a device or exhaust the machine are never run",
        broken_by: destructive_command,
    },
    BuiltIn {
        name: "no-force-push",
        prose: "git push --force rewrites history on the remote",
        broken_by: force_push,
    },
    BuiltIn {
        name: "stay-in-the-worktree",
        prose: "files are written only inside the run's worktree",
        broken_by: write_outside_worktree,
    },
    BuiltIn {
        name: "no-secrets-in-commands",
        prose: "a command must not contain a secret; read it from the environment instead",
        broken_by: secret_in_command,
    },
    BuiltIn {
        name: "no-dumping-large-files",
        prose: "cat, less and more do not print a file over 64 KB whole; read a range of its lines (the read tool's offset and limit, or sed -n '100,200p' FILE) or grep for what you need",
        broken_by: dumps_large_file,
    },
];

/// The largest file `cat`, `less` and `more` may print whole, in bytes.
const LARGE_FILE: u64 = 64 * 1024;

/// The built-in rules as a person reads them: each name with what it refuses, in the order they are tested.
pub fn patterns() -> Vec<(&'static str, &'static str)> {
    BUILT_IN
        .iter()
        .map(|rule| (rule.name, rule.prose))
        .collect()
}

fn destructive_command(call: &ToolCall) -> bool {
    static FORK_BOMB: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r":\s*\(\s*\)\s*\{[^}]*:\s*\|\s*:").expect("pattern compiles"));
    call.tool == "bash"
        && (FORK_BOMB.is_match(call.command())
            || any_simple(call, &|simple, cwd| {
                let Some((name, args)) = simple.words.split_first() else {
                    return false;
                };
                match name.as_str() {
                    "rm" => recursive_rm_outside_worktree(call, cwd, args),
                    "dd" => args.iter().any(|arg| arg.starts_with("of=/dev/")),
                    command => command.starts_with("mkfs"),
                }
            }))
}

/// `rm` with a recursive flag (`-r`, `-R`, `-rf`, `-fr`, `--recursive`) and a target outside the worktree.
fn recursive_rm_outside_worktree(call: &ToolCall, cwd: &Path, args: &[String]) -> bool {
    let recursive = args.iter().any(|arg| {
        arg == "--recursive"
            || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains(['r', 'R']))
    });
    recursive
        && args
            .iter()
            .filter(|arg| !arg.starts_with('-'))
            .any(|target| call.outside_worktree(cwd, target))
}

fn force_push(call: &ToolCall) -> bool {
    call.tool == "bash"
        && any_simple(call, &|simple, _| {
            let Some((name, args)) = simple.words.split_first() else {
                return false;
            };
            if name != "git" {
                return false;
            }
            git_subcommand(args).is_some_and(|(subcommand, rest)| {
                subcommand == "push"
                    && rest.iter().any(|arg| {
                        arg.starts_with("--force")
                            || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('f'))
                            || (arg.starts_with('+') && arg.len() > 1)
                    })
            })
        })
}

/// A `write` or `edit` to a path outside the worktree, or a shell command that redirects output (`>`, `>>`) or
/// pipes into `tee` outside it.
fn write_outside_worktree(call: &ToolCall) -> bool {
    match call.tool {
        "write" | "edit" => call.input["path"]
            .as_str()
            .is_none_or(|path| call.outside_worktree(&call.start_dir(), path)),
        "bash" => any_simple(call, &|simple, cwd| {
            let tee_targets = match simple.words.split_first() {
                Some((name, args)) if name == "tee" => {
                    args.iter().filter(|arg| !arg.starts_with('-')).collect()
                }
                _ => Vec::new(),
            };
            simple
                .writes
                .iter()
                .chain(tee_targets)
                .any(|target| call.writes_elsewhere(cwd, target))
        }),
        _ => false,
    }
}

fn secret_in_command(call: &ToolCall) -> bool {
    call.tool == "bash" && crate::blobs::redact(call.command()) != call.command()
}

/// `cat`, `less` or `more` of a file in the worktree larger than [`LARGE_FILE`], printed for the model to read: not
/// piped into another command or redirected to a file.
fn dumps_large_file(call: &ToolCall) -> bool {
    call.tool == "bash"
        && !call.command().contains('|')
        && any_simple(call, &|simple, cwd| {
            let Some((name, args)) = simple.words.split_first() else {
                return false;
            };
            ["cat", "less", "more"].contains(&name.as_str())
                && simple.writes.is_empty()
                && args.iter().filter(|arg| !arg.starts_with('-')).any(|file| {
                    std::fs::metadata(resolve(cwd, file))
                        .is_ok_and(|meta| meta.is_file() && meta.len() > LARGE_FILE)
                })
        })
}

/// The name of the first built-in rule the call breaks, with what the rule says.
pub(super) fn broken_rule(call: &ToolCall) -> Option<String> {
    BUILT_IN
        .iter()
        .find(|rule| (rule.broken_by)(call))
        .map(|rule| format!("{}: {}", rule.name, rule.prose))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::decide::guard::WORKTREE;
    use crate::decide::guard::shell::MAX_NESTING;

    /// The name of the rule that `tool` with `input` breaks in the worktree, if any.
    fn broken(tool: &str, input: Value) -> Option<&'static str> {
        let call = ToolCall {
            tool,
            input,
            worktree: Some(Path::new(WORKTREE)),
        };
        BUILT_IN
            .iter()
            .find(|rule| (rule.broken_by)(&call))
            .map(|rule| rule.name)
    }

    #[test]
    fn the_patterns_are_listed_with_what_they_refuse() {
        let patterns = patterns();
        assert_eq!(
            patterns.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            [
                "no-destructive-commands",
                "no-force-push",
                "stay-in-the-worktree",
                "no-secrets-in-commands",
                "no-dumping-large-files"
            ]
        );
        let call = ToolCall {
            tool: "bash",
            input: json!({ "command": "git push --force" }),
            worktree: Some(Path::new(WORKTREE)),
        };
        let (name, description) = patterns[1];
        assert_eq!(broken_rule(&call), Some(format!("{name}: {description}")));
    }

    fn bash(command: &str) -> Option<&'static str> {
        broken("bash", json!({ "command": command }))
    }

    #[test]
    fn recursive_deletes_outside_the_worktree_and_other_destruction_are_refused() {
        let rule = Some("no-destructive-commands");
        for command in [
            "rm -rf /",
            "rm -rf /*",
            "rm -rf ~",
            "rm -rf ~/work",
            "rm -rf $HOME",
            "rm -rf ..",
            "rm -rf ../other",
            "rm -fr /",
            "rm -r -f /",
            "rm --recursive --force /",
            "rm -R /etc",
            "rm -rf /srv/wt/10",
            "cd build && sudo rm -rf /",
            "echo done; rm -rf /var",
            "mkfs.ext4 /dev/sda1",
            "mkfs /dev/sda1",
            "dd if=/dev/zero of=/dev/sda bs=1M",
            ":(){ :|:& };:",
        ] {
            assert_eq!(bash(command), rule, "{command}");
        }
    }

    #[test]
    fn deletes_inside_the_worktree_and_harmless_look_alikes_are_allowed() {
        for command in [
            "rm -rf ./target",
            "rm -rf build/",
            "rm -rf target/debug ./a/../b",
            "rm -rf /srv/wt/1/target",
            "rm file.txt",
            "rm -f /tmp/x",
            "dd if=/dev/zero of=./disk.img bs=1M count=1",
            "echo mkfs",
            "ls /",
        ] {
            assert_eq!(bash(command), None, "{command}");
        }
    }

    #[test]
    fn a_cd_moves_where_relative_paths_are() {
        for command in [
            "cd / && rm -rf *",
            "cd .. && rm -rf src",
            "cd ~ && rm -rf .",
            "cd && rm -rf build",
            "cd $HOME; rm -rf build",
            "cd /tmp && rm -rf build",
            "cd src && cd ../.. && rm -rf x",
        ] {
            assert_eq!(bash(command), Some("no-destructive-commands"), "{command}");
        }
        for command in [
            "cd src && rm -rf build",
            "cd src/.. && rm -rf build",
            "cd /srv/wt/1/src && rm -rf build",
            "cd / && cd /srv/wt/1 && rm -rf build",
        ] {
            assert_eq!(bash(command), None, "{command}");
        }
        assert_eq!(
            bash("cd .. && echo x > out.txt"),
            Some("stay-in-the-worktree")
        );
    }

    #[test]
    fn nested_shells_are_looked_into() {
        for command in [
            "bash -c \"rm -rf /\"",
            "sh -c 'rm -rf /'",
            "zsh -c 'cd /; rm -rf *'",
            "sudo bash -lc 'echo hi && rm -rf /etc'",
            "dash -c \"sh -c 'bash -c \\\"rm -rf /\\\"'\"",
            "eval rm -rf /",
            "eval \"cd ..; rm -rf src\"",
            "xargs rm -rf /",
            "xargs -n1 sudo rm -rf ../x",
        ] {
            assert_eq!(bash(command), Some("no-destructive-commands"), "{command}");
        }
        assert_eq!(bash("sh -c 'git push -f'"), Some("no-force-push"));
        assert_eq!(
            bash("bash -c 'echo x > /etc/passwd'"),
            Some("stay-in-the-worktree")
        );
        for command in [
            "bash -c \"cargo build\"",
            "bash script.sh",
            "eval echo hi",
            "xargs rm -rf build",
            "xargs",
            "bash -c \"cd src; rm -rf build\"",
        ] {
            assert_eq!(bash(command), None, "{command}");
        }
    }

    #[test]
    fn shells_nested_too_deep_are_refused() {
        let mut command = "echo hi".to_string();
        for depth in 1..=MAX_NESTING + 1 {
            command = format!("sh -c '{}'", command.replace('\'', "'\"'\"'"));
            let expected = (depth > MAX_NESTING).then_some("no-destructive-commands");
            assert_eq!(bash(&command), expected, "depth {depth}");
        }
    }

    #[test]
    fn redirects_and_tee_outside_the_worktree_are_refused() {
        for command in [
            "echo x > /etc/passwd",
            "echo x>/etc/passwd",
            "echo x >> ../outside.txt",
            "echo x 2> /etc/err",
            "make &> ~/log",
            "make >& /etc/log",
            "ls | tee /etc/x",
            "ls | tee -a ../x",
            "ls | tee out.txt /etc/x",
            "echo x > /tmp/../etc/x",
            "echo x > \"/etc/a b\"",
            "cd .. && echo x > y",
        ] {
            assert_eq!(bash(command), Some("stay-in-the-worktree"), "{command}");
        }
    }

    #[test]
    fn redirects_that_stay_or_go_to_harmless_places_are_allowed() {
        for command in [
            "echo hi > out.txt",
            "echo hi >> ./a/../out.txt",
            "echo hi > /srv/wt/1/out.txt",
            "cargo test 2>&1 | tee /tmp/log",
            "cmd > /dev/null 2>&1",
            "cmd &> /dev/null",
            "cmd >&2",
            "cmd 2>/dev/stderr >/dev/stdout",
            "cmd >/tmp/x/y.log",
            "ls | tee out.txt",
            "echo \"a > /etc/b\"",
            "echo 'x; rm -rf /'",
            "cat < /etc/hosts",
            "a && b || c &",
        ] {
            assert_eq!(bash(command), None, "{command}");
        }
    }

    #[test]
    fn force_pushes_are_refused() {
        for command in [
            "git push --force",
            "git push -f origin main",
            "git push origin main --force-with-lease",
            "git push --force-with-lease=main origin",
            "git push origin +main",
            "git push origin +HEAD:main",
            "git -C repo push -f",
            "git push -uf origin main",
        ] {
            assert_eq!(bash(command), Some("no-force-push"), "{command}");
        }
    }

    #[test]
    fn ordinary_git_is_allowed() {
        for command in [
            "git push",
            "git push origin HEAD",
            "git push -u origin feature",
            "git commit -m \"push -f\"",
            "git status",
            "echo git push --force",
            "git log --format=%h origin/+main",
        ] {
            assert_eq!(bash(command), None, "{command}");
        }
    }

    #[test]
    fn writes_must_stay_in_the_worktree() {
        for path in [
            "/etc/passwd",
            "../outside.txt",
            "src/../../outside.txt",
            "/srv/wt/10/file",
            "/srv/wt/1/../2/file",
            "~/.bashrc",
        ] {
            for tool in ["write", "edit"] {
                assert_eq!(
                    broken(tool, json!({ "path": path })),
                    Some("stay-in-the-worktree"),
                    "{tool} {path}"
                );
            }
        }
        assert_eq!(
            broken("write", json!({})),
            Some("stay-in-the-worktree"),
            "no path"
        );
        for path in ["src/main.rs", "./a/../b.txt", "/srv/wt/1/src/main.rs"] {
            assert_eq!(broken("write", json!({ "path": path })), None, "{path}");
            assert_eq!(broken("edit", json!({ "path": path })), None, "{path}");
        }
        assert_eq!(broken("read", json!({ "path": "/etc/hosts" })), None);
    }

    #[test]
    fn a_run_without_a_worktree_may_not_write() {
        let call = ToolCall {
            tool: "write",
            input: json!({ "path": "src/main.rs" }),
            worktree: None,
        };
        assert!(write_outside_worktree(&call));
    }

    #[test]
    fn secrets_in_commands_are_refused() {
        let key = format!("sk-ant-{}", "k".repeat(30));
        assert_eq!(
            bash(&format!("curl -H 'x-api-key: {key}' https://example.com")),
            Some("no-secrets-in-commands")
        );
        assert_eq!(
            bash("curl -H \"x-api-key: $ANTHROPIC_API_KEY\" https://example.com"),
            None
        );
        // Only commands are checked; what a file contains is the redaction of results.
        assert_eq!(
            broken("write", json!({ "path": "k.txt", "content": key })),
            None
        );
    }

    #[test]
    fn printing_a_large_file_whole_is_refused_with_a_way_to_read_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("big.log"),
            "x".repeat(LARGE_FILE as usize + 1),
        )
        .unwrap();
        std::fs::write(dir.path().join("small.txt"), "hello\n").unwrap();
        let broken = |command: &str| {
            let call = ToolCall {
                tool: "bash",
                input: json!({ "command": command }),
                worktree: Some(dir.path()),
            };
            broken_rule(&call)
        };
        for command in [
            "cat big.log",
            "less ./big.log",
            "cat small.txt big.log",
            "cat -n big.log",
        ] {
            let reason = broken(command).unwrap_or_else(|| panic!("{command} was allowed"));
            assert!(reason.starts_with("no-dumping-large-files: "), "{reason}");
            assert!(
                reason.contains("grep") && reason.contains("sed -n"),
                "{reason}"
            );
        }
        for command in [
            "cat small.txt",
            "cat big.log | grep error",
            "cat big.log > copy.log",
            "grep error big.log",
            "sed -n '1,100p' big.log",
            "cat missing.txt",
        ] {
            assert_eq!(broken(command), None, "{command}");
        }
    }
}
