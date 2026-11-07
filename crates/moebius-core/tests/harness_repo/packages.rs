//! Pi packages (issue 171): found in a fake npm registry, installed from it and from a local git repository as
//! changes of the harness configuration repository that only a person merges, named by agents as `pkg:<name>`, updated
//! with their changelog, removed, and checked every night for newer versions. The registry is a fake on port 0 and the
//! git sources are local repositories.

use std::path::{Path, PathBuf};

use moebius_core::config::{Config, TimeOfDay};
use moebius_core::repos::git;
use moebius_core::router::Grant;
use moebius_core::server::Server;
use reqwest::Method;
use serde_json::{Value, json};

use super::fixtures::*;
use super::registry::{Registry, package_json};

struct Env {
    dir: tempfile::TempDir,
    origin: PathBuf,
    clone: PathBuf,
    server: Server,
    api: Api,
    registry: Registry,
}

async fn env_with(before: impl FnOnce(&Path)) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    before(&dir.path().join("work"));
    git(
        &dir.path().join("work"),
        &["push", "--quiet", origin.to_str().unwrap(), "main"],
    )
    .unwrap();
    let registry = Registry::start().await;
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\nmcp_env = [\"DOCS_TOKEN\"]\n[packages]\nnpm_registry = {:?}\n",
        fake_pi.display().to_string(),
        origin.display().to_string(),
        registry.url,
    );
    let config = Config::parse(&text, dir.path(), &dir.path().join("moebius.toml")).unwrap();
    let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    let api = Api::start(&server).await;
    let clone = dir.path().join(".moebius/harness");
    Env {
        dir,
        origin,
        clone,
        server,
        api,
        registry,
    }
}

async fn env() -> Env {
    env_with(|_| {}).await
}

/// Publishes `pi-tools` at `version`: an extension named by a `pi` manifest, a skill, and a changelog.
fn publish_tools(registry: &Registry, version: &str, changelog: &str) {
    let manifest = package_json(
        "pi-tools",
        version,
        json!({"description": "Tools for Pi", "pi": {"extensions": ["./src/main.ts"], "skills": ["./skills"]},
               "scripts": {"postinstall": "node setup.js"}}),
    );
    let main = format!("export default () => {{ /* {version} */ }};\n");
    registry.publish(
        "pi-tools",
        version,
        &[
            ("package.json", &manifest),
            ("src/main.ts", &main),
            (
                "skills/review/SKILL.md",
                "---\nname: review\ndescription: \"Review.\"\n---\nLook.\n",
            ),
            ("CHANGELOG.md", changelog),
        ],
    );
}

const LOG_1_0: &str = "# Changelog\n\n## [1.0.0]\n\n- first\n";
const LOG_1_1: &str = "# Changelog\n\n## [1.1.0]\n\n- better\n\n## [1.0.0]\n\n- first\n";

