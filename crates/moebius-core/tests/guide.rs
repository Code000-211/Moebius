//! The guide in `docs/guide/` works. Every fenced block that claims to be something is checked:
//!
//! - ```` ```luau ```` blocks are complete column scripts. Each has a marker line right before it,
//!   `<!-- script: ready.luau -->`, naming the script it replaces. The test commits it as that repository override and
//!   runs it against a card in that script's column; it must not fail. A marker `<!-- script: ready.luau; test: key -->`
//!   also runs the scenario `key` below, which checks what the script does.
//! - ```` ```toml ```` blocks are `moebius.toml` snippets and must pass `Config::parse`, unless marked `<!-- skip -->`.
//!   Blocks marked `<!-- agent-file -->` are `agents/<name>.toml` files of the harness configuration repository and must
//!   parse as the settings of an agent.
//! - ```` ```rules ```` blocks, and ```` ```markdown ```` blocks marked `<!-- rules -->`, are `rules.md` files and must
//!   give at least one rule.
//! - Rust and TypeScript blocks marked `<!-- excerpt: path -->` must be lines of that file, in order, ignoring
//!   indentation.
//! - `curl` commands in ```` ```sh ```` blocks must name a route of the desktop API with the method they use.
//! - Links to files in the repository must point at files that exist.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use moebius_core::app::{App, PERSON};
use moebius_core::board::{Engine, Limits, Runs, Step, Workflow};
use moebius_core::config::Config;
use moebius_core::harness_repo::AgentSettings;
use moebius_core::repos::git;
use moebius_core::rules;
use moebius_core::store::{Card, Run, Store};
use moebius_plugin_api::{
    Call, DecideRequest, Decision, Host, Kind, Manifest, Plugin, PluginError,
};
use serde_json::Value;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> String {
    let file = repository_root().join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

/// The pages of the guide, as file name and text.
fn pages() -> Vec<(String, String)> {
    let mut pages = Vec::new();
    for entry in std::fs::read_dir(repository_root().join("docs/guide")).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if name.ends_with(".md") {
            let text = read(&format!("docs/guide/{name}"));
            pages.push((name, text));
        }
    }
    pages.sort();
    pages
}

/// One fenced block of a page.
struct Block {
    page: String,
    /// The line of the opening fence, from 1.
    line: usize,
    language: String,
    /// The comment `<!-- ... -->` on the line right before the fence, without its delimiters.
    marker: Option<String>,
    text: String,
}

impl Block {
    fn name(&self) -> String {
        format!("{}:{}", self.page, self.line)
    }
}

fn blocks(page: &str, text: &str) -> Vec<Block> {
    let mut found = Vec::new();
    let mut open: Option<Block> = None;
    let mut marker = None;
    for (index, line) in text.lines().enumerate() {
        if let Some(block) = open.as_mut() {
            if line.trim_end() == "```" {
                found.extend(open.take());
            } else {
                block.text.push_str(line);
                block.text.push('\n');
            }
        } else if let Some(language) = line.strip_prefix("```") {
            open = Some(Block {
                page: page.into(),
                line: index + 1,
                language: language.trim().into(),
                marker: marker.take(),
                text: String::new(),
            });
        } else {
            marker = line
                .trim()
                .strip_prefix("<!--")
                .and_then(|rest| rest.strip_suffix("-->"))
                .map(|inner| inner.trim().to_string());
        }
    }
    assert!(open.is_none(), "{page} has a fenced block that never ends");
    found
}

fn all_blocks(language: &str) -> Vec<Block> {
    pages()
        .iter()
        .flat_map(|(page, text)| blocks(page, text))
        .filter(|block| block.language == language)
        .collect()
}

// ---- The pages -----------------------------------------------------------------------------------------------

