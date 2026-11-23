//! A page's labels (ADR 0022): any number of short words kept in the page file as YAML front matter, such as
//! `---\nlabels: [Runbook, On call]\n---` at the top of the text. Reading finds them, and setting them rewrites only
//! the `labels` key (its whole value, however many lines it takes), so any other key stays as it was, byte for byte,
//! and the file keeps its line ending.
//!
//! Text is front matter only when it is closed by a `---` line and what lies between the two fences reads as YAML (a
//! `key:` line, a `- ` item, a comment, an indented line, or a blank one, with at least one that says something). A
//! page that starts with a horizontal rule, a paragraph and another rule has none, and neither has one that is not
//! closed: such a page has no labels and its text is left as it is.

use super::{Author, PageView, Wiki};
use crate::app::{App, AppError, Result};

/// The longest label, in characters.
const LABEL_LIMIT: usize = 40;

/// What a label may not hold, which would not survive as one item of a YAML list.
const REFUSED: &[char] = &[
    ',', '[', ']', '{', '}', '"', '\'', '\\', '#', ':', '|', '>', '*', '&', '!', '%', '@', '`',
];

/// A text cut into its front matter lines (the lines between the two `---` lines, without their line endings), what
/// follows, and the line ending the front matter is written with.
struct Split<'a> {
    front: Vec<&'a str>,
    body: &'a str,
    eol: &'static str,
}

/// Whether `line` could be a line of a YAML mapping: blank, a comment, indented, an item, or `key:`.
fn looks_like_yaml(line: &str) -> bool {
    let line = line.trim_end();
    if line.is_empty()
        || line.starts_with(['#', ' ', '\t'])
        || line == "-"
        || line.starts_with("- ")
    {
        return true;
    }
    line.split_once(':').is_some_and(|(key, rest)| {
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
            && (rest.is_empty() || rest.starts_with(' '))
    })
}

/// The front matter of `text` and the body after it, or `None` when the text has none.
fn split(text: &str) -> Option<Split<'_>> {
    let mut rest = text;
    let mut take = || -> Option<&str> {
        if rest.is_empty() {
            return None;
        }
        let (line, tail) = rest.split_once('\n').unwrap_or((rest, ""));
        rest = tail;
        Some(line.strip_suffix('\r').unwrap_or(line))
    };
    let opener = take()?;
    if opener.trim_end() != "---" {
        return None;
    }
    let eol = eol_of(text);
    let mut front = Vec::new();
    loop {
        let line = take()?;
        if matches!(line.trim_end(), "---" | "...") {
            let says_something = front.iter().any(|line: &&str| !line.trim().is_empty());
            if !says_something || !front.iter().all(|line| looks_like_yaml(line)) {
                return None;
            }
            return Some(Split {
                front,
                body: rest,
                eol,
            });
        }
        front.push(line);
    }
}

/// The text without its front matter.
pub(super) fn body(text: &str) -> &str {
    split(text).map_or(text, |split| split.body)
}

/// Where the value of the `labels` key at `at` ends (the index after its last line) and the labels it holds; `None`
/// labels when the value is not a list or a word, such as a list that is never closed.
fn value_of(front: &[&str], at: usize) -> (usize, Vec<String>) {
    let first = front[at].strip_prefix("labels:").unwrap_or("");
    let value = without_comment(first).trim();
    if value.starts_with('[') {
        // A flow list may go on over several lines: take lines until it closes.
        let mut text = value.to_string();
        for end in at + 1..=front.len() {
            if let Some(items) = flow_list(&text) {
                return (end, items);
            }
            match front.get(end) {
                Some(line) => {
                    text.push('\n');
                    text.push_str(line);
                }
                None => break,
            }
        }
        return (at + 1, Vec::new());
    }
    if value.is_empty() {
        let mut end = at + 1;
        let mut items = Vec::new();
        while let Some(line) = front.get(end) {
            let line = without_comment(line).trim();
            if let Some(item) = line.strip_prefix("- ").or(line.strip_prefix('-')) {
                items.push(unquote(item.trim()));
                end += 1;
            } else if line.is_empty()
                && front[end..].iter().any(|l| l.trim_start().starts_with('-'))
            {
                end += 1;
            } else {
                break;
            }
        }
        // Blank lines after the last item belong to what follows.
        while end > at + 1 && front[end - 1].trim().is_empty() {
            end -= 1;
        }
        return (end, items);
    }
    if matches!(value, "~" | "null" | "Null" | "NULL") {
        return (at + 1, Vec::new());
    }
    (at + 1, vec![unquote(value)])
}

