//! How the guard reads a tool call, for the built-in rules (`patterns`) and for replays (`reversible`): the call
//! itself, the simple commands of a command line with what they redirect to, the shells a command starts (`bash -c`,
//! `eval`, `xargs`) and the `cd`s that move where relative paths are, and paths resolved without touching the disk. It
//! reads the command line; it runs nothing.

use std::iter::Peekable;
use std::path::{Component, Path, PathBuf};
use std::str::Chars;

use serde_json::Value;

/// A tool call as the patterns see it.
pub(super) struct ToolCall<'a> {
    pub(super) tool: &'a str,
    pub(super) input: Value,
    pub(super) worktree: Option<&'a Path>,
}

impl ToolCall<'_> {
    pub(super) fn command(&self) -> &str {
        self.input["command"].as_str().unwrap_or_default()
    }

    /// Where a command starts: the worktree, or `/` for a run without one.
    pub(super) fn start_dir(&self) -> PathBuf {
        self.worktree.unwrap_or(Path::new("/")).to_path_buf()
    }

    /// Whether `path`, relative to `cwd`, is somewhere other than inside the run's worktree. A run without a
    /// worktree has no place that is its own.
    pub(super) fn outside_worktree(&self, cwd: &Path, path: &str) -> bool {
        let Some(worktree) = self.worktree else {
            return true;
        };
        is_home(path) || !resolve(cwd, path).starts_with(normalise(worktree))
    }

    /// Whether a shell command that writes to `path`, relative to `cwd`, writes somewhere it may not: not the
    /// worktree, `/tmp` or the null device and standard streams.
    pub(super) fn writes_elsewhere(&self, cwd: &Path, path: &str) -> bool {
        let resolved = resolve(cwd, path);
        let allowed = !is_home(path)
            && (resolved.starts_with("/tmp")
                || ["/dev/null", "/dev/stdout", "/dev/stderr"]
                    .iter()
                    .any(|stream| resolved == Path::new(stream)));
        !allowed && self.outside_worktree(cwd, path)
    }
}

/// The call in the guard's vocabulary: Claude Code's tool names are mapped to `bash`, `write` and `edit`, and its
/// `file_path` and `notebook_path` to `path`; for Claude Code's writing tools those win over a `path` the call
/// also carries, because they are what the tool writes to. Other calls are as they are.
pub(super) fn normalise_call(tool: &str, mut input: Value) -> (String, Value) {
    let writes_file_path = matches!(tool, "Write" | "Edit" | "MultiEdit" | "NotebookEdit");
    let tool = match tool {
        "Bash" => "bash",
        "Write" => "write",
        "Edit" | "MultiEdit" | "NotebookEdit" => "edit",
        other => other,
    };
    if let Some(object) = input.as_object_mut()
        && (writes_file_path || !object.contains_key("path"))
        && let Some(path) = ["file_path", "notebook_path"]
            .iter()
            .find_map(|key| object.get(*key).cloned())
    {
        object.insert("path".into(), path);
    }
    (tool.to_string(), input)
}

/// Whether `path` starts at the home directory, which is never inside a worktree.
pub(super) fn is_home(path: &str) -> bool {
    path.starts_with('~') || path.starts_with("$HOME") || path.starts_with("${HOME}")
}

