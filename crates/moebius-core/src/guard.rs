//! The `guard` decider (spec 3.2, ADR 0007): a compiled-in plugin that judges `tool_call` questions. It first checks
//! the built-in rules, which are fixed patterns over the call (see [`BUILT_IN`]), and refuses with the name of the
//! rule broken. A call that breaks none is allowed.

use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use moebius_plugin_api::{Call, DecideRequest, Decision, Kind, Manifest, Plugin, PluginError};
use regex::Regex;
use serde_json::Value;

/// The plugin's name, which `Decisions` routes `tool_call` to.
pub const NAME: &str = "guard";

/// A tool call as the patterns see it.
struct ToolCall<'a> {
    tool: &'a str,
    input: Value,
    worktree: Option<&'a Path>,
}

impl ToolCall<'_> {
    fn command(&self) -> &str {
        self.input["command"].as_str().unwrap_or_default()
    }

    /// Whether `path` is somewhere other than inside the run's worktree. A run without a worktree has no place
    /// that is its own.
    fn outside_worktree(&self, path: &str) -> bool {
        let Some(worktree) = self.worktree else {
            return true;
        };
        if path.starts_with('~') || path.starts_with("$HOME") || path.starts_with("${HOME}") {
            return true;
        }
        !resolve(worktree, path).starts_with(normalise(worktree))
    }
}

/// A rule checked by a pattern: its name, what it says (which is the reason given when it is broken) and whether a
/// call breaks it.
struct BuiltIn {
    name: &'static str,
    prose: &'static str,
    broken_by: fn(&ToolCall) -> bool,
}

const BUILT_IN: [BuiltIn; 4] = [
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
];

/// The words of each simple command of `command`, with quotes and leading `sudo`, `env` and `NAME=value` removed.
fn simple_commands(command: &str) -> Vec<Vec<&str>> {
    command
        .split([';', '|', '&', '\n', '(', ')', '`'])
        .map(|part| {
            part.split_whitespace()
                .map(|word| word.trim_matches(['"', '\'']))
                .skip_while(|word| {
                    matches!(*word, "sudo" | "env" | "command") || word.contains('=')
                })
                .collect::<Vec<_>>()
        })
        .filter(|words| !words.is_empty())
        .collect()
}

fn destructive_command(call: &ToolCall) -> bool {
    static FORK_BOMB: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r":\s*\(\s*\)\s*\{[^}]*:\s*\|\s*:").expect("pattern compiles"));
    call.tool == "bash"
        && (FORK_BOMB.is_match(call.command())
            || simple_commands(call.command()).iter().any(|words| {
                let args = &words[1..];
                match words[0] {
                    "rm" => recursive_rm_outside_worktree(call, args),
                    "dd" => args.iter().any(|arg| arg.starts_with("of=/dev/")),
                    command => command.starts_with("mkfs"),
                }
            }))
}

/// `rm` with a recursive flag (`-r`, `-R`, `-rf`, `-fr`, `--recursive`) and a target outside the worktree.
fn recursive_rm_outside_worktree(call: &ToolCall, args: &[&str]) -> bool {
    let recursive = args.iter().any(|arg| {
        *arg == "--recursive"
            || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains(['r', 'R']))
    });
    recursive
        && args
            .iter()
            .filter(|arg| !arg.starts_with('-'))
            .any(|target| call.outside_worktree(target))
}

fn force_push(call: &ToolCall) -> bool {
    if call.tool != "bash" {
        return false;
    }
    simple_commands(call.command()).iter().any(|words| {
        if words[0] != "git" {
            return false;
        }
        // The subcommand is the first word that is not an option (or the value of -C or -c).
        let mut rest = words[1..].iter();
        let mut subcommand = None;
        while let Some(word) = rest.next() {
            match *word {
                "-C" | "-c" => {
                    rest.next();
                }
                _ if word.starts_with('-') => {}
                _ => {
                    subcommand = Some(word);
                    break;
                }
            }
        }
        subcommand == Some(&"push")
            && rest.any(|arg| {
                arg.starts_with("--force")
                    || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('f'))
                    || (arg.starts_with('+') && arg.len() > 1)
            })
    })
}