const PAGES: [&str; 12] = [
    "README.md",
    "column-scripts.md",
    "workflows.md",
    "rules.md",
    "agents.md",
    "plugins.md",
    "extensions.md",
    "desktop-api.md",
    "command-line.md",
    "autoresearch.md",
    "grilling.md",
    "running-moebius.md",
];

#[test]
fn the_guide_has_its_pages_and_the_readme_links_to_each() {
    let names: Vec<String> = pages().into_iter().map(|(name, _)| name).collect();
    for page in PAGES {
        assert!(names.iter().any(|name| name == page), "{page} is missing");
    }
    let readme = read("docs/guide/README.md");
    for page in PAGES.iter().skip(1) {
        assert!(
            readme.contains(&format!("]({page})")),
            "docs/guide/README.md does not link to {page}"
        );
    }
    for path in ["README.md", "CLAUDE.md"] {
        assert!(
            read(path).contains("docs/guide/"),
            "{path} does not mention the guide"
        );
    }
}

#[test]
fn links_to_files_point_at_files_that_exist() {
    let mut checked = 0;
    for (page, text) in pages() {
        for part in text.split("](").skip(1) {
            let Some(target) = part.split(')').next() else {
                continue;
            };
            if target.starts_with("http") || target.starts_with('#') {
                continue;
            }
            let path = target.split('#').next().unwrap();
            let file = repository_root().join("docs/guide").join(path);
            assert!(file.exists(), "{page} links to {target}, which is missing");
            checked += 1;
        }
    }
    assert!(checked > 20, "only {checked} links found");
}

// ---- Configuration, rules and excerpts -----------------------------------------------------------------------