impl Env {
    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (u16, Value) {
        let (status, text) = self.api.call(method, path, body).await;
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    /// Proposes installing `source`, as the API takes it.
    async fn add(&self, source: &str, agent: Option<&str>) -> (u16, Value) {
        let mut body = json!({ "source": source });
        if let Some(agent) = agent {
            body["agent"] = json!(agent);
        }
        self.call(Method::POST, "/v1/extensions/packages", Some(body))
            .await
    }

    /// Merges the change as a person.
    async fn merge(&self, change: &Value) {
        let path = format!("/v1/changes/{}/merge", change["id"]);
        let (status, body) = self
            .call(Method::POST, &path, Some(json!({"reason": "reviewed"})))
            .await;
        assert_eq!(status, 200, "{body}");
    }

    /// Installs and merges `source` for `agent`.
    async fn install(&self, source: &str, agent: Option<&str>) -> Value {
        let (status, offered) = self.add(source, agent).await;
        assert_eq!(status, 200, "{offered}");
        self.merge(&offered["change"]).await;
        offered
    }

    /// A file of a branch of the clone.
    fn show(&self, branch: &str, path: &str) -> String {
        git(&self.clone, &["show", &format!("{branch}:{path}")]).unwrap()
    }

    fn branch_files(&self, branch: &str, folder: &str) -> Vec<String> {
        git(
            &self.clone,
            &["ls-tree", "-r", "--name-only", branch, folder],
        )
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
    }

    fn coder(&self) -> moebius_core::config::AgentConfig {
        self.server.harness.agents.read().unwrap().agents["Coder"].clone()
    }
}

fn file_url(path: &Path) -> String {
    let path = path.display().to_string().replace('\\', "/");
    format!("file://{}{path}", if cfg!(windows) { "/" } else { "" })
}

fn npm_available() -> bool {
    let program = if cfg!(windows) { "npm.cmd" } else { "npm" };
    std::process::Command::new(program)
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

#[tokio::test]
async fn the_catalog_is_searched_by_keyword_text_and_type() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    env.registry.describe(
        "pi-tools",
        &["pi-package", "pi-extension", "pi-skill"],
        "Tools for Pi",
    );
    env.registry.publish("pi-themes", "2.0.0", &[]);
    env.registry
        .describe("pi-themes", &["pi-package", "theme"], "Dark colours");
    env.registry.publish("not-pi", "1.0.0", &[]);
    env.registry.describe(
        "not-pi",
        &["tools"],
        "Tools for Pi, but no package of the catalog",
    );

    let (status, all) = env.call(Method::GET, "/v1/extensions/search", None).await;
    assert_eq!(status, 200, "{all}");
    let names = |hits: &Value| -> Vec<String> {
        hits.as_array()
            .unwrap()
            .iter()
            .map(|h| h["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names(&all), ["pi-themes", "pi-tools"]);
    let tools = &all[1];
    assert_eq!(tools["version"], "1.0.0");
    assert_eq!(tools["description"], "Tools for Pi");
    assert_eq!(tools["kinds"], json!(["extension", "skill"]));
    assert_eq!(tools["weekly_downloads"], 42);

    let (_, by_text) = env
        .call(Method::GET, "/v1/extensions/search?q=Dark", None)
        .await;
    assert_eq!(names(&by_text), ["pi-themes"]);
    let (_, by_type) = env
        .call(Method::GET, "/v1/extensions/search?type=skill", None)
        .await;
    assert_eq!(names(&by_type), ["pi-tools"]);
    let (status, _) = env
        .call(Method::GET, "/v1/extensions/search?type=gadget", None)
        .await;
    assert_eq!(status, 422);
    // Every search is the registry's own with the catalog's keyword in front of the words.
    assert_eq!(
        env.registry.searches(),
        [
            "keywords:pi-package",
            "keywords:pi-package Dark",
            "keywords:pi-package"
        ]
    );
}

#[tokio::test]
async fn an_npm_package_is_proposed_as_a_change_that_a_person_merges_and_an_agent_names() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    let start = head(&env.clone);

    let (status, offered) = env.add("npm:pi-tools", Some("Coder")).await;
    assert_eq!(status, 200, "{offered}");
    let branch = offered["change"]["branch"].as_str().unwrap();
    assert_eq!(
        (offered["action"].as_str(), offered["to"].as_str()),
        (Some("add"), Some("1.0.0"))
    );
    assert!(
        offered["integrity"]
            .as_str()
            .unwrap()
            .starts_with("sha512-")
    );
    assert_eq!(offered["resources"]["extensions"], json!(["src/main.ts"]));
    assert_eq!(offered["resources"]["skills"], json!(["skills/review"]));
    assert_eq!(offered["agents"], json!(["Coder"]));
    assert!(
        offered["files"]
            .as_array()
            .unwrap()
            .contains(&json!("A packages/pi-tools/src/main.ts"))
    );

    // It waits in the inbox, and nothing is installed until a person merges it.
    let inbox = env.server.app.store.inbox().unwrap();
    assert_eq!((inbox.len(), inbox[0].kind.as_str()), (1, "ready_to_merge"));
    assert_eq!(head(&env.clone), start);
    let (_, listed) = env.call(Method::GET, "/v1/extensions/packages", None).await;
    assert_eq!(listed, json!([]));
    assert_eq!(
        env.coder().extensions,
        [env.clone.join("extensions/log.ts")]
    );

    // The branch shows the package, its version, source, download and integrity, and the agent's names for it.
    let lock = env.show(branch, "packages.lock");
    for expected in [
        "name = \"pi-tools\"",
        "source = \"npm:pi-tools\"",
        "version = \"1.0.0\"",
        "/tarballs/pi-tools-1.0.0.tgz",
        "integrity = \"sha512-",
        "sha256 = \"",
    ] {
        assert!(lock.contains(expected), "{expected} in {lock}");
    }
    assert!(
        env.show(branch, "agents/Coder.toml")
            .contains("pkg:pi-tools")
    );
    let message = git(&env.clone, &["log", "-1", "--format=%B", branch]).unwrap();
    assert!(
        message.contains("Install scripts (not run): postinstall"),
        "{message}"
    );
    assert!(message.contains("Extensions: src/main.ts"), "{message}");

    // A person merges it: it lands on the origin and the Coder gets the package's extension and skill.
    env.merge(&offered["change"]).await;
    assert!(git(&env.origin, &["show", "main:packages/pi-tools/src/main.ts"]).is_ok());
    assert!(git(&env.origin, &["show", "main:packages.lock"]).is_ok());
    let (_, listed) = env.call(Method::GET, "/v1/extensions/packages", None).await;
    assert_eq!(listed[0]["name"], "pi-tools");
    assert_eq!(listed[0]["version"], "1.0.0");
    assert_eq!(listed[0]["agents"], json!(["Coder"]));
    assert_eq!(listed[0]["install_scripts"], json!(["postinstall"]));
    assert_eq!(
        env.coder().extensions,
        [
            env.clone.join("extensions/log.ts"),
            env.clone.join("packages/pi-tools/src/main.ts")
        ]
    );
    assert!(
        env.coder()
            .skills
            .contains(&env.clone.join("packages/pi-tools/skills/review"))
    );

    // Installing it again is refused, and info says it is installed.
    let (status, again) = env.add("npm:pi-tools", None).await;
    assert_eq!(status, 409, "{again}");
    let (status, info) = env
        .call(
            Method::GET,
            "/v1/extensions/info?source=npm%3Api-tools",
            None,
        )
        .await;
    assert_eq!(status, 200, "{info}");
    assert_eq!(
        (info["name"].as_str(), info["installed"].as_str()),
        (Some("pi-tools"), Some("1.0.0"))
    );
    assert_eq!(info["description"], "Tools for Pi");
}

#[tokio::test]
async fn a_version_range_takes_the_newest_release_it_allows() {
    let env = env().await;
    for version in ["1.0.0", "1.2.0", "2.0.0"] {
        publish_tools(&env.registry, version, LOG_1_0);
    }
    let (status, offered) = env.add("npm:pi-tools@^1.0.0", None).await;
    assert_eq!(status, 200, "{offered}");
    assert_eq!(offered["to"], "1.2.0");
    assert_eq!(offered["source"], "npm:pi-tools@^1.0.0");
    let (status, error) = env.add("npm:pi-tools@>=1", None).await;
    assert_eq!(status, 422, "{error}");
    let (status, error) = env.add("pi-tools", None).await;
    assert_eq!(status, 422, "{error}");
    let (status, error) = env.add("npm:no-such-package", None).await;
    assert_eq!(status, 404, "{error}");
}

#[tokio::test]
async fn the_pi_manifest_and_the_convention_directories_are_both_read() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    // No manifest: the convention directories.
    env.registry.publish(
        "plain-pack",
        "0.3.0",
        &[
            ("extensions/one.ts", ""),
            ("extensions/two/index.js", ""),
            (
                "skills/audit/SKILL.md",
                "---\nname: audit\ndescription: x\n---\n",
            ),
            ("prompts/fix.md", "Fix it."),
            ("themes/dark.json", "{}"),
        ],
    );
    env.install("npm:pi-tools", None).await;
    env.install("npm:plain-pack", None).await;
    let (_, listed) = env.call(Method::GET, "/v1/extensions/packages", None).await;
    let by_name = |name: &str| {
        listed
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(
        by_name("pi-tools")["resources"]["extensions"],
        json!(["src/main.ts"])
    );
    let plain = by_name("plain-pack");
    assert_eq!(
        plain["resources"]["extensions"],
        json!(["extensions/one.ts", "extensions/two/index.js"])
    );
    assert_eq!(plain["resources"]["skills"], json!(["skills/audit"]));
    // Prompt templates and themes are listed and not loaded.
    assert_eq!(plain["resources"]["prompts"], json!(["prompts/fix.md"]));
    assert_eq!(plain["resources"]["themes"], json!(["themes/dark.json"]));
}

#[tokio::test]
async fn a_download_that_is_not_what_the_registry_said_is_refused() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    env.registry.corrupt("pi-tools", "1.0.0");
    let (status, error) = env.add("npm:pi-tools", Some("Coder")).await;
    assert_eq!(status, 422, "{error}");
    assert!(
        error
            .as_str()
            .unwrap()
            .contains("integrity of pi-tools@1.0.0 does not match"),
        "{error}"
    );
    // Nothing was offered or left behind.
    assert!(env.server.app.store.inbox().unwrap().is_empty());
    assert!(env.server.app.store.harness_changes().unwrap().is_empty());
    assert_eq!(
        git(&env.clone, &["branch", "--list", "moebius/ext-*"]).unwrap(),
        ""
    );
    let (_, listed) = env.call(Method::GET, "/v1/extensions/packages", None).await;
    assert_eq!(listed, json!([]));
}

#[tokio::test]
async fn no_install_script_runs_and_the_production_dependencies_are_vendored() {
    if !npm_available() {
        eprintln!("npm is not installed: the install of dependencies is not tested");
        return;
    }
    let env = env().await;
    // The package and its dependency both have a postinstall script that would leave a file behind.
    let script = "node -e \"require('fs').writeFileSync('ran.txt', 'x')\"";
    env.registry.publish(
        "tiny-dep",
        "1.0.0",
        &[
            (
                "package.json",
                &package_json(
                    "tiny-dep",
                    "1.0.0",
                    json!({"scripts": {"postinstall": script}}),
                ),
            ),
            ("index.js", "module.exports = 1;\n"),
        ],
    );
    env.registry
        .publish("dev-only", "1.0.0", &[("index.js", "")]);
    env.registry.publish(
        "with-dep",
        "1.0.0",
        &[
            (
                "package.json",
                &package_json(
                    "with-dep",
                    "1.0.0",
                    json!({"dependencies": {"tiny-dep": "1.0.0"}, "devDependencies": {"dev-only": "1.0.0"},
                           "scripts": {"postinstall": script}}),
                ),
            ),
            ("extensions/uses-dep.ts", "import dep from 'tiny-dep';\n"),
        ],
    );
    let (status, offered) = env.add("npm:with-dep", None).await;
    assert_eq!(status, 200, "{offered}");
    let branch = offered["change"]["branch"].as_str().unwrap();
    let files = env.branch_files(branch, "packages/with-dep");
    assert!(
        files.contains(&"packages/with-dep/node_modules/tiny-dep/index.js".to_string()),
        "{files:?}"
    );
    assert!(
        !files.iter().any(|f| f.ends_with("ran.txt")),
        "an install script ran: {files:?}"
    );
    assert!(!files.iter().any(|f| f.contains("dev-only")), "{files:?}");
    // The dependency's files are counted, not listed.
    assert!(offered["dependency_files"].as_u64().unwrap() >= 2);
    assert!(
        !offered["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_str().unwrap().contains("node_modules"))
    );
}

#[tokio::test]
async fn a_package_from_a_git_repository_is_locked_at_its_commit() {
    let env = env().await;
    let repo = env.dir.path().join("pi-git-tools");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "-b", "main"]).unwrap();
    write(
        &repo,
        "package.json",
        &package_json("git-tools", "0.1.0", json!({})),
    );
    write(&repo, "extensions/hello.ts", "export default () => {};\n");
    commit_all(&repo, "first");
    git(&repo, &["tag", "v1"]).unwrap();
    let v1 = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    write(
        &repo,
        "extensions/hello.ts",
        "export default () => { /* two */ };\n",
    );
    commit_all(&repo, "second");
    let url = file_url(&repo);

    // A tag is pinned: the package is the first commit, and an update finds nothing.
    let tagged = format!("git:{url}@v1");
    let (status, offered) = env.add(&tagged, Some("Coder")).await;
    assert_eq!(status, 200, "{offered}");
    assert_eq!(
        (offered["package"].as_str(), offered["to"].as_str()),
        (Some("git-tools"), Some("0.1.0"))
    );
    assert_eq!(offered["resolved"], v1.as_str());
    assert_eq!(offered["integrity"], Value::Null);
    let branch = offered["change"]["branch"].as_str().unwrap();
    assert!(
        env.show(branch, "packages/git-tools/extensions/hello.ts")
            .contains("export default () => {};")
    );
    assert!(
        env.branch_files(branch, "packages/git-tools")
            .iter()
            .all(|f| !f.contains(".git/"))
    );
    let lock = env.show(branch, "packages.lock");
    assert!(
        lock.contains(&format!("resolved = \"{v1}\"")) && lock.contains(&tagged),
        "{lock}"
    );
    env.merge(&offered["change"]).await;
    let (status, updates) = env
        .call(
            Method::POST,
            "/v1/extensions/packages/git-tools/update",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200, "{updates}");
    assert_eq!(updates["up_to_date"], json!(["git-tools"]));
}

#[tokio::test]
async fn a_git_package_is_updated_to_the_newest_commit_of_its_ref_with_the_diff() {
    let env = env().await;
    let repo = env.dir.path().join("pi-follow");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "-b", "main"]).unwrap();
    write(
        &repo,
        "package.json",
        &package_json("follow", "0.1.0", json!({})),
    );
    write(&repo, "extensions/hello.ts", "export default () => {};\n");
    commit_all(&repo, "first");
    let source = format!("git:{}@main", file_url(&repo));
    env.install(&source, None).await;

    write(
        &repo,
        "package.json",
        &package_json("follow", "0.2.0", json!({})),
    );
    write(
        &repo,
        "extensions/hello.ts",
        "export default () => { /* better */ };\n",
    );
    commit_all(&repo, "second");
    let second = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let (status, updates) = env
        .call(
            Method::POST,
            "/v1/extensions/packages/follow/update",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200, "{updates}");
    let offered = &updates["offered"][0];
    assert_eq!(
        (offered["from"].as_str(), offered["to"].as_str()),
        (Some("0.1.0"), Some("0.2.0"))
    );
    assert_eq!(offered["resolved"], second.as_str());
    assert!(
        offered["files"]
            .as_array()
            .unwrap()
            .contains(&json!("M packages/follow/extensions/hello.ts"))
    );
    assert!(
        offered["diff"]
            .as_str()
            .unwrap()
            .contains("+export default () => { /* better */ };")
    );
    // The installed package is the old one until a person merges.
    let (_, listed) = env.call(Method::GET, "/v1/extensions/packages", None).await;
    assert_eq!(listed[0]["version"], "0.1.0");
    env.merge(&offered["change"]).await;
    let (_, listed) = env.call(Method::GET, "/v1/extensions/packages", None).await;
    assert_eq!(
        (
            listed[0]["version"].as_str(),
            listed[0]["resolved"].as_str()
        ),
        (Some("0.2.0"), Some(second.as_str()))
    );
}

