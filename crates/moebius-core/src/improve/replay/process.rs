//! Running a call in the copy: `bash`, `grep` and `find` as processes with an empty environment and a time limit
//! after which their process group is killed, and the file tools applied by Moebius itself.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::call::Call;
use crate::decide::guard;
use crate::platform;

/// Runs `program` in `dir` with an empty environment (`PATH` only, `HOME` is `dir`), no input and its output in `log`,
/// and kills its process group when it passes `limit`. Returns the exit code and whether it was killed for the time.
fn run_limited(
    program: &str,
    args: &[&str],
    dir: &Path,
    log: &Path,
    limit: Duration,
) -> std::io::Result<(Option<i32>, bool)> {
    let file = std::fs::File::create(log)?;
    run_apart(program, args, dir, (file.try_clone()?, file), limit)
}

/// Like `run_limited`, with what the program prints on its standard output and on its standard error in two files.
pub(crate) fn run_apart(
    program: &str,
    args: &[&str],
    dir: &Path,
    (stdout, stderr): (std::fs::File, std::fs::File),
    limit: Duration,
) -> std::io::Result<(Option<i32>, bool)> {
    let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let mut child = platform::own_group(
        Command::new(program)
            .args(args)
            .current_dir(dir)
            .env_clear()
            .env("PATH", path)
            .env("HOME", dir)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr),
    )
    .spawn()
    .map_err(|e| platform::unix_program_error(program, e))?;
    let id = child.id();
    let kill = || platform::kill_tree(id);
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait()? {
            // Whatever the command left running goes with it.
            kill();
            return Ok((status.code(), false));
        }
        if Instant::now() >= deadline {
            kill();
            child.wait()?;
            return Ok((None, true));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Applies a file tool to the copy, the way the tool would: the exit code and what it printed.
fn apply_file_tool(copy: &Path, tool: &str, input: &Value, relative: &Path) -> (i32, String) {
    let target = copy.join(relative);
    let text = |key: &str| input[key].as_str();
    let done = |result: std::io::Result<String>| match result {
        Ok(output) => (0, output),
        Err(e) => (1, e.to_string()),
    };
    let invalid = |why: &str| (1, why.to_string());
    match tool {
        "read" => done(std::fs::read_to_string(&target)),
        "write" => {
            let Some(content) = text("content") else {
                return invalid("the call has no content to write");
            };
            done((|| {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, content)?;
                Ok(format!("wrote {} bytes", content.len()))
            })())
        }
        "edit" => {
            fn pair(edit: &Value) -> Option<(&str, &str)> {
                Some((edit["oldText"].as_str()?, edit["newText"].as_str()?))
            }
            let mut edits: Vec<_> = input["edits"]
                .as_array()
                .map(|edits| edits.iter().filter_map(pair).collect())
                .unwrap_or_default();
            edits.extend(pair(input));
            done((|| {
                let mut content = std::fs::read_to_string(&target)?;
                for (old, new) in &edits {
                    if content.matches(old).count() != 1 {
                        return Err(std::io::Error::other(format!(
                            "{old:?} is not in the file exactly once"
                        )));
                    }
                    content = content.replacen(old, new, 1);
                }
                std::fs::write(&target, content)?;
                Ok(format!("made {} edits", edits.len()))
            })())
        }
        _ => unreachable!("only the file tools are applied"),
    }
}

/// Runs the call in the copy and returns how it ended; its output is in `log`.
pub(super) fn run_in_copy(
    call: &Call,
    copy: &Path,
    log: &Path,
    limit: Duration,
) -> std::io::Result<(Option<i32>, bool)> {
    let input = &call.input;
    let relative = |key: &str, default: &str| {
        guard::relative_to(&call.worktree, input[key].as_str().unwrap_or(default))
            .unwrap_or_default()
    };
    match call.tool.as_str() {
        "bash" => {
            let command = input["command"].as_str().unwrap_or_default();
            run_limited("bash", &["-c", command], copy, log, limit)
        }
        "grep" | "find" => {
            let path = relative("path", ".");
            let path = if path.as_os_str().is_empty() {
                Path::new(".")
            } else {
                &path
            };
            let pattern = input["pattern"].as_str();
            let path = path.to_string_lossy().into_owned();
            let (program, args) = match (call.tool.as_str(), pattern) {
                ("grep", Some(pattern)) => {
                    ("grep", vec!["-rn", "-e", pattern, "--", path.as_str()])
                }
                ("find", Some(pattern)) => ("find", vec![path.as_str(), "-name", pattern]),
                ("find", None) => ("find", vec![path.as_str()]),
                _ => {
                    std::fs::write(log, "the call has no pattern")?;
                    return Ok((Some(1), false));
                }
            };
            run_limited(program, &args, copy, log, limit)
        }
        "ls" => {
            let dir = copy.join(relative("path", "."));
            let (exit, output) = match std::fs::read_dir(&dir) {
                Ok(entries) => {
                    let mut names: Vec<_> = entries
                        .flatten()
                        .map(|e| {
                            let slash = if e.path().is_dir() { "/" } else { "" };
                            format!("{}{slash}", e.file_name().to_string_lossy())
                        })
                        .collect();
                    names.sort();
                    (0, names.join("\n"))
                }
                Err(e) => (1, e.to_string()),
            };
            std::fs::write(log, output)?;
            Ok((Some(exit), false))
        }
        tool => {
            let (exit, output) = apply_file_tool(copy, tool, input, &relative("path", ""));
            std::fs::write(log, output)?;
            Ok((Some(exit), false))
        }
    }
}

#[cfg(test)]
// Some tests are Unix only; on Windows what only they use is unused.
#[cfg_attr(not(unix), allow(unused))]
mod tests {
    use super::*;

    // Unix only: the replayed call runs with `bash`.
    #[cfg(unix)]
    #[test]
    fn a_command_runs_with_an_empty_environment_and_is_killed_at_its_time_limit() {
        let dir = tempfile::tempdir().unwrap();
        let (copy, log) = (dir.path().join("copy"), dir.path().join("log"));
        std::fs::create_dir_all(&copy).unwrap();
        let run = |command: &str, seconds| {
            run_limited(
                "bash",
                &["-c", command],
                &copy,
                &log,
                Duration::from_secs(seconds),
            )
            .unwrap()
        };
        let variables = "env | cut -d= -f1 | grep -vx -e PWD -e SHLVL -e _ | sort | tr '\\n' ' '; echo $HOME; exit 3";
        assert_eq!(run(variables, 30), (Some(3), false));
        let printed = std::fs::read_to_string(&log).unwrap();
        assert_eq!(printed.trim(), format!("HOME PATH {}", copy.display()));
        assert_eq!(run("sleep 30", 1), (None, true));
    }
}