#[test]
fn every_toml_block_is_a_configuration_unless_it_says_skip() {
    let mut parsed = 0;
    let mut agent_files = 0;
    for block in all_blocks("toml") {
        match block.marker.as_deref() {
            Some("skip") => continue,
            Some("agent-file") => {
                toml::from_str::<AgentSettings>(&block.text)
                    .unwrap_or_else(|e| panic!("{}: {e}", block.name()));
                agent_files += 1;
                continue;
            }
            None => {}
            Some(other) => panic!("{}: unknown marker {other:?}", block.name()),
        }
        Config::parse(
            &block.text,
            Path::new("/srv/moebius"),
            Path::new("moebius.toml"),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", block.name()));
        parsed += 1;
    }
    assert!(parsed >= 4, "only {parsed} configurations found");
    assert!(agent_files >= 2, "only {agent_files} agent files found");
}

#[test]
fn every_rules_block_gives_rules() {
    let mut parsed = 0;
    let rules_blocks = all_blocks("rules").into_iter().chain(
        all_blocks("markdown")
            .into_iter()
            .filter(|block| block.marker.as_deref() == Some("rules")),
    );
    for block in rules_blocks {
        let rules = rules::parse(&block.text);
        assert!(!rules.is_empty(), "{}: no rule found", block.name());
        parsed += 1;
    }
    assert!(parsed >= 2, "only {parsed} rules files found");
}

/// Whether the lines of `excerpt` appear in `file` one after the other, ignoring indentation and blank lines.
fn is_excerpt(excerpt: &str, file: &str) -> bool {
    let wanted: Vec<&str> = excerpt
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let have: Vec<&str> = file
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    !wanted.is_empty() && have.windows(wanted.len()).any(|window| window == wanted)
}

#[test]
fn excerpts_are_lines_of_the_files_they_name() {
    let mut checked = 0;
    for (page, text) in pages() {
        for block in blocks(&page, &text) {
            let Some(path) = block
                .marker
                .as_deref()
                .and_then(|m| m.strip_prefix("excerpt:"))
            else {
                continue;
            };
            let path = path.trim();
            assert!(
                is_excerpt(&block.text, &read(path)),
                "{} is not an excerpt of {path}",
                block.name()
            );
            checked += 1;
        }
    }
    assert!(checked >= 4, "only {checked} excerpts found");
}

#[test]
fn unmarked_rust_and_typescript_blocks_do_not_exist() {
    // A block of code in the guide is either checked or clearly not code to copy.
    for language in ["rust", "ts", "typescript"] {
        for block in all_blocks(language) {
            let marker = block.marker.as_deref().unwrap_or("");
            assert!(
                marker.starts_with("excerpt:") || marker == "skip",
                "{} needs an excerpt marker (or skip)",
                block.name()
            );
        }
    }
}

// ---- curl commands ---------------------------------------------------------------------------------------------

/// The routes of the desktop API as written in `api.rs`: path and the methods it answers.
fn routes() -> Vec<(String, BTreeSet<&'static str>)> {
    let source = read("crates/moebius-core/src/api.rs");
    let source = source.split("#[cfg(test)]").next().unwrap();
    source
        .split(".route(")
        .skip(1)
        .filter_map(|rest| {
            let rest = rest.split(".layer(").next().unwrap();
            let path = rest.split('"').nth(1)?.to_string();
            let end = rest.find("\n        .").unwrap_or(rest.len());
            let handlers = &rest[..end];
            let mut methods = BTreeSet::new();
            if handlers.contains("get(") {
                methods.insert("GET");
            }
            if handlers.contains("post(") {
                methods.insert("POST");
            }
            if handlers.contains("put(") {
                methods.insert("PUT");
            }
            if handlers.contains("delete(") {
                methods.insert("DELETE");
            }
            Some((path, methods))
        })
        .collect()
}

#[test]
fn curl_commands_name_routes_that_exist() {
    let routes = routes();
    assert!(routes.len() >= 18, "{routes:?}");
    let mut checked = 0;
    for block in all_blocks("sh") {
        let text = block.text.replace("\\\n", " ");
        for command in text.lines().filter(|l| l.contains("curl")) {
            let Some(after) = command.split("$API").nth(1) else {
                continue;
            };
            let url = after
                .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ')'))
                .next()
                .unwrap();
            let path = url.split('?').next().unwrap();
            let method = ["PUT", "DELETE"]
                .into_iter()
                .find(|method| command.contains(&format!("-X {method}")))
                .unwrap_or(if command.contains("-X POST") || command.contains(" -d ") {
                    "POST"
                } else {
                    "GET"
                });
            // A segment of the route in braces stands for anything.
            let route = routes.iter().find(|(route, _)| {
                let (route, given): (Vec<_>, Vec<_>) =
                    (route.split('/').collect(), path.split('/').collect());
                route.len() == given.len()
                    && route
                        .iter()
                        .zip(&given)
                        .all(|(r, g)| r == g || (r.starts_with('{') && !g.is_empty()))
            });
            let Some((_, methods)) = route else {
                panic!("{}: {path} is not a route", block.name());
            };
            assert!(
                methods.contains(method),
                "{}: {path} does not answer {method}",
                block.name()
            );
            checked += 1;
        }
    }
    assert!(checked >= 15, "only {checked} curl commands found");
}

// ---- Column scripts ----------------------------------------------------------------------------------------------

/// What `moebius.start_run` started.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Started {
    agent: String,
    purpose: String,
    note: Option<String>,
}

/// A harness whose runs never end, with the agents it was given.
struct Harness {
    agents: Vec<String>,
    started: Mutex<Vec<Started>>,
}

impl Runs for Harness {
    fn start(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        purpose: &str,
        note: Option<&str>,
    ) -> Result<Run, String> {
        self.started.lock().unwrap().push(Started {
            agent: agent.into(),
            purpose: purpose.into(),
            note: note.map(String::from),
        });
        app.store
            .add_run(Some(card.id), agent, "idle", purpose)
            .map_err(|e| e.to_string())
    }

    fn has_agent(&self, name: &str) -> bool {
        self.agents.iter().any(|agent| agent == name)
    }

    fn steer(&self, _: i64, _: &str) -> bool {
        false
    }

