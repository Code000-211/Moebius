//! Links in the wiki: `[[Page name]]` leads to a page, `WEB-12` to a card (and `#12`, in a project's page, to a card
//! of that project). Code, inline or fenced, holds no links. A link to a page names it by its path (`Team/On call`)
//! or by its name (`On call`); a name that several pages have goes to the one in the linking page's folder, then the
//! one of its project, then the first by path, and capitals do not matter.
//!
//! A page lists the pages and cards that link to it, and a card the pages that link to it. The links between pages
//! are kept in the store (`store::wiki_links`), made again from the files whenever the wiki changes; the cards'
//! links to pages are read from their text when asked, since a card's text changes without the wiki. Moving or
//! renaming a page rewrites the links to it in the other pages, in the commit of the move, so no link is left
//! behind in the files either.

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use super::pages::name_of;
use super::{PageInfo, Seen, Wiki, file_of, project_key};
use crate::app::{App, AppError, Result};
use crate::decide::Actor;
use crate::store::{WIKI_CARD, WIKI_PAGE, WikiLink, parse_card_key};

static PAGE_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[([^\[\]|\n]+)(?:\|[^\[\]\n]*)?\]\]").expect("a valid pattern")
});
static CARD_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)(?:^|[^\w/#&.\-])([A-Z][A-Z0-9]{1,8}-[0-9]+)\b").expect("a valid pattern")
});
static CARD_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)(?:^|[\s(])#([0-9]+)\b").expect("a valid pattern"));

/// A link to a page as written in a text: the name it gives and where that is in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRef {
    pub target: String,
    /// The bytes of the text that hold the target, without the spaces around it.
    pub span: Range<usize>,
}

/// The links a text makes.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Refs {
    pub pages: Vec<PageRef>,
    /// The cards' names, such as `WEB-12`, once each.
    pub cards: BTreeSet<String>,
}

/// `text` with every character of code, inline or fenced, made a space, so that offsets stay the same.
fn mask(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            blank(&mut out, line);
        } else if fenced {
            blank(&mut out, line);
        } else {
            mask_inline(&mut out, line);
        }
    }
    out
}

/// Appends `line` with everything but its line break made spaces.
fn blank(out: &mut String, line: &str) {
    for c in line.chars() {
        if c == '\n' {
            out.push('\n');
        } else {
            out.extend(std::iter::repeat_n(' ', c.len_utf8()));
        }
    }
}

/// Appends `line` with its inline code spans made spaces.
fn mask_inline(out: &mut String, line: &str) {
    let bytes = line.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'`' {
            let next = line[at..].find('`').map_or(bytes.len(), |i| at + i);
            out.push_str(&line[at..next]);
            at = next;
            continue;
        }
        let run = bytes[at..].iter().take_while(|b| **b == b'`').count();
        let ticks = "`".repeat(run);
        let after = at + run;
        let closing = line[after..].match_indices(&ticks).find(|(i, _)| {
            let end = after + i + run;
            bytes.get(after + i + run).is_none_or(|b| *b != b'`') && end <= bytes.len()
        });
        match closing {
            Some((i, _)) => {
                blank(out, &line[at..after + i + run]);
                at = after + i + run;
            }
            None => {
                out.push_str(&line[at..after]);
                at = after;
            }
        }
    }
}

/// The links `text` makes; `project` is the key of the project the page is in, which `#12` is a card of.
pub fn extract(text: &str, project: Option<&str>) -> Refs {
    let masked = mask(text);
    let mut refs = Refs::default();
    let mut rest = masked.clone().into_bytes();
    for found in PAGE_LINK.captures_iter(&masked) {
        let whole = found.get(0).expect("the whole match");
        let target = found.get(1).expect("the target");
        let trimmed = target.as_str().trim();
        if !trimmed.is_empty() {
            let start =
                target.start() + (target.as_str().len() - target.as_str().trim_start().len());
            refs.pages.push(PageRef {
                target: trimmed.to_string(),
                span: start..start + trimmed.len(),
            });
        }
        // What is inside the brackets is a page's name, not a card's.
        rest[whole.range()].fill(b' ');
    }
    let rest = String::from_utf8(rest).expect("only ASCII spaces were put over whole matches");
    for found in CARD_KEY.captures_iter(&rest) {
        let key = &found[1];
        if parse_card_key(key).is_some() {
            refs.cards.insert(key.to_string());
        }
    }
    if let Some(key) = project {
        for found in CARD_NUMBER.captures_iter(&rest) {
            refs.cards.insert(format!("{key}-{}", &found[1]));
        }
    }
    refs
}

