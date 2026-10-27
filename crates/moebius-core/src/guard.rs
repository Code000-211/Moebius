//! The `guard` decider (spec 3.2, ADR 0007): a compiled-in plugin that judges `tool_call` questions in two steps.
//!
//! 1. The built-in rules are fixed patterns over the call (see [`BUILT_IN`]). A call that breaks one is refused with
//!    the rule's name, and the judgement's decider is `guard/patterns`.
//! 2. The rules in prose (`context["rules"]`, see `check`) are judged by the [`Teacher`]. Without prose rules the call
//!    is allowed and nobody is asked. With them and a teacher configured (`[teacher]` in the configuration) the
//!    teacher is asked and the judgement's decider is `guard/teacher`; with them and no teacher the call is refused
//!    (fail closed). The teacher also refuses when it fails, is too slow or answers anything but the JSON asked for.
//!    A refusal's reason starts with the rule's name.
//!
//! A question whose context has a `rules_error` (the rules files could not be read) is refused outright, by
//! `guard/patterns`.
//!
//! The question's context also carries `run_id` and `card_id` (when the run has a card), so that the teacher's tokens
//! are counted against the run being checked.

use std::collections::BTreeMap;
use std::iter::Peekable;
use std::path::{Component, Path, PathBuf};
use std::str::Chars;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use moebius_plugin_api::{Call, DecideRequest, Decision, Kind, Manifest, Plugin, PluginError};
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::TeacherConfig;
use crate::router::{Grant, Protocol, Router};
use crate::rules::{self, Rule};

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

    /// Where a command starts: the worktree, or `/` for a run without one.
    fn start_dir(&self) -> PathBuf {
        self.worktree.unwrap_or(Path::new("/")).to_path_buf()
    }

    /// Whether `path`, relative to `cwd`, is somewhere other than inside the run's worktree. A run without a
    /// worktree has no place that is its own.
    fn outside_worktree(&self, cwd: &Path, path: &str) -> bool {
        let Some(worktree) = self.worktree else {
            return true;
        };
        is_home(path) || !resolve(cwd, path).starts_with(normalise(worktree))
    }

    /// Whether a shell command that writes to `path`, relative to `cwd`, writes somewhere it may not: not the
    /// worktree, `/tmp` or the null device and standard streams.
    fn writes_elsewhere(&self, cwd: &Path, path: &str) -> bool {
        let resolved = resolve(cwd, path);
        let allowed = !is_home(path)
            && (resolved.starts_with("/tmp")
                || ["/dev/null", "/dev/stdout", "/dev/stderr"]
                    .iter()
                    .any(|stream| resolved == Path::new(stream)));
        !allowed && self.outside_worktree(cwd, path)
    }
}

fn is_home(path: &str) -> bool {
    path.starts_with('~') || path.starts_with("$HOME") || path.starts_with("${HOME}")
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

/// One simple command of a command line.
#[derive(Default)]
struct Simple {
    /// Its words, with quotes removed and leading `sudo`, `env`, `command` and `NAME=value` skipped. The first is
    /// the command.
    words: Vec<String>,
    /// The files it redirects output to (`>`, `>>`, `2>`, `&>`), not counting `2>&1` and the like.
    writes: Vec<String>,
}

/// The simple commands of `command`, which are cut at `;`, `|`, `&`, new lines, parentheses and backticks outside
/// quotes.
fn simple_commands(command: &str) -> Vec<Simple> {
    let mut commands = Vec::new();
    let mut current = Simple::default();
    let mut chars = command.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ';' | '|' | '\n' | '(' | ')' | '`' => {
                chars.next();
                commands.push(std::mem::take(&mut current));
            }
            '&' => {
                chars.next();
                // `&>` redirects; the `>` is read next.
                if chars.peek() != Some(&'>') {
                    commands.push(std::mem::take(&mut current));
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'>') {
                    chars.next();
                }
                let duplicates = chars.peek() == Some(&'&');
                if duplicates {
                    chars.next();
                }
                while chars.next_if(|c| c.is_whitespace()).is_some() {}
                let target = read_word(&mut chars);
                let descriptor = target.chars().all(|c| c.is_ascii_digit() || c == '-');
                if !(duplicates && descriptor) {
                    current.writes.push(target);
                }
            }
            c if c.is_whitespace() || c == '<' => {
                chars.next();
            }
            _ => {
                let word = read_word(&mut chars);
                // The `2` of `2>file` is a file descriptor, not a word.
                let descriptor =
                    chars.peek() == Some(&'>') && word.chars().all(|c| c.is_ascii_digit());
                if !descriptor {
                    current.words.push(word);
                }
            }
        }
    }
    commands.push(current);
    commands
        .into_iter()
        .map(|mut simple| {
            simple.words = without_prefixes(simple.words);
            simple
        })
        .filter(|simple| !simple.words.is_empty() || !simple.writes.is_empty())
        .collect()
}