/// The labels of `text`, in the order written, each once; none for a text without front matter, and none when
/// `labels` is not a list, a block list or a word (`null` and `~` say there are none).
pub(super) fn labels_of(text: &str) -> Vec<String> {
    let Some(split) = split(text) else {
        return Vec::new();
    };
    let Some(at) = split.front.iter().position(|line| key_of(line)) else {
        return Vec::new();
    };
    let mut labels: Vec<String> = Vec::new();
    for item in value_of(&split.front, at).1 {
        if !item.is_empty() && !labels.contains(&item) {
            labels.push(item);
        }
    }
    labels
}

/// Whether the line is the top-level key `labels`.
fn key_of(line: &str) -> bool {
    line.strip_prefix("labels:")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

/// `text` up to a `#` that starts a comment (one at the start, or after a space), outside quotes.
fn without_comment(text: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut before = ' ';
    for (at, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' && before.is_whitespace() => return &text[..at],
            None => {}
        }
        before = c;
    }
    text
}

/// A word as YAML writes it, without its quotes (`""` and `''` quote, a backslash escapes in double quotes).
fn unquote(item: &str) -> String {
    let inner = |quote: char| {
        item.strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
    };
    if let Some(inner) = inner('"') {
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            match (c, c == '\\') {
                (_, true) => out.extend(chars.next()),
                _ => out.push(c),
            }
        }
        return out;
    }
    if let Some(inner) = inner('\'') {
        return inner.replace("''", "'");
    }
    item.to_string()
}

/// The items of the flow list in `text`, which starts with `[`: items are split at commas outside quotes, and what
/// follows the closing `]` may only be a comment. `None` when the list is not closed or is followed by more.
fn flow_list(text: &str) -> Option<Vec<String>> {
    let mut chars = text.strip_prefix('[')?.char_indices().peekable();
    let inner = &text[1..];
    let mut items: Vec<String> = Vec::new();
    let mut start = 0;
    let mut quote: Option<char> = None;
    let mut before = ' ';
    while let Some((at, c)) = chars.next() {
        match quote {
            Some('"') if c == '\\' => {
                chars.next();
            }
            Some(q) if c == q => {
                if q == '\'' && chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            }
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' && before.is_whitespace() => {
                // A comment inside the list runs to the end of its line.
                for (_, next) in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            None if c == ',' => {
                items.push(item_of(&inner[start..at]));
                start = at + 1;
            }
            None if c == ']' => {
                items.push(item_of(&inner[start..at]));
                let after = inner[at + 1..].trim();
                if !after.is_empty() && !after.starts_with('#') {
                    return None;
                }
                items.retain(|item| !item.is_empty());
                return Some(items);
            }
            None => {}
        }
        before = c;
    }
    None
}

/// One item of a flow list, its comments and the white space around it taken off, unquoted.
fn item_of(raw: &str) -> String {
    let mut text = String::new();
    for line in raw.lines() {
        text.push_str(without_comment(line));
        text.push(' ');
    }
    unquote(text.trim())
}

/// `labels` checked and cleaned: trimmed, without repeats, each a plain word or words.
pub(super) fn clean(labels: &[String]) -> Result<Vec<String>> {
    let mut clean: Vec<String> = Vec::new();
    for label in labels {
        let label = label.trim();
        if label.is_empty() {
            continue;
        }
        if label.chars().count() > LABEL_LIMIT {
            return Err(AppError::Invalid(format!(
                "the label {label:?} is longer than {LABEL_LIMIT} characters"
            )));
        }
        if let Some(c) = label
            .chars()
            .find(|c| c.is_control() || REFUSED.contains(c))
        {
            return Err(AppError::Invalid(format!(
                "the label {label:?} has the character {c:?}"
            )));
        }
        if !clean.iter().any(|each| each == label) {
            clean.push(label.to_string());
        }
    }
    Ok(clean)
}