#[tokio::test]
async fn an_update_shows_the_changelog_between_the_versions_and_stays_in_the_range() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    env.registry
        .publish("ranged", "1.0.0", &[("extensions/a.ts", "")]);
    env.install("npm:pi-tools", Some("Coder")).await;
    env.install("npm:ranged@^1.0.0", None).await;
    publish_tools(&env.registry, "1.1.0", LOG_1_1);
    env.registry
        .publish("ranged", "2.0.0", &[("extensions/a.ts", "")]);

    let (status, updates) = env
        .call(Method::POST, "/v1/extensions/update", Some(json!({})))
        .await;
    assert_eq!(status, 200, "{updates}");
    // The package without a range moves to the newest; the one that asked for ^1.0.0 stays.
    assert_eq!(updates["up_to_date"], json!(["ranged"]));
    let offered = &updates["offered"][0];
    assert_eq!(updates["offered"].as_array().unwrap().len(), 1);
    assert_eq!(
        (
            offered["package"].as_str(),
            offered["from"].as_str(),
            offered["to"].as_str()
        ),
        (Some("pi-tools"), Some("1.0.0"), Some("1.1.0"))
    );
    let changelog = offered["changelog"].as_str().unwrap();
    assert!(
        changelog.contains("## [1.1.0]") && changelog.contains("- better"),
        "{changelog}"
    );
    assert!(!changelog.contains("- first"), "{changelog}");
    assert!(
        offered["diff"]
            .as_str()
            .unwrap()
            .contains("-export default () => { /* 1.0.0 */ };")
    );
    assert_eq!(offered["agents"], json!(["Coder"]));
    let branch = offered["change"]["branch"].as_str().unwrap();
    assert!(
        env.show(branch, "packages.lock")
            .contains("version = \"1.1.0\"")
    );
    assert!(
        env.show(branch, "packages/pi-tools/CHANGELOG.md")
            .contains("## [1.1.0]")
    );
    env.merge(&offered["change"]).await;
    let (_, again) = env
        .call(
            Method::POST,
            "/v1/extensions/packages/pi-tools/update",
            Some(json!({})),
        )
        .await;
    assert_eq!(again["up_to_date"], json!(["pi-tools"]));
    let (status, _) = env
        .call(
            Method::POST,
            "/v1/extensions/packages/nothing/update",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_package_is_removed_with_its_names_in_the_agents() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    env.install("npm:pi-tools", Some("Coder")).await;
    assert_eq!(env.coder().extensions.len(), 2);

    let (status, offered) = env
        .call(Method::DELETE, "/v1/extensions/packages/pi-tools", None)
        .await;
    assert_eq!(status, 200, "{offered}");
    assert_eq!(
        (offered["action"].as_str(), offered["from"].as_str()),
        (Some("remove"), Some("1.0.0"))
    );
    assert_eq!(offered["agents"], json!(["Coder"]));
    let branch = offered["change"]["branch"].as_str().unwrap();
    assert!(env.branch_files(branch, "packages").is_empty());
    assert!(
        git(
            &env.clone,
            &["cat-file", "-e", &format!("{branch}:packages.lock")]
        )
        .is_err()
    );
    assert!(!env.show(branch, "agents/Coder.toml").contains("pkg:"));
    // Until a person merges, the package is still given to the agent.
    assert_eq!(env.coder().extensions.len(), 2);
    env.merge(&offered["change"]).await;
    assert_eq!(
        env.coder().extensions,
        [env.clone.join("extensions/log.ts")]
    );
    let (_, listed) = env.call(Method::GET, "/v1/extensions/packages", None).await;
    assert_eq!(listed, json!([]));
    assert!(!env.clone.join("packages/pi-tools").exists());
    let (status, _) = env
        .call(Method::DELETE, "/v1/extensions/packages/pi-tools", None)
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_package_that_is_not_in_the_lock_is_an_error_of_the_agent() {
    let env = env_with(|work| {
        write(
            work,
            "agents/Ghost.toml",
            "model = \"x\"\nextensions = [\"pkg:ghost\"]\n",
        );
        write(
            work,
            "agents/Spooky.toml",
            "model = \"x\"\nskills = [\"pkg:ghost/skills/boo\"]\n",
        );
        commit_all(work, "Agents that name a package that is not there");
    })
    .await;
    {
        let set = env.server.harness.agents.read().unwrap();
        assert!(!set.agents.contains_key("Ghost") && !set.agents.contains_key("Spooky"));
        assert!(
            set.errors["Ghost"].contains("the package \"ghost\" is not in packages.lock"),
            "{:?}",
            set.errors
        );
        assert!(
            set.errors["Spooky"].contains("not in packages.lock"),
            "{:?}",
            set.errors
        );
    }
    // Saving such an agent through the API is refused the same way, and so is a resource the package does not have.
    let body = json!({"settings": {"model": "x", "extensions": ["pkg:ghost"]}, "instructions": ""});
    let (status, error) = env.call(Method::PUT, "/v1/agents/Ghost2", Some(body)).await;
    assert_eq!(status, 422, "{error}");
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    env.install("npm:pi-tools", None).await;
    let body = json!({"settings": {"model": "x", "extensions": ["pkg:pi-tools/src/other.ts"]}, "instructions": ""});
    let (status, error) = env.call(Method::PUT, "/v1/agents/Ghost2", Some(body)).await;
    assert_eq!(status, 422, "{error}");
    assert!(
        error.as_str().unwrap().contains("has no extension"),
        "{error}"
    );
    let body = json!({"settings": {"model": "x", "extensions": ["pkg:pi-tools/src/main.ts"], "skills": ["pkg:pi-tools"]}, "instructions": ""});
    let (status, saved) = env.call(Method::PUT, "/v1/agents/Ghost2", Some(body)).await;
    assert_eq!(status, 200, "{saved}");
}

// Unix only: the run is on the fake Pi (testdata/fake-pi), a Python script started by its `#!` line.
#[cfg(unix)]
#[tokio::test]
async fn a_run_gets_the_extensions_and_skills_of_a_package_before_the_bridge() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    env.install("npm:pi-tools", Some("Coder")).await;
    let card = new_card(&env.server, env.dir.path());
    let argv = argv_of_a_run(&env.server, env.dir.path(), &card, "Coder");
    let values = |flag: &str| -> Vec<String> {
        argv.windows(2)
            .filter(|w| w[0] == flag)
            .map(|w| w[1].clone())
            .collect()
    };
    // The Coder has an MCP server, so Pi's MCP support comes first; then its own extensions, then the bridge's.
    let extensions: Vec<String> = values("--extension")
        .into_iter()
        .filter(|extension| !extension.starts_with("builtin:"))
        .collect();
    let main = env
        .clone
        .join("packages/pi-tools/src/main.ts")
        .display()
        .to_string();
    assert_eq!(
        extensions[0],
        env.clone.join("extensions/log.ts").display().to_string()
    );
    assert_eq!(extensions[1], main);
    assert!(
        extensions[extensions.len() - 1].ends_with("moebius-bridge.ts"),
        "{extensions:?}"
    );
    assert!(
        values("--skill").contains(
            &env.clone
                .join("packages/pi-tools/skills/review")
                .display()
                .to_string()
        )
    );
    // Pi's own package handling is never used.
    assert!(argv.iter().any(|a| a == "--no-extensions"));
    assert!(
        !argv
            .iter()
            .any(|a| a.starts_with("npm:") || a.starts_with("git:")),
        "{argv:?}"
    );
}