/// `words` without the leading `sudo`, `env`, `command` and `NAME=value`.
fn without_prefixes(words: Vec<String>) -> Vec<String> {
    let skipped = words
        .iter()
        .take_while(|word| {
            matches!(word.as_str(), "sudo" | "env" | "command") || word.contains('=')
        })
        .count();
    words.into_iter().skip(skipped).collect()
}

/// The next word, with its quotes removed; quoted spaces and operators stay in the word.
fn read_word(chars: &mut Peekable<Chars>) -> String {
    let mut word = String::new();
    let mut quote = None;
    while let Some(&c) = chars.peek() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, ';' | '|' | '&' | '\n' | '(' | ')' | '`' | '<' | '>') => break,
            (None, c) if c.is_whitespace() => break,
            (quote, '\\') if quote != Some('\'') => {
                chars.next();
                word.extend(chars.peek());
            }
            _ => word.push(c),
        }
        chars.next();
    }
    word
}

/// How deep shells started by a command (`bash -c`, `eval`) are looked into. A command that nests deeper is
/// refused, whatever it does.
const MAX_NESTING: usize = 4;

/// Whether `test` holds for a simple command of `command` or of a shell it starts, given the directory the command
/// is in. `cd` moves the directory for the commands after it.
fn any_simple(call: &ToolCall, test: &dyn Fn(&Simple, &Path) -> bool) -> bool {
    script_any(call.command(), call.start_dir(), 0, test)
}

fn script_any(
    script: &str,
    mut cwd: PathBuf,
    depth: usize,
    test: &dyn Fn(&Simple, &Path) -> bool,
) -> bool {
    depth > MAX_NESTING
        || simple_commands(script)
            .iter()
            .any(|simple| command_any(simple, &mut cwd, depth, test))
}

