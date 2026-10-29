//! The fuzzy match of "Go to file": the characters typed must occur in a path in that order, and the paths rank as an
//! editor ranks them. Characters that follow each other score more, and so do those that start a word (after `/`,
//! `_`, `-`, `.` or a space, or a capital after a lowercase letter) or fall in the file's name rather than its
//! directories. A gap between two matches costs a point per character skipped, and a shorter path wins a tie. Two
//! more ideas come from the file pickers of the Pi ecosystem (FFF, oh-my-pi): a capital typed must match a capital,
//! and a file named exactly as typed wins; and the files opened lately rank first (the most recent most), as do
//! those in the directories of the file shown, which stands in for the frecency and directory distance of those tools.

/// What a matched character is worth by itself.
const MATCH: i64 = 1;

/// Extra for a match directly after the one before it.
const CONSECUTIVE: i64 = 12;

/// Extra for a match that starts a word.
const WORD_START: i64 = 16;

/// Extra for a match in the file's name, after the last `/`.
const IN_NAME: i64 = 6;

/// Extra for a typed capital that matches a capital, when the query has any.
const CASE: i64 = 4;

/// Extra for a file whose name, or whose name without its extension, is the query.
const EXACT_NAME: i64 = 30;

/// What the most recently opened file adds; each older one adds 2 less, down to nothing.
const RECENT: i64 = 24;

/// What each directory shared with the file shown adds, at most [`NEAR_MAX`].
const NEAR: i64 = 2;
const NEAR_MAX: i64 = 8;

/// Below every real score, for a character that cannot be matched there.
const NONE: i64 = i64::MIN / 2;

/// How well `query` matches `path`: `None` when its characters do not occur in order, ignoring case and blanks. A
/// blank query matches everything with 0. Higher is better.
pub fn score(query: &str, path: &str) -> Option<i64> {
    let typed: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
    let wanted: Vec<char> = typed.iter().flat_map(|c| c.to_lowercase()).collect();
    if wanted.is_empty() {
        return Some(0);
    }
    // A character that lowercases to several cannot be told apart by case; the check is then left out.
    let cased = typed.len() == wanted.len() && typed.iter().any(|c| c.is_uppercase());
    let chars: Vec<char> = path.chars().collect();
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let name = chars.iter().rposition(|c| *c == '/').map_or(0, |i| i + 1);
    let worth = |j: usize| -> i64 {
        let boundary = j == 0
            || matches!(chars[j - 1], '/' | '_' | '-' | '.' | ' ')
            || (chars[j - 1].is_lowercase() && chars[j].is_uppercase());
        MATCH + if boundary { WORD_START } else { 0 } + if j >= name { IN_NAME } else { 0 }
    };
    // `before[j]` is the best score of the characters matched so far with the last one at `j`.
    let mut before = vec![NONE; chars.len()];
    for (i, want) in wanted.iter().enumerate() {
        let mut now = vec![NONE; chars.len()];
        // The best `before[k] + k` over the `k` that leave a gap before `j`.
        let mut apart = NONE;
        for j in 0..chars.len() {
            if j >= 2 && before[j - 2] > NONE {
                apart = apart.max(before[j - 2] + (j - 2) as i64);
            }
            if lower[j] != *want {
                continue;
            }
            let worth = |j| {
                worth(j)
                    + if cased && chars[j] == typed[i] {
                        CASE
                    } else {
                        0
                    }
            };
            now[j] = if i == 0 {
                worth(j)
            } else {
                let next = (j >= 1 && before[j - 1] > NONE).then(|| before[j - 1] + CONSECUTIVE);
                let gap = (apart > NONE).then(|| apart - (j as i64 - 1));
                match next.max(gap) {
                    Some(best) => best + worth(j),
                    None => NONE,
                }
            };
        }
        before = now;
    }
    let best = before.into_iter().max().filter(|best| *best > NONE)?;
    let file: String = lower[name..].iter().collect();
    let query: String = wanted.iter().collect();
    let exact = file == query || file.rsplit_once('.').is_some_and(|(stem, _)| stem == query);
    Some(best - chars.len() as i64 / 8 + if exact { EXACT_NAME } else { 0 })
}

/// What a path is worth besides its match: being among the files opened lately (`recent`, the latest first), and
/// sharing directories with `current`, the file shown.
fn standing(path: &str, recent: &[String], current: Option<&str>) -> i64 {
    let lately = recent
        .iter()
        .position(|other| other == path)
        .map_or(0, |i| (RECENT - 2 * i as i64).max(0));
    let near = current.map_or(0, |current| {
        let shared = path
            .split('/')
            .zip(current.split('/'))
            .take_while(|(a, b)| a == b)
            .count()
            .saturating_sub(usize::from(path == current));
        let dirs = path.matches('/').count().min(shared);
        (dirs as i64 * NEAR).min(NEAR_MAX)
    });
    lately + near
}

