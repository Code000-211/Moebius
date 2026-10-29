//! What a proposal for Moebius's own configuration changes, as a person reads it: a unified diff of a file's text
//! before and after, with three lines of context, as `git diff` shows one.

/// The lines of context around each change.
const CONTEXT: usize = 3;

/// Above this many line pairs the files are not compared line by line: the whole old text is shown removed and the
/// whole new text added.
const LIMIT: usize = 4_000_000;

/// One line of the diff: in both texts, only in the old one, or only in the new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line<'a> {
    Same(&'a str),
    Removed(&'a str),
    Added(&'a str),
}

/// The lines of `old` and `new` in order, the longest run of common lines kept.
fn lines<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Line<'a>> {
    if old.len() * new.len() > LIMIT {
        let removed = old.iter().map(|line| Line::Removed(line));
        return removed.chain(new.iter().map(|l| Line::Added(l))).collect();
    }
    // common[i][j]: the length of the longest common lines of old[i..] and new[j..].
    let mut common = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i][j] = if old[i] == new[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            out.push(Line::Same(old[i]));
            (i, j) = (i + 1, j + 1);
        } else if i < old.len() && (j == new.len() || common[i + 1][j] >= common[i][j + 1]) {
            out.push(Line::Removed(old[i]));
            i += 1;
        } else {
            out.push(Line::Added(new[j]));
            j += 1;
        }
    }
    out
}

/// The start of a hunk's range as its header gives it: the first line, counting from 1, or the line before an empty
/// range.
fn start(at: usize, count: usize) -> usize {
    if count == 0 { at } else { at + 1 }
}

/// The unified diff of the file `path` from `old` to `new`, with the headers `--- a/<path>` and `+++ b/<path>` (`---
/// /dev/null` for a file that is new); empty when they are the same.
pub fn unified(path: &str, old: &str, new: &str) -> String {
    let (old_lines, new_lines): (Vec<&str>, Vec<&str>) =
        (old.lines().collect(), new.lines().collect());
    let lines = lines(&old_lines, &new_lines);
    let changed: Vec<usize> = (0..lines.len())
        .filter(|&n| !matches!(lines[n], Line::Same(_)))
        .collect();
    if changed.is_empty() {
        return String::new();
    }
    // Where each line of the diff is in the old and in the new text.
    let mut at = Vec::with_capacity(lines.len());
    let (mut in_old, mut in_new) = (0, 0);
    for line in &lines {
        at.push((in_old, in_new));
        match line {
            Line::Same(_) => (in_old, in_new) = (in_old + 1, in_new + 1),
            Line::Removed(_) => in_old += 1,
            Line::Added(_) => in_new += 1,
        }
    }
    let from = if old.is_empty() {
        "/dev/null".to_string()
    } else {
        format!("a/{path}")
    };
    let mut out = format!("--- {from}\n+++ b/{path}\n");
    let mut n = 0;
    while n < changed.len() {
        // A hunk takes the changes whose context touches.
        let mut last = n;
        while last + 1 < changed.len() && changed[last + 1] - changed[last] <= 2 * CONTEXT + 1 {
            last += 1;
        }
        let first = changed[n].saturating_sub(CONTEXT);
        let end = (changed[last] + CONTEXT + 1).min(lines.len());
        let hunk = &lines[first..end];
        let olds = hunk.iter().filter(|l| !matches!(l, Line::Added(_))).count();
        let news = hunk
            .iter()
            .filter(|l| !matches!(l, Line::Removed(_)))
            .count();
        let (old_at, new_at) = at[first];
        out += &format!(
            "@@ -{},{olds} +{},{news} @@\n",
            start(old_at, olds),
            start(new_at, news)
        );
        for line in hunk {
            match line {
                Line::Same(text) => out += &format!(" {text}\n"),
                Line::Removed(text) => out += &format!("-{text}\n"),
                Line::Added(text) => out += &format!("+{text}\n"),
            }
        }
        n = last + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_is_shown_with_three_lines_of_context() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nd\nE\nf\ng\nh\n";
        assert_eq!(
            unified("x.toml", old, new),
            "--- a/x.toml\n+++ b/x.toml\n@@ -2,7 +2,7 @@\n b\n c\n d\n-e\n+E\n f\n g\n h\n"
        );
        assert_eq!(unified("x.toml", old, old), "");
    }

    #[test]
    fn changes_far_apart_are_hunks_of_their_own_and_close_ones_share_one() {
        let old: String = (1..=20).map(|n| format!("l{n}\n")).collect();
        let new = old.replace("l2\n", "two\n").replace("l19\n", "nineteen\n");
        let diff = unified("f", &old, &new);
        assert_eq!(diff.matches("@@ -").count(), 2, "{diff}");
        assert!(
            diff.contains("@@ -1,5 +1,5 @@\n l1\n-l2\n+two\n l3\n l4\n l5\n"),
            "{diff}"
        );
        assert!(
            diff.contains("@@ -16,5 +16,5 @@\n l16\n l17\n l18\n-l19\n+nineteen\n l20\n"),
            "{diff}"
        );
        let near = old.replace("l2\n", "two\n").replace("l8\n", "eight\n");
        assert_eq!(unified("f", &old, &near).matches("@@ -").count(), 1);
    }

    #[test]
    fn a_new_file_and_added_lines_have_empty_ranges() {
        assert_eq!(
            unified(".moebius/rules.md", "", "# Rules\n"),
            "--- /dev/null\n+++ b/.moebius/rules.md\n@@ -0,0 +1,1 @@\n+# Rules\n"
        );
        assert_eq!(
            unified("f", "a\n", "a\nb\n"),
            "--- a/f\n+++ b/f\n@@ -1,1 +1,2 @@\n a\n+b\n"
        );
    }
}