#[tokio::test]
async fn only_a_person_merges_a_change_that_touches_a_package() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    let run = env
        .server
        .app
        .store
        .add_run(None, "Moebius", "pi-bolt", "chat")
        .unwrap();
    let token = env.server.router.issue_token(Grant {
        run_id: run.id,
        card_id: None,
        budget: None,
    });

    // The Moebius agent can search and propose.
    let (status, _) = env
        .api
        .call_with(&token, Method::GET, "/v1/extensions/search", None)
        .await;
    assert_eq!(status, 200);
    let body = json!({"source": "npm:pi-tools", "agent": "Coder"});
    let (status, offered) = env
        .api
        .call_with(&token, Method::POST, "/v1/extensions/packages", Some(body))
        .await;
    assert_eq!(status, 200, "{offered}");
    let offered: Value = serde_json::from_str(&offered).unwrap();

    // It cannot merge the change: it stays open.
    let path = format!("/v1/changes/{}/merge", offered["change"]["id"]);
    let (status, refused) = env
        .api
        .call_with(&token, Method::POST, &path, Some(json!({"reason": "ok"})))
        .await;
    assert_eq!(status, 403, "{refused}");
    assert!(refused.contains("only a person"), "{refused}");
    let change = env
        .server
        .app
        .store
        .change(offered["change"]["id"].as_i64().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(change.state, "open");
    assert!(env.coder().extensions.len() == 1);
    assert!(env.server.app.store.judgements("merge").unwrap().is_empty());

    // A person can.
    env.merge(&offered["change"]).await;
    assert_eq!(env.coder().extensions.len(), 2);
}

