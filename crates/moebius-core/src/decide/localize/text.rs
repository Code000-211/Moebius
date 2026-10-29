//! Reading text for the search of the `localize` baseline: identifiers split on camelCase and snake_case into
//! lowercase words, the words an issue is searched by, and the identifiers and file names an issue names literally.

use std::collections::HashSet;

/// Words that say nothing about where a change goes.
const STOP_WORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "are",
    "but",
    "not",
    "you",
    "all",
    "any",
    "can",
    "had",
    "her",
    "was",
    "one",
    "our",
    "out",
    "has",
    "have",
    "this",
    "that",
    "with",
    "from",
    "they",
    "will",
    "would",
    "there",
    "their",
    "what",
    "when",
    "which",
    "should",
    "could",
    "about",
    "into",
    "than",
    "then",
    "them",
    "these",
    "those",
    "also",
    "been",
    "does",
    "doing",
    "done",
    "each",
    "just",
    "like",
    "make",
    "more",
    "most",
    "much",
    "only",
    "other",
    "over",
    "same",
    "some",
    "such",
    "very",
    "your",
    "its",
    "get",
    "set",
    "use",
    "used",
    "using",
    "add",
    "issue",
    "please",
    "currently",
    "too",
    "instead",
    "want",
    "need",
    "needs",
    "how",
    "why",
    "who",
    "where",
    "while",
    "after",
    "before",
    "because",
];

/// The longest run of letters, digits and underscores that is read as a word; longer ones are hashes and encodings.
const LONGEST_RUN: usize = 64;

/// The most terms an issue is searched by.
const MOST_TERMS: usize = 80;

/// The most identifiers and file names an issue is taken to name.
const MOST_NAMED: usize = 12;

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The runs of letters, digits and underscores of `text` that can be words.
fn runs(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !is_word_char(c))
        .filter(|run| !run.is_empty() && run.chars().count() <= LONGEST_RUN)
}

/// The lowercase parts of an identifier: split at underscores, between a lowercase letter or digit and a capital, and
/// before the last capital of a run of capitals that a lowercase letter follows (`HTTPServer` is `http` and `server`).
fn parts(run: &str) -> Vec<String> {
    let chars: Vec<char> = run.chars().collect();
    let mut parts = Vec::new();
    let mut part = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            if !part.is_empty() {
                parts.push(std::mem::take(&mut part));
            }
            continue;
        }
        if c.is_uppercase() && i > 0 && !part.is_empty() {
            let before = chars[i - 1];
            let after = chars.get(i + 1).copied();
            let lower_to_upper = before.is_lowercase() || before.is_ascii_digit();
            let acronym_end = before.is_uppercase() && after.is_some_and(char::is_lowercase);
            if lower_to_upper || acronym_end {
                parts.push(std::mem::take(&mut part));
            }
        }
        part.extend(c.to_lowercase());
    }
    if !part.is_empty() {
        parts.push(part);
    }
    parts
}

/// The words of `text` as the index holds them: each run as a whole in lowercase when it is made of several parts, and
/// each of its parts of two characters or more, separated by spaces.
pub fn index_words(text: &str) -> String {
    let mut words = String::with_capacity(text.len() / 2);
    for run in runs(text) {
        let parts = parts(run);
        if parts.len() > 1 {
            push(&mut words, &run.to_lowercase());
        }
        for part in parts.iter().filter(|part| part.chars().count() >= 2) {
            push(&mut words, part);
        }
    }
    words
}

fn push(words: &mut String, word: &str) {
    if !words.is_empty() {
        words.push(' ');
    }
    words.push_str(word);
}

/// The terms an issue is searched by: the parts of its words and the identifiers they make, without the words that say
/// nothing, each once, in the order they come; at most [`MOST_TERMS`].
pub fn terms(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut terms = Vec::new();
    for run in runs(text) {
        let parts = parts(run);
        let whole = (parts.len() > 1).then(|| run.to_lowercase());
        for term in whole.into_iter().chain(parts) {
            if term.chars().count() >= 3
                && !STOP_WORDS.contains(&term.as_str())
                && !term.chars().all(|c| c.is_ascii_digit() || c == '_')
                && seen.insert(term.clone())
            {
                terms.push(term);
            }
        }
    }
    terms.truncate(MOST_TERMS);
    terms
}