/// One simple command of a command line.
#[derive(Default)]
pub(super) struct Simple {
    /// Its words, with quotes removed and leading wrappers (`sudo`, `env`, `command`, `time`, `exec`, `nohup`, `timeout`, `nice`) with their options, and `NAME=value` skipped. The first is
    /// the command.
    pub(super) words: Vec<String>,
    /// The files it redirects output to (`>`, `>>`, `2>`, `&>`), not counting `2>&1` and the like.
    pub(super) writes: Vec<String>,
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
                // `>>`, and `>|`, which writes over a file `noclobber` protects.
                if matches!(chars.peek(), Some('>' | '|')) {
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
            '\\' if line_continuation(&chars) => {
                chars.next();
                chars.next();
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

/// Shell reserved words and wrappers that run the command after them, which take no option with a value.
const WRAPPERS: [&str; 15] = [
    "{", "!", "if", "then", "do", "else", "elif", "while", "until", "time", "exec", "nohup",
    "command", "builtin", "nice",
];

/// `words` without what comes before the command: reserved words (`if`, `then`, `{`, `!` ...), wrappers (`sudo`,
/// `env`, `time`, `exec`, `nohup`, `timeout`, `nice`, `command`) with their options, and `NAME=value`. The program
/// that is left is named by its basename, so `/bin/rm` is `rm`.
fn without_prefixes(words: Vec<String>) -> Vec<String> {
    let mut at = 0;
    while let Some(word) = words.get(at) {
        let name = word.rsplit('/').next().unwrap_or(word);
        // The options a wrapper takes a value for (`sudo -u root`, `env -u NAME`, `timeout -s KILL`, `nice -n 5`).
        let valued: &[&str] = match name {
            "sudo" => &[
                "-u",
                "-g",
                "-h",
                "-p",
                "-C",
                "-D",
                "-R",
                "-T",
                "-U",
                "-r",
                "-t",
                "--user",
                "--group",
                "--host",
                "--prompt",
                "--close-from",
                "--chdir",
                "--role",
                "--type",
                "--other-user",
                "--command-timeout",
            ],
            "env" => &["-u", "-C", "-S", "--unset", "--chdir", "--split-string"],
            "timeout" => &["-s", "-k", "--signal", "--kill-after"],
            "nice" => &["-n", "--adjustment"],
            "exec" => &["-a"],
            _ => &[],
        };
        let wrapper = matches!(name, "sudo" | "env" | "timeout") || WRAPPERS.contains(&name);
        if wrapper && !word.contains('=') {
            at += 1;
            while let Some(option) = words.get(at).filter(|w| w.starts_with('-') && w.len() > 1) {
                at += 1 + usize::from(valued.contains(&option.as_str()));
            }
            // `timeout` is followed by its duration.
            if name == "timeout" {
                at += 1;
            }
        } else if word.contains('=') && !word.starts_with('-') {
            at += 1;
        } else {
            break;
        }
    }
    let mut rest: Vec<String> = words.into_iter().skip(at).collect();
    if let Some(program) = rest.first_mut()
        && program.contains('/')
        && let Some(name) = program.rsplit('/').next().filter(|n| !n.is_empty())
    {
        *program = name.to_string();
    }
    rest
}

/// Whether the `$` the reader is at opens a `$'...'` string.
fn ansi_quote(chars: &Peekable<Chars>) -> bool {
    let mut ahead = chars.clone();
    ahead.next();
    ahead.peek() == Some(&'\'')
}

/// Whether the reader is at a backslash before a new line, which continues the line.
fn line_continuation(chars: &Peekable<Chars>) -> bool {
    let mut ahead = chars.clone();
    ahead.next();
    ahead.peek() == Some(&'\n')
}

/// The next word, with its quotes removed; quoted spaces and operators stay in the word.
fn read_word(chars: &mut Peekable<Chars>) -> String {
    let mut word = String::new();
    let mut quote = None;
    let mut escapes = false;
    while let Some(&c) = chars.peek() {
        match (quote, c) {
            (Some(open), c) if c == open => {
                quote = None;
                escapes = false;
            }
            // `$'...'` quotes like single quotes, and `\'` inside it is an escaped quote.
            (None, '$') if ansi_quote(chars) => {
                chars.next();
                quote = Some('\'');
                escapes = true;
            }
            (Some('\''), '\\') if escapes => {
                chars.next();
                word.extend(chars.peek());
            }
            (None, '"' | '\'') => quote = Some(c),
            (None, ';' | '|' | '&' | '\n' | '(' | ')' | '`' | '<' | '>') => break,
            (None, c) if c.is_whitespace() => break,
            (quote, '\\') if quote != Some('\'') => {
                chars.next();
                // A backslash before a new line continues the line.
                if chars.peek() != Some(&'\n') {
                    word.extend(chars.peek());
                }
            }
            _ => word.push(c),
        }
        chars.next();
    }
    word
}

/// How deep shells started by a command (`bash -c`, `eval`) are looked into. A command that nests deeper is
/// refused, whatever it does.
pub(super) const MAX_NESTING: usize = 4;

/// Whether `test` holds for a simple command of `command` or of a shell it starts, given the directory the command
/// is in. `cd` moves the directory for the commands after it.
pub(super) fn any_simple(call: &ToolCall, test: &dyn Fn(&Simple, &Path) -> bool) -> bool {
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

/// The git subcommand of a command's arguments, the first word that is not an option (or the value of -C or -c), and
/// the words after it.
pub(super) fn git_subcommand(args: &[String]) -> Option<(&str, &[String])> {
    let mut rest = args.iter();
    while let Some(word) = rest.next() {
        match word.as_str() {
            "-C" | "-c" => {
                rest.next();
            }
            _ if word.starts_with('-') => {}
            _ => return Some((word, rest.as_slice())),
        }
    }
    None
}

/// `path` against `base` when it is relative, with `.` and `..` resolved without touching the disk.
pub(super) fn resolve(base: &Path, path: &str) -> PathBuf {
    normalise(&base.join(path))
}

/// `path` with `.` and `..` resolved without touching the disk.
pub(super) fn normalise(path: &Path) -> PathBuf {
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