#[tokio::test]
async fn the_nightly_check_puts_one_inbox_item_per_package_with_a_newer_version() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    env.registry
        .publish("steady", "1.0.0", &[("extensions/a.ts", "")]);
    env.install("npm:pi-tools", None).await;
    env.install("npm:steady", None).await;
    let app = env.server.app.clone();
    let check = {
        let app = app.clone();
        move || {
            let app = app.clone();
            async move {
                tokio::task::spawn_blocking(move || app.check_package_updates().unwrap())
                    .await
                    .unwrap()
            }
        }
    };
    let mut events = app.bus.subscribe();
    assert!(check().await.is_empty());
    assert!(app.store.inbox().unwrap().is_empty());
    assert!(events.try_recv().is_err());

    publish_tools(&env.registry, "1.1.0", LOG_1_1);
    let found = check().await;
    let event = events.try_recv().unwrap();
    assert_eq!(event.kind, "extension_update_available");
    assert_eq!(event.data, json!({"name": "pi-tools", "version": "1.1.0"}));
    assert_eq!(found.len(), 1);
    assert_eq!(
        (
            found[0].package.as_str(),
            found[0].from.as_str(),
            found[0].to.as_str()
        ),
        ("pi-tools", "1.0.0", "1.1.0")
    );
    let inbox = app.store.inbox().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].kind, "extension_update");
    assert_eq!(inbox[0].title, "pi-tools 1.0.0 -> 1.1.0");
    assert_eq!(inbox[0].data["package"], "pi-tools");
    assert_eq!(inbox[0].data["to"], "1.1.0");
    // Checking again adds nothing; a newer version replaces the item.
    check().await;
    assert!(events.try_recv().is_err());
    assert_eq!(app.store.inbox().unwrap().len(), 1);
    publish_tools(&env.registry, "1.2.0", LOG_1_1);
    check().await;
    let inbox = app.store.inbox().unwrap();
    assert_eq!(
        (inbox.len(), inbox[0].data["to"].clone()),
        (1, json!("1.2.0"))
    );

    // Proposing the update resolves the item (the change itself is in the inbox as ready to merge).
    let (status, updates) = env
        .call(
            Method::POST,
            "/v1/extensions/packages/pi-tools/update",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200, "{updates}");
    let kinds: Vec<String> = app
        .store
        .inbox()
        .unwrap()
        .into_iter()
        .map(|i| i.kind)
        .collect();
    assert_eq!(kinds, ["ready_to_merge"]);
}

