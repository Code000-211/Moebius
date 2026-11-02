//! Versions and the ranges of Moebius versions a package works with (`moebius = ">=0.2"` in its manifest).
//!
//! A version is one to three numbers separated by dots (`1`, `0.2`, `1.4.7`), the missing ones being 0. A range is
//! comparators separated by commas or spaces: `>=`, `>`, `<=`, `<` and `=` (or none) followed by a version, or `*` for
//! every version. A version is in the range when it satisfies every comparator.

use std::cmp::Ordering;

/// A version of a package or of Moebius.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64, u64, u64);

impl Version {
    /// The version in `text`, or none when it is not one to three numbers separated by dots.
    pub fn parse(text: &str) -> Option<Self> {
        let mut numbers = [0u64; 3];
        for (count, part) in text.trim().split('.').enumerate() {
            if count == 3 || part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            numbers[count] = part.parse().ok()?;
        }
        Some(Self(numbers[0], numbers[1], numbers[2]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Equal,
}

/// A range of versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range(Vec<(Op, Version)>);

impl Range {
    /// The range in `text`, or none when a comparator is not one.
    pub fn parse(text: &str) -> Option<Self> {
        let mut comparators = Vec::new();
        for word in text.split([',', ' ']).filter(|word| !word.is_empty()) {
            if word == "*" {
                continue;
            }
            let (op, rest) = if let Some(rest) = word.strip_prefix(">=") {
                (Op::GreaterEqual, rest)
            } else if let Some(rest) = word.strip_prefix("<=") {
                (Op::LessEqual, rest)
            } else if let Some(rest) = word.strip_prefix('>') {
                (Op::Greater, rest)
            } else if let Some(rest) = word.strip_prefix('<') {
                (Op::Less, rest)
            } else {
                (Op::Equal, word.strip_prefix('=').unwrap_or(word))
            };
            comparators.push((op, Version::parse(rest)?));
        }
        if comparators.is_empty() && text.trim() != "*" {
            return None;
        }
        Some(Self(comparators))
    }

    /// Whether `version` satisfies every comparator.
    pub fn contains(&self, version: Version) -> bool {
        self.0.iter().all(|&(op, bound)| {
            let order = version.cmp(&bound);
            match op {
                Op::Greater => order == Ordering::Greater,
                Op::GreaterEqual => order != Ordering::Less,
                Op::Less => order == Ordering::Less,
                Op::LessEqual => order != Ordering::Greater,
                Op::Equal => order == Ordering::Equal,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn versions_have_one_to_three_numbers_and_order_by_them() {
        assert_eq!(v("0.2"), v("0.2.0"));
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1") > v("0.99.99"));
        for bad in ["", "1.2.3.4", "1.x", "v1", "1..2", "1.2-rc1", "-1"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_range_is_every_comparator_at_once() {
        let range = Range::parse(">=0.2, <1").unwrap();
        assert!(!range.contains(v("0.1.9")));
        assert!(range.contains(v("0.2.0")));
        assert!(range.contains(v("0.9.9")));
        assert!(!range.contains(v("1.0.0")));
        assert!(Range::parse(">0.2 <=0.4").unwrap().contains(v("0.4.0")));
        assert!(!Range::parse(">0.2").unwrap().contains(v("0.2.0")));
        assert!(Range::parse("=0.2.1").unwrap().contains(v("0.2.1")));
        assert!(Range::parse("0.2.1").unwrap().contains(v("0.2.1")));
        assert!(Range::parse("*").unwrap().contains(v("7.7.7")));
        for bad in ["", ">=", ">=x", "^1", "~1.2"] {
            assert_eq!(Range::parse(bad), None, "{bad}");
        }
    }
}
