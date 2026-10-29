//! Keeping a tool's output small where it is made (SWE-agent's agent-computer interface): the result endpoint
//! (`decide::check`) passes each text the bridge reports through `shrink` before the model reads it. With the agent's
//! `tool_output.compress`, escape codes, progress bars and repeated lines go and a well-known command's output is
//! compressed (`compress`); above the agent's budget (`max_lines`, `max_bytes`) only its head and tail are kept. When
//! lines are left out, the whole output is saved to `.moebius/out/<n>.txt` in the run's worktree, which the agent can
//! read in ranges and which is never committed, and the text says so. Hiding the outputs of older turns is the bridge's
//! part (`bridge/moebius-bridge.ts`).

mod compress;

use std::io::Write;
use std::path::Path;

use crate::config::ToolOutput;

/// Where the whole outputs are saved, relative to the worktree.
pub const OUT_DIR: &str = ".moebius/out";

/// A text made smaller, and what was done to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shrunk {
    pub text: String,
    /// What was done, in order: `clean` (escape codes and progress bars), a compressor's name (such as `cargo-test`),
    /// `fold` (repeated lines) and `budget` (head and tail kept).
    pub steps: Vec<String>,
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub lines_before: usize,
    pub lines_after: usize,
    /// Where the whole output was saved, relative to the worktree, if it was.
    pub saved: Option<String>,
}

/// `text`, the output of a call (of `bash` when `command` is its command line), made smaller as `limits` say; `None`
/// when nothing changed. The whole text is saved in `worktree` when lines are left out.
pub fn shrink(
    text: &str,
    command: Option<&str>,
    limits: &ToolOutput,
    worktree: Option<&Path>,
) -> Option<Shrunk> {
    let mut steps = vec![];
    let mut notes = vec![];
    // The last line's end is not a line of its own.
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut lines: Vec<String> = if limits.compress {
        let cleaned = compress::clean(body);
        if cleaned != body {
            steps.push("clean".to_string());
        }
        cleaned.split('\n').map(String::from).collect()
    } else {
        body.split('\n').map(String::from).collect()
    };
    let mut left_out = false;
    if limits.compress
        && let Some(compressor) = command.and_then(compress::for_command)
    {
        let compressed = compressor(lines);
        lines = compressed.lines;
        if compressed.left_out > 0 {
            steps.push(compressed.name.to_string());
            notes.push(format!(
                "{} lines of {} left out",
                thousands(compressed.left_out),
                compressed.what
            ));
            notes.extend(compressed.summary);
            left_out = true;
        }
    }
    if limits.compress && compress::fold_repeats(&mut lines) {
        steps.push("fold".to_string());
    }
    let cut = budget(&lines, limits);
    if cut.is_some() {
        steps.push("budget".to_string());
        left_out = true;
    }
    if steps.is_empty() {
        return None;
    }
    let saved = left_out
        .then(|| worktree.and_then(|w| save(w, text)))
        .flatten();
    let whole = match &saved {
        Some(path) => format!("; the full output is in {path}"),
        None => String::new(),
    };
    if let Some(cut) = cut {
        let note = format!("[{} {} left out{whole}]", thousands(cut.left_out), cut.unit);
        lines = [cut.head, vec![note], cut.tail].concat();
    }
    let mut shrunk = lines.join("\n");
    if !notes.is_empty() {
        let note = format!("[{}{whole}]", notes.join("; "));
        shrunk = if shrunk.trim().is_empty() {
            note
        } else {
            format!("{shrunk}\n{note}")
        };
    }
    if body.len() < text.len() {
        shrunk.push('\n');
    }
    Some(Shrunk {
        bytes_before: text.len(),
        bytes_after: shrunk.len(),
        lines_before: text.lines().count(),
        lines_after: shrunk.lines().count(),
        text: shrunk,
        steps,
        saved,
    })
}

/// The head and tail of an output over its budget, and how much was left out between them.
struct Cut {
    head: Vec<String>,
    tail: Vec<String>,
    left_out: usize,
    /// `lines`, or `bytes` for an output whose lines are too long to keep any whole.
    unit: &'static str,
}

/// The head and tail of `lines` that fit `limits`, half of the budget each; `None` when all of them fit.
fn budget(lines: &[String], limits: &ToolOutput) -> Option<Cut> {
    let max_lines = match limits.max_lines {
        0 => usize::MAX,
        n => n as usize,
    };
    let max_bytes = match limits.max_bytes {
        0 => usize::MAX,
        n => n as usize,
    };
    let bytes: usize = lines.iter().map(|line| line.len() + 1).sum();
    if lines.len() <= max_lines && bytes <= max_bytes {
        return None;
    }
    let (half_lines, half_bytes) = (max_lines.div_ceil(2), max_bytes / 2);
    let fits = |taken: &[String], line: &String| {
        taken.len() < half_lines
            && taken.iter().map(|l| l.len() + 1).sum::<usize>() + line.len() < half_bytes
    };
    let mut head: Vec<String> = vec![];
    for line in lines {
        if !fits(&head, line) {
            break;
        }
        head.push(line.clone());
    }
    let mut tail: Vec<String> = vec![];
    for line in lines[head.len()..].iter().rev() {
        if !fits(&tail, line) {
            break;
        }
        tail.push(line.clone());
    }
    tail.reverse();
    if head.is_empty() && tail.is_empty() {
        // Not one line fits: the text's first and last bytes.
        let text = lines.join("\n");
        let start = &text[..floor_boundary(&text, half_bytes / 2)];
        let end = &text[floor_boundary(&text, text.len().saturating_sub(half_bytes / 2))..];
        return Some(Cut {
            head: vec![start.to_string()],
            tail: vec![end.to_string()],
            left_out: text.len() - start.len() - end.len(),
            unit: "bytes",
        });
    }
    Some(Cut {
        left_out: lines.len() - head.len() - tail.len(),
        head,
        tail,
        unit: "lines",
    })
}