#[tokio::test]
async fn the_schedule_checks_once_a_day_from_the_time_of_the_retro() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    env.install("npm:pi-tools", None).await;
    publish_tools(&env.registry, "1.1.0", LOG_1_1);
    let app = env.server.app.clone();
    let run = |at: u16| {
        let app = app.clone();
        async move {
            tokio::task::spawn_blocking(move || app.on_package_schedule(TimeOfDay(at)).unwrap())
                .await
                .unwrap()
        }
    };
    // Not yet: the time of the Retro is tomorrow's minute 1440, which no day reaches.
    let asked = env.registry.packuments();
    run(24 * 60).await;
    assert_eq!(env.registry.packuments(), asked);
    assert!(app.store.inbox().unwrap().is_empty());
    // From midnight on, the first look at the schedule checks, and the rest of the day does not.
    run(0).await;
    assert_eq!(env.registry.packuments(), asked + 1);
    assert_eq!(app.store.inbox().unwrap().len(), 1);
    run(0).await;
    assert_eq!(env.registry.packuments(), asked + 1);
}

#[tokio::test]
async fn a_dependency_that_is_a_git_repository_is_refused_and_nothing_runs() {
    if !npm_available() {
        eprintln!("npm is not installed: the install of dependencies is not tested");
        return;
    }
    let env = env().await;
    // A repository whose `prepare` script leaves a file outside of it when npm builds it, whatever `--ignore-scripts`.
    let evil = env.dir.path().join("evil");
    std::fs::create_dir_all(&evil).unwrap();
    git(&evil, &["init", "--quiet", "-b", "main"]).unwrap();
    let pwned = env.dir.path().join("PWNED");
    write(
        &evil,
        "mark.js",
        &format!(
            "require('fs').writeFileSync({:?}, 'x');\n",
            pwned.display().to_string().replace('\\', "/")
        ),
    );
    write(
        &evil,
        "package.json",
        &package_json(
            "evil",
            "1.0.0",
            json!({"scripts": {"prepare": "node mark.js"}}),
        ),
    );
    commit_all(&evil, "evil");
    for spec in [
        format!("git+{}", file_url(&evil)),
        format!("file:{}", evil.display()),
        "github:someone/evil".to_string(),
        "https://example.com/evil.tgz".to_string(),
        "npm:other@1".to_string(),
    ] {
        env.registry.publish(
            "bad-dep",
            "1.0.0",
            &[(
                "package.json",
                &package_json("bad-dep", "1.0.0", json!({"dependencies": {"evil": spec}})),
            )],
        );
        let (status, error) = env.add("npm:bad-dep", None).await;
        assert_eq!(status, 422, "{spec}: {error}");
        assert!(
            error.as_str().unwrap().contains("not from the registry"),
            "{error}"
        );
        assert!(!pwned.exists(), "a script ran for {spec}");
    }
    assert!(env.server.app.store.harness_changes().unwrap().is_empty());
}