    fn stop(&self, _: i64) -> bool {
        false
    }
}

/// Decides `start_card`: a card whose title says `[wip]` is held.
struct StartGate;

impl Plugin for StartGate {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "start-gate".into(),
            version: "0.1.0".into(),
            kinds: vec![Kind::Decider],
        }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        if call.method != "decide" {
            return Err(PluginError::Failed(format!("no method {:?}", call.method)));
        }
        let request: DecideRequest = serde_json::from_value(call.params)
            .map_err(|e| PluginError::BadParams(e.to_string()))?;
        let title = request.context.get("title").map_or("", String::as_str);
        let (choice, reason) = if title.contains("[wip]") {
            ("hold", "the title says [wip]")
        } else {
            ("start", "the title does not say [wip]")
        };
        let decision = Decision {
            choice: choice.into(),
            reason: reason.into(),
            by: Some("title".into()),
        };
        serde_json::to_value(decision).map_err(|e| PluginError::Failed(e.to_string()))
    }
}

/// A repository whose default branch carries `source` as `.moebius/<script>`, and an engine over it.
struct Fixture {
    _dir: tempfile::TempDir,
    app: Arc<App>,
    engine: Engine,
    repository: i64,
    harness: Arc<Harness>,
}

impl Fixture {
    fn new(script: &str, source: &str, agents: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(origin.join(".moebius")).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join(".moebius").join(script), source).unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "scripts"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let mut host = Host::new(Duration::from_secs(5));
        host.add(StartGate);
        let app = App::new(store, Arc::new(host), &dir.path().join("data"));
        app.decisions.route("start_card", "start-gate");
        let app = Arc::new(app);
        let repository = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap()
            .id;
        let harness = Arc::new(Harness {
            agents: agents.iter().map(|a| a.to_string()).collect(),
            started: Mutex::default(),
        });
        let limits = Limits {
            run_limit: 3,
            ..Limits::default()
        };
        let engine = Engine::new(app.clone(), harness.clone(), Workflow::default(), limits);
        Self {
            _dir: dir,
            app,
            engine,
            repository,
            harness,
        }
    }

    /// A new card in `column`.
    fn card(&self, column: &str, title: &str, author: &str) -> Card {
        let card = self
            .app
            .create_issue(self.repository, title, "", author)
            .unwrap();
        if column == "Backlog" {
            card
        } else {
            self.app.move_card(card.id, column).unwrap()
        }
    }

    fn column(&self, card: &Card) -> String {
        self.app.card(card.id).unwrap().column
    }

    fn tick(&self, card: &Card) -> Step {
        self.engine.tick(card.id).unwrap()
    }

    /// A run of the card that has ended with `status`.
    fn ended_run(&self, card: &Card, purpose: &str, status: &str) {
        let run = self
            .app
            .store
            .add_run(Some(card.id), "Coder", "idle", purpose)
            .unwrap();
        self.app.store.set_run_status(run.id, status).unwrap();
    }

    /// The kinds and reasons of the card's open inbox items.
    fn inbox(&self, card: &Card) -> Vec<(String, String)> {
        self.app
            .store
            .inbox()
            .unwrap()
            .into_iter()
            .filter(|item| item.card_id == Some(card.id))
            .map(|item| (item.kind, item.data["reason"].as_str().unwrap().into()))
            .collect()
    }
}

type Scenario = fn(&Fixture);

/// What the scripts of the guide do, by the key in their marker.
fn scenarios() -> Vec<(&'static str, Scenario)> {
    vec![
        ("large-cards-alone", large_cards_alone),
        ("ask-about-authors", ask_about_authors),
        ("urgent-skips-blockers", urgent_skips_blockers),
        ("two-rounds", two_rounds),
        ("agent-by-title", agent_by_title),
        ("decide-with-a-plugin", decide_with_a_plugin),
    ]
}