fn command_any(
    simple: &Simple,
    cwd: &mut PathBuf,
    depth: usize,
    test: &dyn Fn(&Simple, &Path) -> bool,
) -> bool {
    if test(simple, cwd) {
        return true;
    }
    let Some((name, args)) = simple.words.split_first() else {
        return false;
    };
    match name.as_str() {
        "cd" => {
            let dir = args.iter().find(|arg| !arg.starts_with('-') || *arg == "-");
            *cwd = match dir {
                // `cd` alone, `cd ~` and `cd -` leave the worktree.
                Some(dir) if dir != "-" && !is_home(dir) => resolve(cwd, dir),
                _ => PathBuf::from("~"),
            };
            false
        }
        "bash" | "sh" | "zsh" | "dash" => args
            .iter()
            .position(|arg| arg.starts_with('-') && !arg.starts_with("--") && arg.contains('c'))
            .is_some_and(|at| script_any(&args[at + 1..].join(" "), cwd.clone(), depth + 1, test)),
        "eval" => script_any(&args.join(" "), cwd.clone(), depth + 1, test),
        "xargs" => {
            let words = args.iter().skip_while(|arg| arg.starts_with('-')).cloned();
            let command = Simple {
                words: without_prefixes(words.collect()),
                writes: Vec::new(),
            };
            command_any(&command, &mut cwd.clone(), depth, test)
        }
        _ => false,
    }
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
            // The subcommand is the first word that is not an option (or the value of -C or -c).
            let mut rest = args.iter();
            let mut subcommand = None;
            while let Some(word) = rest.next() {
                match word.as_str() {
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
            subcommand.is_some_and(|word| word == "push")
                && rest.any(|arg| {
                    arg.starts_with("--force")
                        || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('f'))
                        || (arg.starts_with('+') && arg.len() > 1)
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

/// What the teacher is told. It is a constant so that it can be proposed for change (spec 4, the Retro) without
/// touching the code around it.
const TEACHER_PROMPT: &str = "You judge one tool call of a coding agent against a list of rules. \
Decide whether the call breaks any rule. Judge only what the call itself does, with the card and the worktree as \
context, and refuse only when a rule clearly forbids the call. Answer with JSON only, no other text: \
{\"decision\": \"allow\" | \"refuse\", \"rule\": \"<name of the rule broken, or empty>\", \
\"reason\": \"<one sentence>\"}. A refusal must name one of the rules given.";

/// The teacher model (spec 3.2): judges the rules in prose by asking a model through the router's own OpenAI- or
/// Anthropic-compatible endpoint, without streaming. The request carries a token issued for the run being checked and
/// revoked after the call, so the tokens are counted in `usage` like any model call and provider keys stay in the
/// router.
pub struct Teacher {
    model: String,
    protocol: Protocol,
    timeout: Duration,
    router: Arc<Router>,
    /// Where the router listens, such as `http://127.0.0.1:7172`.
    router_url: String,
}

impl Teacher {
    pub fn new(config: &TeacherConfig, router: Arc<Router>, router_url: String) -> Self {
        Self {
            model: config.model.clone(),
            protocol: config.protocol,
            timeout: config.timeout,
            router,
            router_url,
        }
    }

    /// The teacher's decision about the call described by `context`, judged against `rules`. Anything that goes
    /// wrong is a refusal.
    fn judge(&self, context: &BTreeMap<String, String>, rules: &[Rule]) -> Decision {
        let answer = self
            .ask(context, rules)
            .and_then(|text| parse_answer(&text, rules));
        let (choice, reason) = match answer {
            Ok(Answer::Allow(reason)) => ("allow", reason),
            Ok(Answer::Refuse { rule, reason }) => ("refuse", format!("{rule}: {reason}")),
            Err(why) => ("refuse", format!("teacher: {why}")),
        };
        decision(choice, reason, "teacher")
    }

    /// The teacher model's reply text.
    fn ask(&self, context: &BTreeMap<String, String>, rules: &[Rule]) -> Result<String, String> {
        let number = |key: &str| context.get(key).and_then(|v| v.parse::<i64>().ok());
        let run_id =
            number("run_id").ok_or("the question names no run to count its tokens against")?;
        let token = self.router.issue_token(Grant {
            run_id,
            card_id: number("card_id"),
            budget: None,
        });
        let reply = self.request(&token, &user_message(context, rules));
        self.router.revoke_token(&token);
        reply
    }

    /// One non-streaming request to the router.
    ///
    /// The blocking client is meant for this: the host runs a plugin on a thread of its own, so waiting here blocks
    /// nothing else, and a plain call reads better than a handle into the server's runtime. The client is built per
    /// call because it must be created and dropped outside of an async context, which this thread is.
    fn request(&self, token: &str, user: &str) -> Result<String, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| e.to_string())?;
        let base = self.router_url.trim_end_matches('/');
        let request = match self.protocol {
            Protocol::OpenAi => client
                .post(format!("{base}/v1/chat/completions"))
                .bearer_auth(token)
                .json(&json!({
                    "model": self.model,
                    "stream": false,
                    "messages": [
                        { "role": "system", "content": TEACHER_PROMPT },
                        { "role": "user", "content": user },
                    ],
                })),
            Protocol::Anthropic => client
                .post(format!("{base}/v1/messages"))
                .header("x-api-key", token)
                .header("anthropic-version", "2023-06-01")
                .json(&json!({
                    "model": self.model,
                    "stream": false,
                    "max_tokens": 1024,
                    "system": TEACHER_PROMPT,
                    "messages": [{ "role": "user", "content": user }],
                })),
        };
        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                format!("no answer within {} s", self.timeout.as_secs())
            } else {
                format!("the request failed: {e}")
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("the router answered HTTP {status}"));
        }
        let body: Value = response
            .json()
            .map_err(|e| format!("the reply is not JSON: {e}"))?;
        let text = match self.protocol {
            Protocol::OpenAi => body.pointer("/choices/0/message/content"),
            Protocol::Anthropic => body.pointer("/content/0/text"),
        };
        text.and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "the reply has no text".to_string())
    }
}