/// The `limit` best matches of `query` among `paths`, best first; equal scores put the shorter path first, then the
/// one that sorts first. `recent` and `current` are as for [`standing`]; with a blank query they decide the order.
pub fn rank<'a>(
    query: &str,
    paths: &'a [String],
    limit: usize,
    recent: &[String],
    current: Option<&str>,
) -> Vec<&'a str> {
    let mut scored: Vec<(i64, &str)> = paths
        .iter()
        .filter_map(|path| {
            Some((
                score(query, path)? + standing(path, recent, current),
                path.as_str(),
            ))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.len().cmp(&b.1.len()))
            .then(a.1.cmp(b.1))
    });
    scored
        .into_iter()
        .take(limit)
        .map(|(_, path)| path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn the_characters_must_occur_in_order_ignoring_case() {
        assert!(score("smr", "src/main.rs").is_some());
        assert!(score("MAIN", "src/main.rs").is_some());
        assert!(score("s m", "src/main.rs").is_some());
        assert_eq!(score("", "src/main.rs"), Some(0));
        assert_eq!(score("rsm", "src/main.rs"), None);
        assert_eq!(score("mainx", "src/main.rs"), None);
    }

    #[test]
    fn consecutive_characters_rank_before_scattered_ones() {
        let all = paths(&["lib/zazbzcz.rs", "lib/zabcz.rs", "lib/zabzcz.rs"]);
        assert_eq!(
            rank("abc", &all, 10, &[], None),
            ["lib/zabcz.rs", "lib/zabzcz.rs", "lib/zazbzcz.rs"]
        );
    }

    #[test]
    fn word_starts_rank_before_characters_inside_a_word() {
        let all = paths(&["src/foxbox.rs", "src/foo_bar.rs", "src/fixbar.rs"]);
        let ranked = rank("fb", &all, 10, &[], None);
        assert_eq!(ranked[0], "src/foo_bar.rs");
        // A capital after a lowercase letter starts a word too.
        let all = paths(&["src/fixbar.rs", "src/FooBar.rs"]);
        assert_eq!(rank("fb", &all, 10, &[], None)[0], "src/FooBar.rs");
    }

    #[test]
    fn a_match_in_the_name_beats_one_in_the_directories() {
        let all = paths(&[
            "server/api/mod.rs",
            "docs/server.md",
            "crates/moebius-core/src/server.rs",
        ]);
        assert_eq!(rank("server", &all, 10, &[], None)[0], "docs/server.md");
        let all = paths(&["main/other.rs", "src/main.rs"]);
        assert_eq!(rank("main", &all, 10, &[], None)[0], "src/main.rs");
    }

    #[test]
    fn ties_go_to_the_shorter_path_and_the_list_is_cut() {
        let all = paths(&["b/readme.md", "a/readme.md", "readme.md"]);
        assert_eq!(
            rank("readme", &all, 10, &[], None),
            ["readme.md", "a/readme.md", "b/readme.md"]
        );
        assert_eq!(rank("readme", &all, 2, &[], None).len(), 2);
        assert!(rank("zzz", &all, 10, &[], None).is_empty());
    }

    #[test]
    fn a_typed_capital_and_an_exact_name_rank_first() {
        let all = paths(&["src/fooBar.rs", "src/FooBar.rs", "src/foobar.rs"]);
        assert_eq!(rank("FB", &all, 10, &[], None)[0], "src/FooBar.rs");
        let all = paths(&["src/main_loop.rs", "src/domain.rs", "src/main.rs"]);
        assert_eq!(rank("main", &all, 10, &[], None)[0], "src/main.rs");
    }

    #[test]
    fn recent_files_and_near_ones_rank_first() {
        let all = paths(&["a/util.rs", "b/util.rs", "c/util.rs"]);
        let recent = paths(&["c/util.rs", "b/util.rs"]);
        assert_eq!(
            rank("util", &all, 10, &recent, None),
            ["c/util.rs", "b/util.rs", "a/util.rs"]
        );
        // Without a query the recent files come first, then the rest by length.
        assert_eq!(
            rank("", &all, 10, &recent, None)[..2],
            ["c/util.rs", "b/util.rs"]
        );
        // The directory of the file shown breaks a tie.
        assert_eq!(
            rank("util", &all, 10, &[], Some("b/other.rs"))[0],
            "b/util.rs"
        );
        // A match still beats a recent file that does not match.
        assert_eq!(rank("zzz", &all, 10, &recent, None).len(), 0);
    }
}