fn large_cards_alone(f: &Fixture) {
    // One run is going, and the limit is 3.
    let busy = f.card("Backlog", "Busy", PERSON);
    f.app
        .store
        .add_run(Some(busy.id), "Coder", "idle", "code")
        .unwrap();

    let ordinary = f.card("Ready", "A small fix", PERSON);
    assert_eq!(f.tick(&ordinary).moved.as_deref(), Some("In progress"));

    let large = f.card("Ready", "[large] Rewrite the parser", PERSON);
    let step = f.tick(&large);
    assert_eq!(f.column(&large), "Ready");
    assert_eq!(
        step.waiting.as_deref(),
        Some("a [large] card starts when no other run is going")
    );

    for run in f.app.store.active_runs().unwrap() {
        f.app.store.set_run_status(run.id, "succeeded").unwrap();
    }
    f.tick(&large);
    assert_eq!(f.column(&large), "In progress");
}

fn ask_about_authors(f: &Fixture) {
    // A person's issue is trusted and goes on.
    let own = f.card("Backlog", "Mine", PERSON);
    assert_eq!(f.tick(&own).moved.as_deref(), Some("Ready"));

    // An author on the list waits for a person, with the reason in the inbox and as the card's waiting reason.
    let listed = f.card("Backlog", "Bump a dependency", "dependabot[bot]");
    let step = f.tick(&listed);
    assert_eq!(f.column(&listed), "Backlog");
    let reason = "a person reads the issues of dependabot[bot] first";
    assert_eq!(step.waiting.as_deref(), Some(reason));
    assert_eq!(
        f.inbox(&listed),
        [("approve_issue".to_string(), reason.to_string())]
    );
    // Ticking again adds nothing.
    f.tick(&listed);
    assert_eq!(f.inbox(&listed).len(), 1);

    // Someone else's issue waits too, and a person's approval moves either on.
    let other = f.card("Backlog", "From outside", "stranger");
    f.tick(&other);
    assert_eq!(f.inbox(&other).len(), 1);
    f.app
        .approve_issue(
            &moebius_core::decide::Actor::Person,
            listed.id,
            "looks fine",
        )
        .unwrap();
    assert_eq!(f.column(&listed), "Ready");
    assert!(f.inbox(&listed).is_empty());
}

fn urgent_skips_blockers(f: &Fixture) {
    let blocker = f.card("Backlog", "First", PERSON);
    let blocked = |title: &str| {
        let card = f.card("Ready", title, PERSON);
        f.app
            .store
            .set_dependencies(card.id, &[blocker.number].into())
            .unwrap();
        card
    };

    let normal = blocked("Second");
    let step = f.tick(&normal);
    assert_eq!(
        step.waiting,
        Some(format!("waiting for #{}", blocker.number))
    );
    assert_eq!(f.column(&normal), "Ready");

    let urgent = blocked("[urgent] Third");
    assert_eq!(f.tick(&urgent).moved.as_deref(), Some("In progress"));
}

fn two_rounds(f: &Fixture) {
    let card = f.card("In progress", "Hard one", PERSON);
    f.ended_run(&card, "code", "failed");
    // One failed round: another is started, with the card's note.
    let step = f.tick(&card);
    assert!(step.started_run.is_some());
    assert_eq!(f.harness.started.lock().unwrap().len(), 1);

    // The second round ended in failure too: the card is stuck, and no third run starts.
    let run = f.app.store.runs_for_card(card.id).unwrap().pop().unwrap();
    f.app.store.set_run_status(run.id, "failed").unwrap();
    let step = f.tick(&card);
    assert_eq!(step.started_run, None);
    assert_eq!(f.harness.started.lock().unwrap().len(), 1);
    assert_eq!(
        f.inbox(&card),
        [(
            "stuck".to_string(),
            "the round limit of 2 is reached".to_string()
        )]
    );
}