/// What the teacher is asked about one call: the rules, then the call and what it happens in.
fn user_message(context: &BTreeMap<String, String>, rules: &[Rule]) -> String {
    let get = |key: &str| context.get(key).map_or("", String::as_str);
    format!(
        "Rules:\n\n{}\nTool: {}\nInput: {}\nWorktree: {}\nCard title: {}\nCard body:\n{}\n",
        rules::to_markdown(rules),
        get("tool"),
        get("input"),
        get("worktree"),
        get("card_title"),
        get("card_body"),
    )
}

/// The teacher's judgement of a call, once understood.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Allow(String),
    Refuse { rule: String, reason: String },
}

#[derive(Deserialize)]
struct RawAnswer {
    decision: String,
    #[serde(default)]
    rule: String,
    #[serde(default)]
    reason: String,
}

/// Reads the teacher's reply: JSON, bare or in a ```json fence. A refusal must name one of `rules`.
fn parse_answer(text: &str, rules: &[Rule]) -> Result<Answer, String> {
    let json = text.trim();
    let json = json
        .strip_prefix("```json")
        .or_else(|| json.strip_prefix("```"))
        .and_then(|fenced| fenced.strip_suffix("```"))
        .unwrap_or(json);
    let answer: RawAnswer = serde_json::from_str(json.trim())
        .map_err(|e| format!("the answer is not the JSON asked for ({e})"))?;
    match answer.decision.as_str() {
        "allow" => Ok(Answer::Allow(if answer.reason.is_empty() {
            "the teacher found no rule broken".into()
        } else {
            answer.reason
        })),
        "refuse" if rules.iter().any(|rule| rule.name == answer.rule) => Ok(Answer::Refuse {
            rule: answer.rule,
            reason: answer.reason,
        }),
        "refuse" => Err(format!(
            "the answer refuses for {:?}, which is not one of the rules",
            answer.rule
        )),
        other => Err(format!("{other:?} is neither allow nor refuse")),
    }
}

fn decision(choice: &str, reason: String, by: &str) -> Decision {
    Decision {
        choice: choice.into(),
        reason,
        by: Some(by.into()),
    }
}

/// The compiled-in decider for `tool_call` questions. Without a teacher, prose rules cannot be checked, so calls are
/// refused while any applies.
pub struct Guard {
    teacher: Option<Teacher>,
}