fn write_outside_worktree(call: &ToolCall) -> bool {
    matches!(call.tool, "write" | "edit")
        && call.input["path"]
            .as_str()
            .is_none_or(|path| call.outside_worktree(path))
}

fn secret_in_command(call: &ToolCall) -> bool {
    call.tool == "bash" && crate::blobs::redact(call.command()) != call.command()
}

/// `path` against `base` when it is relative, with `.` and `..` resolved without touching the disk.
fn resolve(base: &Path, path: &str) -> PathBuf {
    normalise(&base.join(path))
}

fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The name of the first built-in rule the call breaks, with what the rule says.
fn broken_rule(call: &ToolCall) -> Option<String> {
    BUILT_IN
        .iter()
        .find(|rule| (rule.broken_by)(call))
        .map(|rule| format!("{}: {}", rule.name, rule.prose))
}

/// The compiled-in decider for `tool_call` questions.
pub struct Guard;

impl Plugin for Guard {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: NAME.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kinds: vec![Kind::Decider],
        }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        let request: DecideRequest = serde_json::from_value(call.params)
            .map_err(|e| PluginError::BadParams(e.to_string()))?;
        if request.kind != "tool_call" {
            return Err(PluginError::Failed(format!(
                "guard decides tool calls, not {:?}",
                request.kind
            )));
        }
        let context = &request.context;
        let tool_call = ToolCall {
            tool: context.get("tool").map_or("", String::as_str),
            input: context
                .get("input")
                .and_then(|input| serde_json::from_str(input).ok())
                .unwrap_or_default(),
            worktree: context.get("worktree").map(Path::new),
        };
        // The prose rules in `context["rules"]` are for the teacher to judge once the patterns have passed.
        let decision = match broken_rule(&tool_call) {
            Some(reason) => Decision {
                choice: "refuse".into(),
                reason,
            },
            None => Decision {
                choice: "allow".into(),
                reason: "no built-in rule is broken".into(),
            },
        };
        serde_json::to_value(decision).map_err(|e| PluginError::Failed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use moebius_plugin_api::Host;
    use serde_json::json;

    use super::*;

    const WORKTREE: &str = "/srv/wt/1";

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

    fn decide(tool: &str, input: Value) -> moebius_plugin_api::Decision {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Guard);
        let request = DecideRequest {
            kind: "tool_call".into(),
            context: BTreeMap::from([
                ("tool".to_string(), tool.to_string()),
                ("input".to_string(), input.to_string()),
                ("worktree".to_string(), WORKTREE.to_string()),
            ]),
            options: vec!["allow".into(), "refuse".into()],
        };
        host.decide(NAME, &request).unwrap()
    }

    #[test]
    fn the_plugin_refuses_with_the_rules_name_and_allows_the_rest() {
        let refused = decide("bash", json!({ "command": "git push --force" }));
        assert_eq!(refused.choice, "refuse");
        assert_eq!(
            refused.reason,
            "no-force-push: git push --force rewrites history on the remote"
        );
        let refused = decide("write", json!({ "path": "/etc/passwd" }));
        assert!(refused.reason.starts_with("stay-in-the-worktree: "));
        assert_eq!(
            decide("bash", json!({ "command": "cargo test" })).choice,
            "allow"
        );
        assert_eq!(
            decide("read", json!({ "path": "/etc/hosts" })).choice,
            "allow"
        );
    }

    #[test]
    fn other_questions_are_not_the_guards() {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Guard);
        let request = DecideRequest {
            kind: "merge".into(),
            context: BTreeMap::new(),
            options: vec!["allow".into(), "refuse".into()],
        };
        assert!(host.decide(NAME, &request).is_err());
    }
}