fn agent_by_title(f: &Fixture) {
    let large = f.card("In progress", "[large] Rewrite the parser", PERSON);
    let small = f.card("In progress", "Fix a typo", PERSON);
    f.tick(&large);
    f.tick(&small);
    let started = f.harness.started.lock().unwrap().clone();
    let agents: Vec<(&str, &str)> = started
        .iter()
        .map(|s| (s.agent.as_str(), s.purpose.as_str()))
        .collect();
    assert_eq!(agents, [("Strong", "code"), ("Coder", "code")]);
}

fn decide_with_a_plugin(f: &Fixture) {
    let fine = f.card("Backlog", "Add a button", PERSON);
    assert_eq!(f.tick(&fine).moved.as_deref(), Some("Ready"));

    let held = f.card("Backlog", "[wip] Add a menu", PERSON);
    let step = f.tick(&held);
    assert_eq!(f.column(&held), "Backlog");
    assert_eq!(step.waiting.as_deref(), Some("the title says [wip]"));

    // Every call records a judgement, by the plugin and the part of it that decided.
    f.tick(&held);
    let judgements: Vec<_> = f
        .app
        .store
        .judgements_for_card(held.id)
        .unwrap()
        .into_iter()
        .filter(|j| j.kind == "start_card")
        .collect();
    assert_eq!(judgements.len(), 2);
    assert_eq!(judgements[0].decider, "start-gate/title");
    assert_eq!(judgements[0].choice, "hold");
}

/// The script file and scenario key of a `luau` block's marker.
fn script_marker(block: &Block) -> (String, Option<String>) {
    let marker = block
        .marker
        .as_deref()
        .unwrap_or_else(|| panic!("{} has no <!-- script: ... --> marker", block.name()));
    let mut script = None;
    let mut key = None;
    for part in marker.split(';') {
        match part.trim().split_once(':') {
            Some(("script", file)) => script = Some(file.trim().to_string()),
            Some(("test", name)) => key = Some(name.trim().to_string()),
            _ => panic!("{}: cannot read the marker {marker:?}", block.name()),
        }
    }
    let script = script.unwrap_or_else(|| panic!("{}: the marker names no script", block.name()));
    (script, key)
}

/// The column a script file belongs to.
fn column_of(script: &str) -> &'static str {
    match script {
        "backlog.luau" => "Backlog",
        "ready.luau" => "Ready",
        "code.luau" => "In progress",
        "review.luau" => "In review",
        "merge.luau" => "Ready to merge",
        "done.luau" => "Done",
        other => panic!("{other} is not the name of a column script"),
    }
}

#[test]
fn every_luau_block_is_a_script_that_runs_and_the_marked_ones_do_what_the_page_says() {
    let scenarios = scenarios();
    let mut used = BTreeSet::new();
    let mut count = 0;
    for block in all_blocks("luau") {
        let (script, key) = script_marker(&block);
        let agents: &[&str] = if key.as_deref() == Some("agent-by-title") {
            &["Coder", "Strong"]
        } else {
            &["Coder"]
        };

        // It loads, defines on_card and runs without an error against a card in its column.
        let smoke = Fixture::new(&script, &block.text, agents);
        let card = smoke.card(column_of(&script), "A card", PERSON);
        smoke
            .engine
            .tick(card.id)
            .unwrap_or_else(|e| panic!("{}: {e}", block.name()));

        // And it does what the page says.
        if let Some(key) = key {
            let scenario = scenarios
                .iter()
                .find(|(name, _)| *name == key)
                .unwrap_or_else(|| panic!("{}: no scenario {key:?}", block.name()))
                .1;
            scenario(&Fixture::new(&script, &block.text, agents));
            used.insert(key);
        }
        count += 1;
    }
    assert!(count >= 6, "only {count} scripts found");
    for (name, _) in &scenarios {
        assert!(used.contains(*name), "no block uses the scenario {name:?}");
    }
}