impl Guard {
    pub fn new(teacher: Option<Teacher>) -> Self {
        Self { teacher }
    }
}

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
        let decision = if let Some(error) = context.get("rules_error") {
            decision(
                "refuse",
                format!("the rules cannot be read, so no call is allowed: {error}"),
                "patterns",
            )
        } else if let Some(reason) = broken_rule(&tool_call) {
            decision("refuse", reason, "patterns")
        } else {
            let prose = rules::parse(context.get("rules").map_or("", String::as_str));
            match (&self.teacher, prose.is_empty()) {
                (_, true) => decision("allow", "no rule is broken".into(), "patterns"),
                (Some(teacher), false) => teacher.judge(context, &prose),
                (None, false) => decision(
                    "refuse",
                    "no teacher is configured to check the rules in prose".into(),
                    "teacher",
                ),
            }
        };
        serde_json::to_value(decision).map_err(|e| PluginError::Failed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
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

    fn decide(tool: &str, input: Value) -> moebius_plugin_api::Decision {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Guard::new(None));
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
    fn unreadable_rules_refuse_every_call() {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Guard::new(None));
        let request = DecideRequest {
            kind: "tool_call".into(),
            context: BTreeMap::from([
                ("tool".to_string(), "read".to_string()),
                ("input".to_string(), "{}".to_string()),
                (
                    "rules_error".to_string(),
                    "cannot read rules.md".to_string(),
                ),
            ]),
            options: vec!["allow".into(), "refuse".into()],
        };
        let refused = host.decide(NAME, &request).unwrap();
        assert_eq!(refused.choice, "refuse");
        assert!(refused.reason.contains("rules cannot be read"));
        assert!(refused.reason.contains("cannot read rules.md"));
    }

    #[test]
    fn other_questions_are_not_the_guards() {
        let mut host = Host::new(Duration::from_secs(30));
        host.add(Guard::new(None));
        let request = DecideRequest {
            kind: "merge".into(),
            context: BTreeMap::new(),
            options: vec!["allow".into(), "refuse".into()],
        };
        assert!(host.decide(NAME, &request).is_err());
    }

    mod teacher {
        use std::sync::Mutex;

        use axum::extract::State;
        use axum::http::{HeaderMap, StatusCode};
        use axum::response::IntoResponse;
        use axum::routing::post;

        use super::*;
        use crate::decide::{Decisions, Question};
        use crate::router::Upstream;
        use crate::store::Store;

        const RULES: &str = "## no-billing\nNever change files under billing/.\n";

        /// What the fake upstream answers.
        #[derive(Clone)]
        enum Reply {
            /// A chat completion (or message) whose text is this.
            Text(String),
            Status(u16),
            /// Answers only after this long.
            Slow(Duration),
        }

        struct Fake {
            reply: Mutex<Reply>,
            /// The bodies of the requests that arrived, with their credentials.
            requests: Mutex<Vec<(String, Value)>>,
        }

        impl Fake {
            fn answers(&self, reply: Reply) {
                *self.reply.lock().unwrap() = reply;
            }

            fn answer_text(&self, decision: &str, rule: &str) {
                self.answers(Reply::Text(
                    json!({ "decision": decision, "rule": rule, "reason": "it is under billing/" })
                        .to_string(),
                ));
            }

            fn request_count(&self) -> usize {
                self.requests.lock().unwrap().len()
            }
        }

        async fn serve(
            fake: Arc<Fake>,
            headers: HeaderMap,
            body: Value,
        ) -> axum::response::Response {
            let credential = ["authorization", "x-api-key"]
                .iter()
                .find_map(|name| headers.get(*name))
                .map(|v| v.to_str().unwrap().to_string())
                .unwrap_or_default();
            let anthropic = body.get("max_tokens").is_some();
            fake.requests.lock().unwrap().push((credential, body));
            let reply = fake.reply.lock().unwrap().clone();
            let text = match reply {
                Reply::Status(code) => return StatusCode::from_u16(code).unwrap().into_response(),
                Reply::Slow(delay) => {
                    tokio::time::sleep(delay).await;
                    String::new()
                }
                Reply::Text(text) => text,
            };
            if anthropic {
                axum::Json(json!({
                    "model": "teacher-model",
                    "content": [{ "type": "text", "text": text }],
                    "usage": { "input_tokens": 40, "output_tokens": 8 },
                }))
                .into_response()
            } else {
                axum::Json(json!({
                    "model": "teacher-model",
                    "choices": [{ "message": { "role": "assistant", "content": text } }],
                    "usage": { "prompt_tokens": 40, "completion_tokens": 8 },
                }))
                .into_response()
            }
        }

        /// A guard with a teacher behind a real router, whose upstream is a fake.
        struct Teaching {
            _dir: tempfile::TempDir,
            store: Arc<Store>,
            decisions: Arc<Decisions>,
            fake: Arc<Fake>,
            router: Arc<Router>,
            router_url: String,
            run: i64,
            card: i64,
        }

        async fn teaching(protocol: Protocol, teacher: Option<Duration>) -> Teaching {
            let dir = tempfile::tempdir().unwrap();
            let store = Arc::new(Store::in_memory(dir.path()).unwrap());
            let repo = store
                .add_repository("demo", "/srv/demo", "none", "main")
                .unwrap();
            let card = store
                .add_card(
                    repo.id,
                    "Fix invoices",
                    "Round the totals",
                    "person",
                    "In progress",
                )
                .unwrap();
            let run = store
                .add_run(Some(card.id), "Coder", "pi-bolt", "code")
                .unwrap();
            let fake = Arc::new(Fake {
                reply: Mutex::new(Reply::Text(String::new())),
                requests: Mutex::new(Vec::new()),
            });
            let upstream = axum::Router::new()
                .route(
                    "/v1/chat/completions",
                    post(
                        |State(fake): State<Arc<Fake>>,
                         headers: HeaderMap,
                         axum::Json(body): axum::Json<Value>| {
                            serve(fake, headers, body)
                        },
                    ),
                )
                .route(
                    "/v1/messages",
                    post(
                        |State(fake): State<Arc<Fake>>,
                         headers: HeaderMap,
                         axum::Json(body): axum::Json<Value>| {
                            serve(fake, headers, body)
                        },
                    ),
                )
                .with_state(fake.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let upstream_url = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move { axum::serve(listener, upstream).await });

            let router = Arc::new(Router::new(
                store.clone(),
                vec![Upstream {
                    protocol,
                    base_url: upstream_url,
                    api_key: None,
                }],
            ));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let router_url = format!("http://{}", listener.local_addr().unwrap());
            let routes = router.clone().routes();
            tokio::spawn(async move { axum::serve(listener, routes).await });

            let teacher = teacher.map(|timeout| {
                let config = TeacherConfig {
                    model: "teacher-model".into(),
                    protocol,
                    timeout,
                };
                Teacher::new(&config, router.clone(), router_url.clone())
            });
            let mut host = Host::new(Duration::from_secs(30));
            host.add(Guard::new(teacher));
            let mut decisions = Decisions::new(store.clone(), Arc::new(host));
            decisions.route("tool_call", NAME);
            Teaching {
                _dir: dir,
                store,
                decisions: Arc::new(decisions),
                fake,
                router,
                router_url,
                run: run.id,
                card: card.id,
            }
        }

        impl Teaching {
            /// Decides a call of the run through `Decisions`, as the check endpoint does.
            async fn decide(
                &self,
                tool: &str,
                input: Value,
                rules: &str,
            ) -> crate::decide::Decision {
                let question = Question {
                    kind: "tool_call".into(),
                    context: BTreeMap::from([
                        ("tool".to_string(), tool.to_string()),
                        ("input".to_string(), input.to_string()),
                        ("worktree".to_string(), WORKTREE.to_string()),
                        ("card_title".to_string(), "Fix invoices".to_string()),
                        ("card_body".to_string(), "Round the totals".to_string()),
                        ("run_id".to_string(), self.run.to_string()),
                        ("card_id".to_string(), self.card.to_string()),
                        ("rules".to_string(), rules.to_string()),
                    ]),
                    options: vec!["allow".into(), "refuse".into()],
                    fail_closed: "refuse".into(),
                    run_id: Some(self.run),
                    card_id: Some(self.card),
                };
                let decisions = self.decisions.clone();
                tokio::task::spawn_blocking(move || decisions.decide(&question).unwrap())
                    .await
                    .unwrap()
            }

            async fn write_billing(&self) -> crate::decide::Decision {
                self.decide("write", json!({ "path": "billing/invoice.rs" }), RULES)
                    .await
            }
        }

        const SECONDS: Duration = Duration::from_secs(20);

        #[tokio::test(flavor = "multi_thread")]
        async fn a_teacher_refuses_a_call_that_breaks_a_prose_rule_and_is_named_and_counted() {
            let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
            t.fake.answer_text("refuse", "no-billing");
            let decision = t.write_billing().await;
            assert_eq!(decision.choice, "refuse");
            assert!(!decision.refused);
            assert!(
                decision.reason.starts_with("no-billing: "),
                "{}",
                decision.reason
            );
            assert_eq!(decision.decider, "guard/teacher");
            assert_eq!(
                t.store.judgements("tool_call").unwrap()[0].decider,
                "guard/teacher"
            );

            let usage = t.store.usage_for_run(t.run).unwrap();
            assert_eq!(usage.len(), 1);
            assert_eq!((usage[0].input_tokens, usage[0].output_tokens), (40, 8));
            assert_eq!(usage[0].model, "teacher-model");

            // The teacher saw the rules and the call, and the token issued for it is gone.
            let (_, body) = t.fake.requests.lock().unwrap()[0].clone();
            assert_eq!(body["model"], "teacher-model");
            assert_eq!(body["stream"], false);
            assert_eq!(body["messages"][0]["content"], TEACHER_PROMPT);
            let user = body["messages"][1]["content"].as_str().unwrap();
            for expected in [
                "## no-billing",
                "Never change files under billing/.",
                "Tool: write",
                "billing/invoice.rs",
                WORKTREE,
                "Fix invoices",
                "Round the totals",
            ] {
                assert!(user.contains(expected), "{expected} in {user}");
            }
            assert_eq!(t.router.token_count(), 0);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_teacher_that_allows_allows_even_in_a_json_fence() {
            let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
            t.fake.answer_text("allow", "");
            let decision = t.write_billing().await;
            assert_eq!(decision.choice, "allow");
            assert_eq!(decision.decider, "guard/teacher");

            t.fake.answers(Reply::Text(
                "```json\n{\"decision\": \"refuse\", \"rule\": \"no-billing\", \"reason\": \"no\"}\n```".into(),
            ));
            let decision = t.write_billing().await;
            assert_eq!(decision.choice, "refuse");
            assert_eq!(decision.reason, "no-billing: no");
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn an_anthropic_teacher_is_asked_through_the_messages_endpoint() {
            let t = teaching(Protocol::Anthropic, Some(SECONDS)).await;
            t.fake.answer_text("refuse", "no-billing");
            let decision = t.write_billing().await;
            assert_eq!(decision.choice, "refuse");
            assert_eq!(decision.decider, "guard/teacher");
            let (_, body) = t.fake.requests.lock().unwrap()[0].clone();
            assert_eq!(body["system"], TEACHER_PROMPT);
            assert_eq!(t.store.usage_for_run(t.run).unwrap().len(), 1);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_teacher_that_fails_or_makes_no_sense_refuses() {
            let t = teaching(Protocol::OpenAi, Some(Duration::from_secs(1))).await;
            let cases = [
                (Reply::Text("I think it is fine.".into()), "not the JSON"),
                (
                    Reply::Text(r#"{"decision": "maybe", "rule": "", "reason": ""}"#.into()),
                    "neither allow nor refuse",
                ),
                (
                    Reply::Text(
                        r#"{"decision": "refuse", "rule": "no-such-rule", "reason": "x"}"#.into(),
                    ),
                    "not one of the rules",
                ),
                (
                    Reply::Text(r#"{"decision": "refuse", "rule": "", "reason": "x"}"#.into()),
                    "not one of the rules",
                ),
                (Reply::Status(500), "HTTP 500"),
                (Reply::Slow(Duration::from_secs(4)), "no answer within 1 s"),
            ];
            for (reply, expected) in cases {
                t.fake.answers(reply);
                let decision = t.write_billing().await;
                assert_eq!(decision.choice, "refuse", "{expected}");
                assert_eq!(decision.decider, "guard/teacher", "{expected}");
                assert!(
                    decision.reason.starts_with("teacher: ") && decision.reason.contains(expected),
                    "{expected}: {}",
                    decision.reason
                );
            }
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn without_prose_rules_nobody_is_asked() {
            let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
            for rules in ["", "just words, no sections\n"] {
                let decision = t
                    .decide("write", json!({ "path": "billing/invoice.rs" }), rules)
                    .await;
                assert_eq!(decision.choice, "allow");
            }
            assert_eq!(t.fake.request_count(), 0);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn prose_rules_without_a_teacher_refuse() {
            let t = teaching(Protocol::OpenAi, None).await;
            let decision = t.write_billing().await;
            assert_eq!(decision.choice, "refuse");
            assert!(!decision.refused);
            assert!(decision.reason.contains("no teacher is configured"));
            assert_eq!(decision.decider, "guard/teacher");
            assert_eq!(t.fake.request_count(), 0);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_built_in_refusal_is_the_patterns_and_never_reaches_the_teacher() {
            let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
            t.fake.answer_text("allow", "");
            let decision = t
                .decide("bash", json!({ "command": "git push --force" }), RULES)
                .await;
            assert_eq!(decision.choice, "refuse");
            assert!(decision.reason.starts_with("no-force-push: "));
            assert_eq!(decision.decider, "guard/patterns");
            assert_eq!(t.fake.request_count(), 0);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_question_without_a_run_cannot_be_put_to_the_teacher() {
            let t = teaching(Protocol::OpenAi, Some(SECONDS)).await;
            let rules = rules::parse(RULES);
            let teacher = Teacher::new(
                &TeacherConfig {
                    model: "m".into(),
                    protocol: Protocol::OpenAi,
                    timeout: SECONDS,
                },
                Arc::new(Router::new(t.store.clone(), vec![])),
                t.router_url.clone(),
            );
            let decision =
                tokio::task::spawn_blocking(move || teacher.judge(&BTreeMap::new(), &rules))
                    .await
                    .unwrap();
            assert_eq!(decision.choice, "refuse");
            assert!(decision.reason.contains("no run"));
        }
    }
}
