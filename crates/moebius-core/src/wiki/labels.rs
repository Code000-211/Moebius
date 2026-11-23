//! A page's labels (ADR 0022): any number of short words kept in the page file as YAML front matter, such as
//! `---\nlabels: [Runbook, On call]\n---` at the top of the text. Reading finds them, and setting them rewrites only
//! the `labels` key, so any other key stays as it was. Front matter that is not closed by a `---` line is not front
//! matter: such a page has no labels and its text is left as it is.

use super::{Author, PageView, Wiki};
use crate::app::{App, AppError, Result};

/// The longest label, in characters.
const LABEL_LIMIT: usize = 40;

/// What a label may not hold, which would not survive as one item of a YAML list.
const REFUSED: &[char] = &[
    ',', '[', ']', '{', '}', '"', '\'', '\\', '#', ':', '|', '>', '*', '&', '!', '%', '@', '`',
];

/// A text cut into its front matter lines (the lines between the two `---` lines) and what follows.
struct Split<'a> {
    front: Vec<&'a str>,
    body: &'a str,
}

/// The front matter of `text` and the body after it, or `None` when the text has none or it is not closed.
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
    if take()?.trim_end() != "---" {
        return None;
    }
    let mut front = Vec::new();
    loop {
        let line = take()?;
        if matches!(line.trim_end(), "---" | "...") {
            return Some(Split { front, body: rest });
        }
        front.push(line);
    }
}

/// The text without its front matter.
pub(super) fn body(text: &str) -> &str {
    split(text).map_or(text, |split| split.body)
}

/// The labels of `text`, in the order written; none for a text without front matter, with front matter that is not
/// closed, or whose `labels` is not a list or a word.
pub(super) fn labels_of(text: &str) -> Vec<String> {
    let Some(split) = split(text) else {
        return Vec::new();
    };
    let Some(at) = split.front.iter().position(|line| key_of(line).is_some()) else {
        return Vec::new();
    };
    let value = split.front[at]
        .split_once(':')
        .map_or("", |(_, value)| value)
        .trim();
    let items: Vec<String> = if value.is_empty() {
        split.front[at + 1..]
            .iter()
            .map_while(|line| line.trim_start().strip_prefix("- "))
            .map(|item| unquote(item.trim()))
            .collect()
    } else if let Some(inner) = value.strip_prefix('[') {
        match inner.strip_suffix(']') {
            Some(inner) => inner.split(',').map(|item| unquote(item.trim())).collect(),
            None => return Vec::new(),
        }
    } else {
        vec![unquote(value)]
    };
    let mut labels: Vec<String> = Vec::new();
    for item in items.into_iter().filter(|item| !item.is_empty()) {
        if !labels.contains(&item) {
            labels.push(item);
        }
    }
    labels
}

/// `labels` when the line is the top-level key of that name.
fn key_of(line: &str) -> Option<&str> {
    line.strip_prefix("labels:")
        .filter(|rest| rest.is_empty() || rest.starts_with(' '))
}

fn unquote(item: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inner) = item
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.to_string();
        }
    }
    item.to_string()
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

/// `text` with its labels set to `labels` (already clean): the `labels` key is made, changed or, when there are
/// none, taken away; any other key and the body stay as they are, and front matter left empty goes.
pub(super) fn with_labels(text: &str, labels: &[String]) -> String {
    let line = (!labels.is_empty()).then(|| format!("labels: [{}]", labels.join(", ")));
    let Some(split) = split(text) else {
        return match line {
            Some(line) => format!("---\n{line}\n---\n{text}"),
            None => text.to_string(),
        };
    };
    let mut front: Vec<String> = Vec::new();
    let mut placed = false;
    let mut lines = split.front.iter().peekable();
    while let Some(each) = lines.next() {
        if key_of(each).is_none() {
            front.push((*each).to_string());
            continue;
        }
        // The key's own block list goes with it.
        while lines
            .next_if(|next| next.trim_start().starts_with("- ") || next.trim().is_empty())
            .is_some()
        {}
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
    format!("---\n{}\n---\n{}", front.join("\n"), split.body)
}

impl Wiki {
    /// The person `by` sets the labels of the page `path`: a commit that changes the front matter and nothing else.
    /// `base` is the commit of the page as the person read it, as for [`Wiki::write_page_from`].
    pub fn set_labels(
        &self,
        app: &App,
        by: &Author,
        path: &str,
        labels: &[String],
        base: Option<&str>,
    ) -> Result<PageView> {
        let labels = clean(labels)?;
        let text = self.page(path, None)?.text;
        let message = if labels.is_empty() {
            format!("Remove the labels of {path}")
        } else {
            format!("Label {path}: {}", labels.join(", "))
        };
        self.write_page_from(
            app,
            by,
            path,
            &with_labels(&text, &labels),
            Some(&message),
            base,
        )
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
}
