//! What the floating chat tells the Moebius agent of a project's tab: the file open in the editor shown with the lines
//! around the cursor and what is selected, cut to a sensible size, or the terminal or chat shown, and the directories
//! the Files sidebar has open.

use super::{Body, Project, TEXT_LIMIT, Tab};

/// How many lines around the cursor, and how many characters in all, the floating chat is told of the file shown.
const CONTEXT_LINES: usize = 40;

const CONTEXT_CHARS: usize = 3_000;

/// How many characters of the selection the floating chat is told.
const SELECTION_CHARS: usize = 600;

/// `text` cut to `limit` characters, saying so when it was cut.
fn cut(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_string();
    }
    let start: String = text.chars().take(limit).collect();
    format!("{start}\n[cut: the first {limit} of {count} characters]")
}

impl Project {
    /// What the floating chat is told of the tab shown, below the repository's line.
    pub fn context(&self) -> Vec<String> {
        let mut lines = self.find_context();
        lines.extend(self.tab_context());
        lines
    }

    fn tab_context(&self) -> Vec<String> {
        let mut lines = match &self.tab {
            Tab::Board => Vec::new(),
            Tab::Editor(_) => self.editor_context(),
            Tab::Terminal(_) => {
                vec!["A terminal in the project's clone on the server is shown.".to_string()]
            }
            Tab::Chat(_) => vec!["A chat with an agent in this project is shown.".to_string()],
        };
        lines.extend(self.sidebar_context());
        lines
    }

    /// What the Files sidebar shows, when it is shown and has directories open.
    pub fn sidebar_context(&self) -> Option<String> {
        if self.files_folded || self.open_dirs.is_empty() {
            return None;
        }
        let open: Vec<&str> = self.open_dirs.iter().map(String::as_str).collect();
        Some(format!(
            "The Files sidebar shows the default branch; open directories: {}",
            open.join(", ")
        ))
    }

    fn editor_context(&self) -> Vec<String> {
        let Some(file) = self.shown() else {
            return vec!["The editor has no file open.".to_string()];
        };
        let mut lines = vec![format!(
            "Open file: {}{}",
            file.path,
            if file.unsaved() {
                " (unsaved changes)"
            } else {
                ""
            }
        )];
        let others: Vec<&str> = self
            .files
            .iter()
            .filter(|other| other.path != file.path)
            .map(|other| other.path.as_str())
            .collect();
        if !others.is_empty() {
            lines.push(format!("Also open: {}", others.join(", ")));
        }
        let content = match &file.body {
            Body::Text { content, .. } => content,
            Body::Reading => return lines,
            Body::Binary => {
                lines.push("It is a binary file and is not shown.".to_string());
                return lines;
            }
            Body::TooLarge => {
                lines.push(format!("It is larger than {TEXT_LIMIT} and is not shown."));
                return lines;
            }
            Body::Failed(error) => {
                lines.push(format!("It could not be read: {error}"));
                return lines;
            }
        };
        let cursor = content.cursor().position;
        lines.push(format!(
            "Cursor: line {}, column {}",
            cursor.line + 1,
            cursor.column + 1
        ));
        if let Some(selection) = content.selection().filter(|s| !s.is_empty()) {
            lines.push(format!(
                "Selected text:\n{}",
                cut(&selection, SELECTION_CHARS)
            ));
        }
        let first = cursor.line.saturating_sub(CONTEXT_LINES / 2);
        let last = (first + CONTEXT_LINES).min(content.line_count());
        let around: Vec<String> = (first..last)
            .filter_map(|index| {
                let line = content.line(index)?;
                Some(format!("{:>5} | {}", index + 1, line.text))
            })
            .collect();
        let used: usize = lines.iter().map(|line| line.chars().count() + 1).sum();
        lines.push(format!(
            "Lines {}-{} of {}:\n{}",
            first + 1,
            last,
            content.line_count(),
            cut(
                &around.join("\n"),
                CONTEXT_CHARS.saturating_sub(used).max(500)
            )
        ));
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_long_text_is_cut_and_says_so() {
        assert_eq!(cut("short", 10), "short");
        assert_eq!(cut("abcdef", 3), "abc\n[cut: the first 3 of 6 characters]");
    }
}