/// The folder a page is in.
fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The page `target` names for a link written in the page `source`, among `paths`.
pub fn resolve(paths: &[String], source: &str, target: &str) -> Option<String> {
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let by_path: Vec<&String> = paths.iter().filter(|p| same(p, target)).collect();
    if let Some(exact) = by_path.iter().find(|p| p.as_str() == target) {
        return Some((*exact).clone());
    }
    if let Some(first) = by_path.first() {
        return Some((*first).clone());
    }
    let rank = |candidate: &str| {
        if folder(candidate) == folder(source) {
            0
        } else if project_key(candidate) == project_key(source) {
            1
        } else {
            2
        }
    };
    paths
        .iter()
        .filter(|p| same(name_of(p), target))
        .min_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)))
        .cloned()
}

/// `text` with the links that lead to the page `from` made to lead to `to`, which takes `from`'s place in `paths`;
/// none when no link changes. A link by path becomes the new path, a link by name the new name when that still leads
/// to the page.
pub fn retarget(
    text: &str,
    source: &str,
    paths: &[String],
    from: &str,
    to: &str,
) -> Option<String> {
    let after: Vec<String> = paths
        .iter()
        .map(|p| if p == from { to.to_string() } else { p.clone() })
        .collect();
    let mut out = text.to_string();
    let mut changed = false;
    let refs = extract(text, None);
    for link in refs.pages.iter().rev() {
        if resolve(paths, source, &link.target).as_deref() != Some(from) {
            continue;
        }
        let name = name_of(to);
        let by_name = !link.target.contains('/');
        if by_name
            && name.eq_ignore_ascii_case(&link.target)
            && resolve(&after, source, name).as_deref() == Some(to)
        {
            continue;
        }
        let replacement = if by_name && resolve(&after, source, name).as_deref() == Some(to) {
            name
        } else {
            to
        };
        if replacement != link.target {
            out.replace_range(link.span.clone(), replacement);
            changed = true;
        }
    }
    changed.then_some(out)
}

/// The text of the page `from` once it is at `to`, with its own links made to lead where they led: a link by name
/// leads by the folder of the page it is written in, so a page that moves may need its names written as paths, and a
/// link to the page itself follows it. `paths` are the pages before the move; none when no link changes.
pub fn retarget_moved(text: &str, paths: &[String], from: &str, to: &str) -> Option<String> {
    let after: Vec<String> = paths
        .iter()
        .map(|p| if p == from { to.to_string() } else { p.clone() })
        .collect();
    let mut out = text.to_string();
    let mut changed = false;
    for link in extract(text, None).pages.iter().rev() {
        let Some(old) = resolve(paths, from, &link.target) else {
            continue;
        };
        let want = if old == from { to.to_string() } else { old };
        if resolve(&after, to, &link.target).as_deref() == Some(want.as_str()) {
            continue;
        }
        let name = name_of(&want);
        let replacement = if !link.target.contains('/')
            && resolve(&after, to, name).as_deref() == Some(want.as_str())
        {
            name
        } else {
            want.as_str()
        };
        out.replace_range(link.span.clone(), replacement);
        changed = true;
    }
    changed.then_some(out)
}

/// What a page links to, as written.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PageLink {
    /// The name the link gives.
    pub link: String,
    /// The page it leads to; none when there is no such page.
    pub path: Option<String>,
}

/// A card as a link leads to it or comes from it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CardRef {
    /// The card's name, such as `WEB-12`.
    pub key: String,
    /// The card's title; none when there is no such card.
    pub title: Option<String>,
}

/// The pages and cards on one side of a link.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Links {
    pub pages: Vec<PageLink>,
    pub cards: Vec<CardRef>,
}