/// The largest index at most `at` that is on a character boundary of `text`.
fn floor_boundary(text: &str, at: usize) -> usize {
    (0..=at.min(text.len()))
        .rev()
        .find(|&index| text.is_char_boundary(index))
        .unwrap_or(0)
}

/// `n` with a comma between each group of three digits, as in `1,840`.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Saves `text` as the next free `.moebius/out/<n>.txt` of `worktree` and returns that path, relative to the
/// worktree; `None` when it cannot be written. The directory ignores itself in git, so nothing in it is committed.
fn save(worktree: &Path, text: &str) -> Option<String> {
    let dir = worktree.join(OUT_DIR);
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join(".gitignore"), "*\n").ok()?;
    (1..10_000).find_map(|n| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(format!("{n}.txt")))
            .ok()?;
        file.write_all(text.as_bytes()).ok()?;
        Some(format!("{OUT_DIR}/{n}.txt"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(max_lines: u32, max_bytes: u32, compress: bool) -> ToolOutput {
        ToolOutput {
            hide_after_turns: 10,
            max_lines,
            max_bytes,
            compress,
        }
    }

    fn numbered(n: usize) -> String {
        (1..=n)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_small_plain_output_is_left_alone() {
        assert_eq!(
            shrink("a\nb\n", Some("ls"), &ToolOutput::default(), None),
            None
        );
    }

    #[test]
    fn an_output_over_the_budget_keeps_its_head_and_tail_and_is_saved_whole() {
        let dir = tempfile::tempdir().unwrap();
        let text = numbered(2040);
        let shrunk = shrink(
            &text,
            Some("seq 2040"),
            &limits(200, 0, true),
            Some(dir.path()),
        )
        .unwrap();
        let lines: Vec<&str> = shrunk.text.lines().collect();
        assert_eq!(lines.len(), 201);
        assert_eq!((lines[0], lines[99]), ("line 1", "line 100"));
        assert_eq!(
            lines[100],
            "[1,840 lines left out; the full output is in .moebius/out/1.txt]"
        );
        assert_eq!((lines[101], lines[200]), ("line 1941", "line 2040"));
        assert_eq!(shrunk.saved.as_deref(), Some(".moebius/out/1.txt"));
        assert_eq!(shrunk.steps, ["budget"]);
        assert_eq!((shrunk.lines_before, shrunk.lines_after), (2040, 201));
        assert_eq!(shrunk.bytes_before, text.len());
        assert_eq!(shrunk.bytes_after, shrunk.text.len());
        let out = dir.path().join(OUT_DIR);
        assert_eq!(std::fs::read_to_string(out.join("1.txt")).unwrap(), text);
        // The next one gets the next number.
        let again = shrink(&text, None, &limits(200, 0, true), Some(dir.path())).unwrap();
        assert_eq!(again.saved.as_deref(), Some(".moebius/out/2.txt"));
    }

    #[test]
    fn the_byte_budget_holds_too_even_for_one_long_line() {
        let text = numbered(1000);
        let shrunk = shrink(&text, None, &limits(0, 1024, false), None).unwrap();
        assert!(shrunk.text.len() < 1100, "{}", shrunk.text.len());
        assert!(shrunk.text.contains("lines left out]"), "{}", shrunk.text);
        let long = "é".repeat(10_000);
        let shrunk = shrink(&long, None, &limits(0, 1024, false), None).unwrap();
        assert!(shrunk.text.len() < 1100);
        assert!(shrunk.text.contains("bytes left out]"), "{}", shrunk.text);
    }

    #[test]
    fn without_limits_and_compression_nothing_changes() {
        let text = format!("\x1b[31m{}\x1b[0m", numbered(5000));
        assert_eq!(
            shrink(&text, Some("cargo test"), &limits(0, 0, false), None),
            None
        );
    }

    #[test]
    fn a_test_run_is_compressed_and_saved_whole() {
        let dir = tempfile::tempdir().unwrap();
        let mut text = String::from(
            "   Compiling demo v0.1.0\n     Running unittests src/lib.rs (target/debug/deps/demo-1)\n\nrunning 40 tests\n",
        );
        for i in 0..40 {
            text += &format!("test t{i} ... ok\n");
        }
        text += "\ntest result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
        let shrunk = shrink(
            &text,
            Some("cargo test"),
            &ToolOutput::default(),
            Some(dir.path()),
        )
        .unwrap();
        assert_eq!(shrunk.steps, ["cargo-test"]);
        assert!(
            shrunk.text.trim_end().ends_with(
                "passing tests left out; test binaries passed whole: 1 (40 passed, 0 ignored); the full output is in .moebius/out/1.txt]"
            ),
            "{}",
            shrunk.text
        );
        assert!(shrunk.bytes_after < shrunk.bytes_before / 4);
    }

    #[test]
    fn numbers_are_grouped_by_thousands() {
        assert_eq!(thousands(7), "7");
        assert_eq!(thousands(1840), "1,840");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