/// The identifiers `text` names literally, in lowercase: those written in snake_case or camelCase, and any single word
/// of four characters or more in backticks.
pub fn identifiers(text: &str) -> Vec<String> {
    let mut named = Vec::new();
    for run in runs(text) {
        if run.chars().count() >= 5 && parts(run).len() > 1 {
            named.push(run.to_lowercase());
        }
    }
    for (i, quoted) in text.split('`').enumerate() {
        // Between two backticks, not outside them.
        if i % 2 == 1 && quoted.chars().count() >= 4 && quoted.chars().all(is_word_char) {
            named.push(quoted.to_lowercase());
        }
    }
    let mut seen = HashSet::new();
    named.retain(|name| !STOP_WORDS.contains(&name.as_str()) && seen.insert(name.clone()));
    named.truncate(MOST_NAMED);
    named
}

/// The file names and paths `text` names, in lowercase: the words that hold a slash, or end in a dot and an extension
/// of one to five characters of which one is a letter. Whether a file of that name exists is for the index to say.
pub fn file_names(text: &str) -> Vec<String> {
    let mut named = Vec::new();
    for word in text.split_whitespace() {
        let word = word
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '`' | '\''
                        | '"'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '<'
                        | '>'
                        | ','
                        | ';'
                        | ':'
                        | '!'
                        | '?'
                )
            })
            .trim_end_matches('.');
        let word = word.strip_prefix("./").unwrap_or(word);
        if word.chars().count() < 3 {
            continue;
        }
        let name = word.rsplit('/').next().unwrap_or(word);
        let has_extension = name.rsplit_once('.').is_some_and(|(stem, extension)| {
            stem.chars().count() >= 2
                && (1..=5).contains(&extension.chars().count())
                && extension.chars().all(char::is_alphanumeric)
                && extension.chars().any(char::is_alphabetic)
        });
        if word.contains('/') || has_extension {
            named.push(word.to_lowercase());
        }
    }
    let mut seen = HashSet::new();
    named.retain(|name| seen.insert(name.clone()));
    named.truncate(MOST_NAMED);
    named
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(run: &str) -> Vec<String> {
        parts(run)
    }

    #[test]
    fn identifiers_split_on_camel_case_snake_case_and_acronyms() {
        assert_eq!(split("parseConfig"), ["parse", "config"]);
        assert_eq!(split("parse_config_file"), ["parse", "config", "file"]);
        assert_eq!(split("HTTPServer"), ["http", "server"]);
        assert_eq!(split("ParseXMLFile2"), ["parse", "xml", "file2"]);
        assert_eq!(split("utf8"), ["utf8"]);
        assert_eq!(split("__init__"), ["init"]);
    }

    #[test]
    fn the_index_holds_whole_identifiers_and_their_parts() {
        assert_eq!(
            index_words("fn parseConfig(a: u8) { read_file(); }"),
            "fn parseconfig parse config u8 read_file read file"
        );
        assert_eq!(index_words("x = 1;"), "");
        // A run that is a hash is no word.
        assert_eq!(index_words(&"a".repeat(65)), "");
    }

    #[test]
    fn an_issue_is_searched_by_its_words_without_the_ones_that_say_nothing() {
        let found =
            terms("The parseConfig function should not crash on the empty file, parse_config too");
        assert_eq!(
            found,
            [
                "parseconfig",
                "parse",
                "config",
                "function",
                "crash",
                "empty",
                "file",
                "parse_config"
            ]
        );
    }

    #[test]
    fn identifiers_and_file_names_are_taken_where_the_issue_writes_them() {
        assert_eq!(
            identifiers(
                "`parse` is slow, and parse_config calls readFile; the word config is plain"
            ),
            ["parse_config", "readfile", "parse"]
        );
        assert_eq!(
            file_names(
                "See src/parser.rs, (config.toml) and ./docs/guide/index.md. Version 1.2 is fine, e.g. this."
            ),
            ["src/parser.rs", "config.toml", "docs/guide/index.md"]
        );
        assert!(file_names("nothing named here").is_empty());
    }
}