#[tokio::test]
async fn an_agent_that_is_not_a_name_is_refused_and_no_file_is_written() {
    let env = env().await;
    publish_tools(&env.registry, "1.0.0", LOG_1_0);
    let victim = env.dir.path().join("victim.toml");
    std::fs::write(&victim, "# keep\n").unwrap();
    let outside = victim.with_extension("").display().to_string();
    for agent in [outside.as_str(), "../victim", "../rules", "Coder/../Coder"] {
        let (status, error) = env.add("npm:pi-tools", Some(agent)).await;
        assert_eq!(status, 422, "{agent}: {error}");
    }
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "# keep\n");
    assert!(env.server.app.store.harness_changes().unwrap().is_empty());
    assert_eq!(
        git(&env.clone, &["branch", "--list", "moebius/ext-*"]).unwrap(),
        ""
    );
}

#[tokio::test]
async fn only_a_person_merges_a_proposal_that_touches_a_package() {
    let env = env().await;
    let branch = "proposal/2026-03-10-package";
    git(&env.clone, &["checkout", "--quiet", "-b", branch]).unwrap();
    write(
        &env.clone,
        "packages/sneaky/extensions/x.ts",
        "export default () => {};\n",
    );
    commit_all(&env.clone, "Add a package");
    git(&env.clone, &["checkout", "--quiet", "main"]).unwrap();
    let store = &env.server.app.store;
    store.add_retro("2026-03-10", &json!({}), "done").unwrap();
    store
        .add_proposal("2026-03-10", branch, "Add a package", "", &[], &[])
        .unwrap();
    let id = store.proposals(None).unwrap()[0].id;
    store.set_proposal_status(id, "untried").unwrap();
    let run = store.add_run(None, "Moebius", "pi-bolt", "chat").unwrap();
    let token = env.server.router.issue_token(Grant {
        run_id: run.id,
        card_id: None,
        budget: None,
    });

    let path = format!("/v1/proposals/{id}/merge");
    let (status, refused) = env
        .api
        .call_with(&token, Method::POST, &path, Some(json!({"reason": "ok"})))
        .await;
    assert_eq!(status, 403, "{refused}");
    assert!(refused.contains("only a person"), "{refused}");
    let status_of = || store.proposal(id).unwrap().unwrap().status;
    assert_ne!(status_of(), "merged");
    assert!(!env.clone.join("packages/sneaky").exists());

    let (status, merged) = env
        .call(Method::POST, &path, Some(json!({"reason": "reviewed"})))
        .await;
    assert_eq!(status, 200, "{merged}");
    assert_eq!(status_of(), "merged");
    assert!(env.clone.join("packages/sneaky/extensions/x.ts").exists());
}

#[tokio::test]
async fn the_merge_itself_refuses_an_agent_a_package_whatever_was_checked_before() {
    let env = env().await;
    let branch = "moebius/late";
    git(&env.clone, &["checkout", "--quiet", "-b", branch]).unwrap();
    write(&env.clone, "packages.lock", "# late\n");
    commit_all(&env.clone, "Late commit");
    git(&env.clone, &["checkout", "--quiet", "main"]).unwrap();
    let repo = env.server.harness_repo.as_ref().unwrap();
    let agent = moebius_core::decide::Actor::Agent {
        name: "Moebius".into(),
        run: 1,
    };
    let refused = repo.merge_branch(&env.server.app, &agent, branch, "m");
    assert!(
        matches!(refused, Err(moebius_core::app::AppError::Forbidden(_))),
        "{refused:?}"
    );
    assert!(
        !env.clone.join("packages.lock").exists()
            || !std::fs::read_to_string(env.clone.join("packages.lock"))
                .unwrap()
                .contains("late")
    );
    repo.merge_branch(
        &env.server.app,
        &moebius_core::decide::Actor::Person,
        branch,
        "m",
    )
    .unwrap();
}