/// The pages and cards that link to a page.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Backlinks {
    pub pages: Vec<PageInfo>,
    pub cards: Vec<CardRef>,
}

/// What a page links to and what links to it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PageLinks {
    pub links: Links,
    pub backlinks: Backlinks,
}

impl Wiki {
    /// Makes the links between pages again from the files. The caller holds the turn.
    pub(super) fn reindex(&self, app: &App) -> Result<()> {
        let paths = self.page_paths()?;
        let mut links = Vec::new();
        for source in &paths {
            let Some(text) = self.read(source) else {
                continue;
            };
            let refs = extract(&text, project_key(source));
            for link in refs.pages {
                if let Some(target) = resolve(&paths, source, &link.target)
                    && &target != source
                {
                    links.push(WikiLink {
                        source: source.clone(),
                        kind: WIKI_PAGE,
                        target,
                    });
                }
            }
            for key in refs.cards {
                links.push(WikiLink {
                    source: source.clone(),
                    kind: WIKI_CARD,
                    target: key,
                });
            }
        }
        app.store.replace_wiki_links(&links)?;
        Ok(())
    }

    /// The text of the page `path`, if it is there.
    pub(super) fn read(&self, path: &str) -> Option<String> {
        let file = file_of(path).ok()?;
        std::fs::read_to_string(self.inside(&file).ok()?).ok()
    }

    /// The paths of every page. The caller holds the turn.
    fn page_paths(&self) -> Result<Vec<String>> {
        Ok(self.list()?.into_iter().map(|page| page.path).collect())
    }

    /// The files that hold links to the page `from`, and the page's own, and what they hold once those lead to `to`,
    /// for a move of the page. The caller holds the turn.
    pub(super) fn retargeted(&self, from: &str, to: &str) -> Result<Vec<(String, String)>> {
        let paths = self.page_paths()?;
        let mut files = Vec::new();
        for source in paths.iter().filter(|p| p.as_str() != from) {
            if let Some(text) = self.read(source)
                && let Some(text) = retarget(&text, source, &paths, from, to)
            {
                files.push((file_of(source)?, text));
            }
        }
        // The page's own links, written where it is now.
        if let Some(text) = self.read(from)
            && let Some(text) = retarget_moved(&text, &paths, from, to)
        {
            files.push((file_of(to)?, text));
        }
        Ok(files)
    }

    /// What the page `path` links to and what links to it: the pages and the cards, from their text, with every link
    /// resolved among the pages `who` sees.
    pub fn links(&self, app: &App, who: &Actor, path: &str) -> Result<PageLinks> {
        let _turn = self.turn();
        file_of(path)?;
        let seen = Seen::of(app, who)?;
        if !seen.page(app, path) {
            return Err(AppError::NotFound(format!("page {path}")));
        }
        // A link leads only among the pages the asker sees, as in a run's prompt: a page they do not see neither
        // answers a link of a page they do see, nor changes which page it leads to.
        let paths: Vec<String> = self
            .page_paths()?
            .into_iter()
            .filter(|page| seen.page(app, page))
            .collect();
        let refs = self
            .read(path)
            .map(|text| extract(&text, project_key(path)))
            .unwrap_or_default();
        let links = Links {
            pages: refs
                .pages
                .iter()
                .map(|link| PageLink {
                    link: link.target.clone(),
                    path: resolve(&paths, path, &link.target),
                })
                .collect(),
            cards: refs
                .cards
                .iter()
                .map(|key| card_ref(app, who, key))
                .collect::<Result<_>>()?,
        };
        // The same for the pages that link here: a link counts when it leads to this page among the pages the asker
        // sees, whatever the index, which resolves among every page, says.
        let mut pages: Vec<PageInfo> = Vec::new();
        for source in paths.iter().filter(|source| source.as_str() != path) {
            let Some(text) = self.read(source) else {
                continue;
            };
            let leads_here = extract(&text, project_key(source))
                .pages
                .iter()
                .any(|link| resolve(&paths, source, &link.target).as_deref() == Some(path));
            if leads_here {
                pages.push(self.info_of(source.clone()));
            }
        }
        let mut cards = Vec::new();
        for card in app.store.cards_with_wiki_links()? {
            if !app.may_see_project(who, card.project_id) {
                continue;
            }
            let scope = format!("{}/{}/.", super::PROJECTS, card.project_key);
            let leads_here = extract(&format!("{}\n{}", card.title, card.body), None)
                .pages
                .iter()
                .any(|link| resolve(&paths, &scope, &link.target).as_deref() == Some(path));
            if leads_here {
                cards.push(CardRef {
                    key: format!("{}-{}", card.project_key, card.project_number),
                    title: Some(card.title),
                });
            }
        }
        Ok(PageLinks {
            links,
            backlinks: Backlinks { pages, cards },
        })
    }

