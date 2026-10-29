//! A rules file as text: a rule's `## name` section put back where it was, appended or dropped, and what a rule's
//! name may be.

/// The heading of a rule's section, if `line` is one.
fn heading(line: &str) -> Option<&str> {
    line.strip_prefix("## ").map(str::trim)
}

/// `file` with the `## name` section replaced by `section`, or dropped when `section` is blank; the parts of the file
/// are kept apart by one blank line. None when the file has no rule of that name.
pub fn replace_rule(file: &str, name: &str, section: &str) -> Option<String> {
    let lines: Vec<&str> = file.lines().collect();
    let start = lines.iter().position(|line| heading(line) == Some(name))?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| heading(line).is_some())
        .map_or(lines.len(), |offset| start + 1 + offset);
    let before = lines[..start].join("\n");
    let after = lines[end..].join("\n");
    Some(joined(&[&before, section, &after]))
}

/// `file` with `section` added at its end.
pub fn append_rule(file: &str, section: &str) -> String {
    joined(&[file, section])
}

/// The parts that are not blank, one blank line apart, ending with a line break; empty when all are blank.
fn joined(parts: &[&str]) -> String {
    let parts: Vec<&str> = parts
        .iter()
        .map(|part| part.trim_end())
        .filter(|part| !part.trim().is_empty())
        .collect();
    match parts.as_slice() {
        [] => String::new(),
        parts => format!("{}\n", parts.join("\n\n")),
    }
}

/// A rule as its section of a rules file.
pub fn section(name: &str, text: &str) -> String {
    format!("## {}\n{}", name.trim(), text.trim_end())
}

/// Whether `name` can be a rule's name: not blank, without spaces or line breaks.
pub fn valid_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && !name.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_is_put_back_where_it_was_appended_or_dropped() {
        let file = "Intro.\n\n## a\none\n\n## b\ntwo\n";
        assert_eq!(
            replace_rule(file, "a", "## a\nfirst\nsecond\n\n\n").unwrap(),
            "Intro.\n\n## a\nfirst\nsecond\n\n## b\ntwo\n"
        );
        assert_eq!(
            replace_rule(file, "b", "## b\nlast").unwrap(),
            "Intro.\n\n## a\none\n\n## b\nlast\n"
        );
        assert_eq!(
            replace_rule("## a\none", "a", "## renamed\nnew").unwrap(),
            "## renamed\nnew\n"
        );
        assert_eq!(
            replace_rule(file, "a", "").unwrap(),
            "Intro.\n\n## b\ntwo\n"
        );
        assert_eq!(replace_rule("## a\none\n", "a", ""), Some(String::new()));
        assert_eq!(replace_rule(file, "c", "## c\nx"), None);
        assert_eq!(
            append_rule(file, "## c\nthree"),
            "Intro.\n\n## a\none\n\n## b\ntwo\n\n## c\nthree\n"
        );
        assert_eq!(append_rule("", "## c\nthree"), "## c\nthree\n");
    }

    #[test]
    fn a_rules_name_has_no_spaces() {
        assert!(valid_name(" no-vendoring "));
        assert!(!valid_name("two words"));
        assert!(!valid_name("  "));
    }
}