/// The line ending a text without front matter is written with: that of its first line.
fn eol_of(text: &str) -> &'static str {
    match text.find('\n') {
        Some(at) if text[..at].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// `text` with its labels set to `labels` (already clean): the `labels` key, its whole value, is made, changed or,
/// when there are none, taken away; any other line and the body stay as they are, front matter left empty goes, and
/// the line ending of the file is kept.
pub(super) fn with_labels(text: &str, labels: &[String]) -> String {
    let line = (!labels.is_empty()).then(|| format!("labels: [{}]", labels.join(", ")));
    let Some(split) = split(text) else {
        let eol = eol_of(text);
        return match line {
            Some(line) => format!("---{eol}{line}{eol}---{eol}{text}"),
            None => text.to_string(),
        };
    };
    let mut front: Vec<String> = Vec::new();
    let mut placed = false;
    let mut at = 0;
    while at < split.front.len() {
        if !key_of(split.front[at]) {
            front.push(split.front[at].to_string());
            at += 1;
            continue;
        }
        at = value_of(&split.front, at).0;
        if let Some(line) = &line
            && !placed
        {
            front.push(line.clone());
            placed = true;
        }
    }
    if let (Some(line), false) = (&line, placed) {
        front.push(line.clone());
    }
    if front.iter().all(|each| each.trim().is_empty()) {
        return split.body.to_string();
    }
    let eol = split.eol;
    format!("---{eol}{}{eol}---{eol}{}", front.join(eol), split.body)
}

impl Wiki {
    /// The person `by` sets the labels of the page `path`: a commit that changes the front matter and nothing else.
    /// The page is read and rewritten in the turn of the write, after the clone has followed `origin`, so an edit
    /// that came meanwhile is kept. `base` is the commit of the page as the person read it, as for
    /// [`Wiki::write_page_from`].
    pub fn set_labels(
        &self,
        app: &App,
        by: &Author,
        path: &str,
        labels: &[String],
        base: Option<&str>,
    ) -> Result<PageView> {
        let labels = clean(labels)?;
        let message = if labels.is_empty() {
            format!("Remove the labels of {path}")
        } else {
            format!("Label {path}: {}", labels.join(", "))
        };
        self.write_page_with(app, by, path, Some(&message), base, |now| match now {
            Some(text) => Ok(with_labels(text, &labels)),
            None => Err(AppError::NotFound(format!("page {path}"))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn a_page_without_front_matter_has_no_labels() {
        assert!(labels_of("# On call\n\nWho.\n").is_empty());
        assert!(labels_of("").is_empty());
        assert!(labels_of("---\nlabels: [A]\n").is_empty(), "never closed");
        assert!(labels_of("text\n---\nlabels: [A]\n---\n").is_empty());
        assert_eq!(body("# On call\n"), "# On call\n");
    }

    #[test]
    fn labels_are_read_from_a_flow_list_a_block_list_or_a_word() {
        let flow = "---\nlabels: [Runbook, On call]\n---\n# Page\n";
        assert_eq!(labels_of(flow), words(&["Runbook", "On call"]));
        assert_eq!(body(flow), "# Page\n");
        let block = "---\nlabels:\n  - Runbook\n  - \"On call\"\n---\n";
        assert_eq!(labels_of(block), words(&["Runbook", "On call"]));
        assert_eq!(
            labels_of("---\nlabels: Runbook\n---\n"),
            words(&["Runbook"])
        );
        assert_eq!(
            labels_of("---\r\nlabels: [A, A, b]\r\n---\r\nbody"),
            words(&["A", "b"])
        );
        assert!(labels_of("---\nlabels: [A, B\n---\n").is_empty());
        assert!(labels_of("---\ntitle: x\n---\n").is_empty());
    }

    #[test]
    fn setting_labels_keeps_the_other_keys_and_the_body() {
        let text = "---\ntitle: Deploy\nlabels: [Old]\nowner: ada\n---\n# Deploy\n";
        assert_eq!(
            with_labels(text, &words(&["Runbook", "On call"])),
            "---\ntitle: Deploy\nlabels: [Runbook, On call]\nowner: ada\n---\n# Deploy\n"
        );
        assert_eq!(
            with_labels(text, &[]),
            "---\ntitle: Deploy\nowner: ada\n---\n# Deploy\n"
        );
        assert_eq!(
            with_labels("---\ntitle: x\n---\nbody\n", &words(&["A"])),
            "---\ntitle: x\nlabels: [A]\n---\nbody\n"
        );
        assert_eq!(
            with_labels("# Page\n", &words(&["A"])),
            "---\nlabels: [A]\n---\n# Page\n"
        );
        assert_eq!(
            with_labels(
                "---\nlabels:\n  - A\n  - B\nowner: ada\n---\nx\n",
                &words(&["C"])
            ),
            "---\nlabels: [C]\nowner: ada\n---\nx\n"
        );
    }

    #[test]
    fn the_last_label_taken_away_takes_the_front_matter() {
        assert_eq!(with_labels("---\nlabels: [A]\n---\n# P\n", &[]), "# P\n");
        assert_eq!(with_labels("# P\n", &[]), "# P\n");
        // Front matter that is not closed is left as it is, and no label is read from it.
        let open = "---\nlabels: [A]\n# P\n";
        assert_eq!(
            with_labels(open, &[]),
            open,
            "an unclosed block is not front matter"
        );
    }

    #[test]
    fn a_label_is_a_plain_word_or_words() {
        assert_eq!(
            clean(&words(&[" Runbook ", "", "Runbook", "On call"])).unwrap(),
            words(&["Runbook", "On call"])
        );
        assert!(clean(&words(&["a,b"])).is_err());
        assert!(clean(&words(&["a]"])).is_err());
        assert!(clean(&words(&["a\nb"])).is_err());
        assert!(clean(&[("x".repeat(41))]).is_err());
    }

    #[test]
    fn a_horizontal_rule_around_a_paragraph_is_not_front_matter() {
        let text = "---\n\nIntro paragraph\n\n---\n\nRest\n";
        assert_eq!(body(text), text);
        assert!(labels_of(text).is_empty());
        // Setting a label puts front matter before the prose and leaves the prose as it was.
        assert_eq!(
            with_labels(text, &words(&["A"])),
            format!("---\nlabels: [A]\n---\n{text}")
        );
        // Two rules with nothing between them, or only blank lines, are rules too.
        assert_eq!(body("---\n---\nx\n"), "---\n---\nx\n");
        assert_eq!(body("---\n\n---\nx\n"), "---\n\n---\nx\n");
        // A comment or an indented line between the fences is YAML.
        assert_eq!(body("---\n# note\n---\nx\n"), "x\n");
    }

    #[test]
    fn a_crlf_file_keeps_its_line_ending_and_its_other_keys_byte_for_byte() {
        let text = "---\r\ntitle:  Deploy \r\nlabels: [Old]\r\nowner: ada\r\n---\r\n# Deploy\r\n";
        assert_eq!(
            with_labels(text, &words(&["A", "B"])),
            "---\r\ntitle:  Deploy \r\nlabels: [A, B]\r\nowner: ada\r\n---\r\n# Deploy\r\n"
        );
        assert_eq!(
            with_labels("# P\r\nx\r\n", &words(&["A"])),
            "---\r\nlabels: [A]\r\n---\r\n# P\r\nx\r\n"
        );
        assert_eq!(
            with_labels(text, &[]),
            "---\r\ntitle:  Deploy \r\nowner: ada\r\n---\r\n# Deploy\r\n"
        );
    }

    #[test]
    fn hand_written_lists_are_read_as_yaml_reads_them() {
        let read = |value: &str| labels_of(&format!("---\nlabels: {value}\n---\n"));
        assert_eq!(read(r#"["a, b", c]"#), words(&["a, b", "c"]));
        assert_eq!(read("['it''s', c]"), words(&["it's", "c"]));
        assert_eq!(read("[a, b] # note"), words(&["a", "b"]));
        assert_eq!(read("[a, # one\n  b]"), words(&["a", "b"]));
        assert!(read("~").is_empty());
        assert!(read("null").is_empty());
        assert!(read("[]").is_empty());
        assert!(read("[a, b").is_empty(), "a list never closed");
        assert!(read("[a] b").is_empty(), "text after the list");
        assert_eq!(
            labels_of("---\nlabels: [a,\n  b,\n  c]\nowner: ada\n---\n"),
            words(&["a", "b", "c"])
        );
        assert_eq!(
            labels_of("---\nlabels: # none yet\n- a\n- 'b'\nowner: ada\n---\n"),
            words(&["a", "b"])
        );
    }

    #[test]
    fn setting_labels_replaces_the_whole_old_value() {
        let multi = "---\nlabels: [a,\n  b]\nowner: ada\n---\nx\n";
        assert_eq!(
            with_labels(multi, &words(&["c"])),
            "---\nlabels: [c]\nowner: ada\n---\nx\n"
        );
        assert_eq!(with_labels(multi, &[]), "---\nowner: ada\n---\nx\n");
        let block = "---\nowner: ada\nlabels:\n- a\n  # kept with it\n- b\n\ntitle: T\n---\nx\n";
        let set = with_labels(block, &words(&["c"]));
        assert!(set.starts_with("---\nowner: ada\nlabels: [c]\n"), "{set}");
        assert!(set.contains("title: T"), "{set}");
        assert_eq!(labels_of(&set), words(&["c"]));
        assert_eq!(
            labels_of(&with_labels(multi, &words(&["x", "y"]))),
            words(&["x", "y"])
        );
    }
}