    /// The pages that link to the card `key`, such as `WEB-12`; a card that is not there is not found.
    pub fn pages_linking_card(&self, app: &App, who: &Actor, key: &str) -> Result<Vec<PageInfo>> {
        let _turn = self.turn();
        let (project, number) = parse_card_key(key)
            .ok_or_else(|| AppError::Invalid(format!("{key:?} is not a card's name")))?;
        match app.store.card_by_key(&project, number)? {
            Some(card) if app.may_see_project(who, card.project_id) => {}
            _ => return Err(AppError::NotFound(format!("card {key}"))),
        }
        let seen = Seen::of(app, who)?;
        Ok(app
            .store
            .wiki_links_to(WIKI_CARD, &format!("{project}-{number}"))?
            .into_iter()
            .filter(|source| seen.page(app, source))
            .map(|source| self.info_of(source))
            .collect())
    }
}

/// The card `key` as a link names it, with its title when there is such a card `who` may see.
fn card_ref(app: &App, who: &Actor, key: &str) -> Result<CardRef> {
    let card = match parse_card_key(key) {
        Some((project, number)) => app
            .store
            .card_by_key(&project, number)?
            .filter(|card| app.may_see_project(who, card.project_id)),
        None => None,
    };
    Ok(CardRef {
        key: key.to_string(),
        title: card.map(|card| card.title),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn a_text_links_to_pages_and_cards() {
        let text = "See [[On call]] and [[ Team/Runbook |the runbook]], WEB-12, (WEB-12) and #7.\n\
                    Not `WEB-3`, not [[Code]] in\n```\n[[Fenced]] WEB-4\n```\nnor in-WEB-5 or ABC-1x or [[WEB-9]].";
        let refs = extract(text, Some("API"));
        let targets: Vec<&str> = refs.pages.iter().map(|p| p.target.as_str()).collect();
        assert_eq!(targets, ["On call", "Team/Runbook", "Code", "WEB-9"]);
        assert_eq!(&text[refs.pages[1].span.clone()], "Team/Runbook");
        let cards: Vec<&str> = refs.cards.iter().map(String::as_str).collect();
        assert_eq!(cards, ["API-7", "WEB-12"]);
        assert!(extract("#7", None).cards.is_empty(), "no project, no #7");
        assert!(
            extract("# Heading\nissue#7 a#8", Some("API"))
                .cards
                .is_empty()
        );
    }

    #[test]
    fn code_holds_no_links() {
        let refs = extract("`[[a]]` ``WEB-1 ` [[b]]``\n~~~\n[[c]]\n~~~\n[[d]]", None);
        let targets: Vec<&str> = refs.pages.iter().map(|p| p.target.as_str()).collect();
        assert_eq!(targets, ["d"]);
        assert!(refs.cards.is_empty());
    }

    #[test]
    fn a_name_leads_to_the_nearest_page_of_it() {
        let all = paths(&[
            "Handbook",
            "Team/On call",
            "projects/WEB/On call",
            "projects/WEB/Runbook",
        ]);
        assert_eq!(
            resolve(&all, "x", "team/on call").as_deref(),
            Some("Team/On call")
        );
        assert_eq!(
            resolve(&all, "Other", "On call").as_deref(),
            Some("Team/On call")
        );
        assert_eq!(
            resolve(&all, "projects/WEB/Runbook", "On call").as_deref(),
            Some("projects/WEB/On call")
        );
        assert_eq!(
            resolve(&all, "Team/Notes", "On call").as_deref(),
            Some("Team/On call")
        );
        assert_eq!(resolve(&all, "x", "Nothing"), None);
    }

    #[test]
    fn a_page_that_moves_keeps_its_own_links() {
        let all = paths(&["Team/Runbook", "Team/Notes", "Other/Runbook"]);
        // A name led to the page of its own folder; from another folder it would lead elsewhere.
        assert_eq!(
            retarget_moved(
                "[[Runbook]] and [[Notes]]",
                &all,
                "Team/Notes",
                "Docs/Notes"
            )
            .as_deref(),
            Some("[[Team/Runbook]] and [[Notes]]")
        );
        // A link to the page itself follows it, by path or by name.
        assert_eq!(
            retarget_moved("[[Team/Notes]], [[notes]]", &all, "Team/Notes", "Team/Rota").as_deref(),
            Some("[[Team/Rota]], [[Rota]]")
        );
        assert_eq!(
            retarget_moved("[[Runbook]]", &all, "Team/Notes", "Team/Rota"),
            None
        );
        assert_eq!(
            retarget_moved("[[Nothing]]", &all, "Team/Notes", "Docs/Notes"),
            None
        );
    }

    #[test]
    fn a_move_changes_the_links_that_led_to_the_page() {
        let all = paths(&["Handbook", "Team/On call", "Team/Notes"]);
        let text = "[[On call]], [[Team/On call|label]], [[Handbook]] and [[on call]]";
        assert_eq!(
            retarget(text, "Team/Notes", &all, "Team/On call", "Team/Rota").as_deref(),
            Some("[[Rota]], [[Team/Rota|label]], [[Handbook]] and [[Rota]]")
        );
        // The name is still the page's in its new folder, so only a link by path changes.
        assert_eq!(
            retarget(text, "Team/Notes", &all, "Team/On call", "Rota/On call").as_deref(),
            Some("[[On call]], [[Rota/On call|label]], [[Handbook]] and [[on call]]")
        );
        assert_eq!(
            retarget("[[Handbook]]", "Team/Notes", &all, "Team/On call", "X"),
            None
        );
        // Another page of the new name would take the name, so the path is written.
        let taken = paths(&["Team/On call", "Other/Rota", "Notes"]);
        assert_eq!(
            retarget("[[On call]]", "Notes", &taken, "Team/On call", "Zed/Rota").as_deref(),
            Some("[[Zed/Rota]]")
        );
    }

    mod wiki {
        use std::path::Path;
        use std::sync::Arc;
        use std::time::Duration;

        use moebius_plugin_api::Host;

        use super::super::super::testing::{bare_origin, push_from_elsewhere};
        use super::super::*;
        use crate::wiki::Author;

        fn opened(dir: &Path) -> (App, Wiki, std::path::PathBuf) {
            let store = Arc::new(crate::store::Store::in_memory(dir).unwrap());
            let host = Arc::new(Host::new(Duration::from_secs(5)));
            let app = App::new(store, host, &dir.join("data"));
            let origin = bare_origin(dir);
            let wiki = Wiki::open(&app, &origin.to_string_lossy(), &dir.join("data/wiki")).unwrap();
            (app, wiki, origin)
        }

        fn ada() -> Author {
            Author {
                name: "Ada".into(),
                handle: "ada".into(),
            }
        }

        /// An owner, who sees every public project.
        fn seer(app: &App) -> Actor {
            let found = app.store.person_by_handle("root").unwrap();
            let owner = found.unwrap_or_else(|| {
                app.store
                    .add_person("root", "Root", crate::store::Role::Owner)
                    .unwrap()
            });
            Actor::person(owner.id, "root")
        }

        fn put(app: &App, wiki: &Wiki, path: &str, text: &str) {
            wiki.write_page(app, &ada(), path, text, None).unwrap();
        }

        fn paths(pages: &[PageInfo]) -> Vec<&str> {
            pages.iter().map(|p| p.path.as_str()).collect()
        }

        #[test]
        fn a_page_lists_the_pages_and_cards_that_link_to_it() {
            let dir = tempfile::tempdir().unwrap();
            let (app, wiki, _origin) = opened(dir.path());
            let repository = app
                .store
                .add_repository("web", "/srv/web", "none", "main")
                .unwrap();
            let card = app
                .store
                .add_card(
                    repository.id,
                    "Fix the cart",
                    "Runbook: [[On call]]",
                    "lea",
                    "Backlog",
                )
                .unwrap();
            app.store
                .add_card(
                    repository.id,
                    "Other",
                    "No link [[Nothing]]",
                    "lea",
                    "Backlog",
                )
                .unwrap();
            let key = format!("{}-{}", card.project_key, card.project_number);

            put(
                &app,
                &wiki,
                "Team/On call",
                "# On call\n\nSee [[Handbook]] and WEB-1.\n",
            );
            put(
                &app,
                &wiki,
                "Handbook",
                "Start with [[team/on call]] and [[Missing]].\n",
            );
            put(
                &app,
                &wiki,
                "projects/WEB/Cart",
                "Is #1 done? Back to [[Handbook]].\n",
            );

            let handbook = wiki.links(&app, &seer(&app), "Handbook").unwrap();
            assert_eq!(
                paths(&handbook.backlinks.pages),
                ["Team/On call", "projects/WEB/Cart"]
            );
            assert!(handbook.backlinks.cards.is_empty());
            let missing: Vec<(&str, Option<&str>)> = handbook
                .links
                .pages
                .iter()
                .map(|l| (l.link.as_str(), l.path.as_deref()))
                .collect();
            assert_eq!(
                missing,
                [("team/on call", Some("Team/On call")), ("Missing", None)]
            );

            let on_call = wiki.links(&app, &seer(&app), "Team/On call").unwrap();
            assert_eq!(paths(&on_call.backlinks.pages), ["Handbook"]);
            assert_eq!(on_call.backlinks.cards.len(), 1);
            assert_eq!(on_call.backlinks.cards[0].key, key);
            assert_eq!(
                on_call.backlinks.cards[0].title.as_deref(),
                Some("Fix the cart")
            );
            assert_eq!(on_call.links.cards[0].key, "WEB-1");
            assert_eq!(
                on_call.links.cards[0].title.as_deref(),
                Some("Fix the cart")
            );

            let linking = wiki.pages_linking_card(&app, &seer(&app), &key).unwrap();
            assert_eq!(paths(&linking), ["Team/On call", "projects/WEB/Cart"]);
            assert!(matches!(
                wiki.pages_linking_card(&app, &seer(&app), "WEB-99"),
                Err(AppError::NotFound(_))
            ));
            assert!(matches!(
                wiki.pages_linking_card(&app, &seer(&app), "nonsense"),
                Err(AppError::Invalid(_))
            ));
        }

        #[test]
        fn a_private_projects_pages_and_cards_are_left_out_for_those_who_are_not_members() {
            let dir = tempfile::tempdir().unwrap();
            let (app, wiki, _origin) = opened(dir.path());
            let repository = app
                .store
                .add_repository("web", "/srv/web", "none", "main")
                .unwrap();
            let card = app
                .store
                .add_card(
                    repository.id,
                    "Secret card",
                    "See [[Handbook]]",
                    "lea",
                    "Backlog",
                )
                .unwrap();
            let lea = app
                .store
                .add_person("lea", "Lea", crate::store::Role::Member)
                .unwrap();
            let olga = app
                .store
                .add_person("olga", "Olga", crate::store::Role::Member)
                .unwrap();
            app.store
                .set_project_access(card.project_id, true, &[lea.id])
                .unwrap();
            put(
                &app,
                &wiki,
                "Handbook",
                "See [[projects/WEB/Plan]] and WEB-1.\n",
            );
            put(
                &app,
                &wiki,
                "projects/WEB/Plan",
                "Back to [[Handbook]] and WEB-1.\n",
            );
            let (member, outsider) = (Actor::person(lea.id, "lea"), Actor::person(olga.id, "olga"));

            let seen = wiki.links(&app, &member, "Handbook").unwrap();
            assert_eq!(paths(&seen.backlinks.pages), ["projects/WEB/Plan"]);
            assert_eq!(seen.backlinks.cards.len(), 1);
            assert_eq!(
                seen.links.pages[0].path.as_deref(),
                Some("projects/WEB/Plan")
            );
            assert_eq!(seen.links.cards[0].title.as_deref(), Some("Secret card"));
            assert_eq!(
                wiki.pages_linking_card(&app, &member, "WEB-1")
                    .unwrap()
                    .len(),
                2
            );

            let hidden = wiki.links(&app, &outsider, "Handbook").unwrap();
            assert!(hidden.backlinks.pages.is_empty());
            assert!(hidden.backlinks.cards.is_empty());
            assert_eq!(hidden.links.pages[0].path, None);
            assert_eq!(hidden.links.cards[0].title, None);
            assert!(matches!(
                wiki.links(&app, &outsider, "projects/WEB/Plan"),
                Err(AppError::NotFound(_))
            ));
            assert!(matches!(
                wiki.pages_linking_card(&app, &outsider, "WEB-1"),
                Err(AppError::NotFound(_))
            ));
        }

        #[test]
        fn a_link_leads_among_the_pages_the_asker_sees() {
            let dir = tempfile::tempdir().unwrap();
            let (app, wiki, _origin) = opened(dir.path());
            let web = app
                .store
                .add_repository("web", "/srv/web", "none", "main")
                .unwrap();
            let secret = app
                .store
                .add_repository("secret", "/srv/secret", "none", "main")
                .unwrap();
            let key = app
                .store
                .project(secret.home_project_id)
                .unwrap()
                .unwrap()
                .key;
            let lea = app
                .store
                .add_person("lea", "Lea", crate::store::Role::Member)
                .unwrap();
            let olga = app
                .store
                .add_person("olga", "Olga", crate::store::Role::Member)
                .unwrap();
            app.store
                .set_project_access(secret.home_project_id, true, &[lea.id])
                .unwrap();
            app.store
                .add_card(web.id, "Plan it", "See [[Plans]]", "lea", "Backlog")
                .unwrap();
            // Both pages are called Plans, and for the card of another project the one in the private project comes
            // first by path.
            put(&app, &wiki, &format!("projects/{key}/Plans"), "Secret.\n");
            put(&app, &wiki, "zeta/Plans", "Public.\n");
            let cards = |who: &Actor, page: &str| {
                wiki.links(&app, who, page).unwrap().backlinks.cards.len()
            };
            let (member, outsider) = (Actor::person(lea.id, "lea"), Actor::person(olga.id, "olga"));
            assert_eq!(cards(&member, &format!("projects/{key}/Plans")), 1);
            assert_eq!(cards(&member, "zeta/Plans"), 0);
            // For a person who does not see the private page, the card's link leads to the page they see.
            assert_eq!(cards(&outsider, "zeta/Plans"), 1);
        }

        #[test]
        fn backlinks_lead_among_the_pages_the_asker_sees() {
            let dir = tempfile::tempdir().unwrap();
            let (app, wiki, _origin) = opened(dir.path());
            let secret = app
                .store
                .add_repository("secret", "/srv/secret", "none", "main")
                .unwrap();
            let key = app
                .store
                .project(secret.home_project_id)
                .unwrap()
                .unwrap()
                .key;
            let open = app
                .store
                .add_repository("zed", "/srv/zed", "none", "main")
                .unwrap();
            let open_key = app
                .store
                .project(open.home_project_id)
                .unwrap()
                .unwrap()
                .key;
            let lea = app
                .store
                .add_person("lea", "Lea", crate::store::Role::Member)
                .unwrap();
            let olga = app
                .store
                .add_person("olga", "Olga", crate::store::Role::Member)
                .unwrap();
            app.store
                .set_project_access(secret.home_project_id, true, &[lea.id])
                .unwrap();
            // Both Plans are in a project other than the linking page's, so [[Plans]] goes to the first by path
            // among every page, the private one, in the index; the outsider sees only the other.
            put(&app, &wiki, &format!("projects/{key}/Plans"), "Secret.\n");
            let public = format!("projects/{open_key}/Plans");
            put(&app, &wiki, &public, "Public.\n");
            put(&app, &wiki, "a/Notes", "See [[Plans]].\n");
            let backlinks = |who: &Actor, page: &str| {
                let pages = wiki.links(&app, who, page).unwrap().backlinks.pages;
                paths(&pages)
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            };
            let (member, outsider) = (Actor::person(lea.id, "lea"), Actor::person(olga.id, "olga"));
            assert_eq!(
                backlinks(&member, &format!("projects/{key}/Plans")),
                ["a/Notes"]
            );
            assert!(backlinks(&member, &public).is_empty());
            assert_eq!(backlinks(&outsider, &public), ["a/Notes"]);
        }

        #[test]
        fn a_rename_keeps_every_link_to_the_page_working() {
            let dir = tempfile::tempdir().unwrap();
            let (app, wiki, _origin) = opened(dir.path());
            put(&app, &wiki, "Team/On call", "# On call\n");
            put(
                &app,
                &wiki,
                "Handbook",
                "[[On call]] and [[Team/On call|the rota]]\n",
            );
            put(&app, &wiki, "Unrelated", "Nothing here\n");
            let before = wiki.history("Unrelated").unwrap().len();

            wiki.move_page(&app, &ada(), "Team/On call", "Team/Rota")
                .unwrap();
            let handbook = wiki.page("Handbook", None).unwrap();
            assert_eq!(handbook.text, "[[Rota]] and [[Team/Rota|the rota]]\n");
            assert!(handbook.version.message.starts_with("Rename Team/On call"));
            assert_eq!(wiki.history("Unrelated").unwrap().len(), before);
            assert_eq!(
                paths(
                    &wiki
                        .links(&app, &seer(&app), "Team/Rota")
                        .unwrap()
                        .backlinks
                        .pages
                ),
                ["Handbook"]
            );
            assert!(
                wiki.links(&app, &seer(&app), "Team/On call")
                    .unwrap()
                    .backlinks
                    .pages
                    .is_empty()
            );

            // The page's own links are written where it is now.
            put(
                &app,
                &wiki,
                "Team/Self",
                "Me: [[Team/Self]] and [[Handbook]]\n",
            );
            wiki.move_page(&app, &ada(), "Team/Self", "Team/Me")
                .unwrap();
            assert_eq!(
                wiki.page("Team/Me", None).unwrap().text,
                "Me: [[Team/Me]] and [[Handbook]]\n"
            );

            // Another folder; a link by name still leads to the page there.
            wiki.move_page(&app, &ada(), "Team/Rota", "Docs/Rota")
                .unwrap();
            assert_eq!(
                wiki.page("Handbook", None).unwrap().text,
                "[[Rota]] and [[Docs/Rota|the rota]]\n"
            );
            assert_eq!(
                paths(
                    &wiki
                        .links(&app, &seer(&app), "Docs/Rota")
                        .unwrap()
                        .backlinks
                        .pages
                ),
                ["Handbook"]
            );
        }

        #[test]
        fn links_made_elsewhere_are_indexed_after_a_refresh() {
            let dir = tempfile::tempdir().unwrap();
            let (app, wiki, origin) = opened(dir.path());
            put(&app, &wiki, "Target", "Here\n");
            push_from_elsewhere(dir.path(), &origin, "Source.md", "Go to [[Target]]\n");
            assert!(
                wiki.links(&app, &seer(&app), "Target")
                    .unwrap()
                    .backlinks
                    .pages
                    .is_empty()
            );
            wiki.refresh(&app).unwrap();
            assert_eq!(
                paths(
                    &wiki
                        .links(&app, &seer(&app), "Target")
                        .unwrap()
                        .backlinks
                        .pages
                ),
                ["Source"]
            );

            // The index is made again from the clone, as when the server starts.
            app.store.replace_wiki_links(&[]).unwrap();
            wiki.index(&app).unwrap();
            assert_eq!(
                paths(
                    &wiki
                        .links(&app, &seer(&app), "Target")
                        .unwrap()
                        .backlinks
                        .pages
                ),
                ["Source"]
            );

            wiki.delete_page(&app, &ada(), "Source").unwrap();
            assert!(
                wiki.links(&app, &seer(&app), "Target")
                    .unwrap()
                    .backlinks
                    .pages
                    .is_empty()
            );
        }
    }
}
