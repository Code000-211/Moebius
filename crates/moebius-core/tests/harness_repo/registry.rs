//! A fake npm registry on port 0 for the tests of Pi packages: the search of the catalog, packuments and tarballs of the
//! packages a test publishes. A test can corrupt a version's integrity, and read what was asked.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use base64::Engine;
use serde_json::{Value, json};
use sha2::{Digest, Sha512};

struct Published {
    files: Vec<(String, String)>,
    /// What the packument says the integrity is, when a test made it wrong.
    integrity: Option<String>,
}

#[derive(Default)]
struct Package {
    versions: BTreeMap<String, Published>,
    latest: String,
    keywords: Vec<String>,
    description: String,
}

#[derive(Default)]
struct State_ {
    base: String,
    packages: BTreeMap<String, Package>,
    /// The `text` of every search, and how many packuments were asked for.
    searches: Vec<String>,
    packuments: usize,
}

type Shared = Arc<Mutex<State_>>;

/// The fake registry.
pub struct Registry {
    pub url: String,
    state: Shared,
}

/// A `package.json` text for the package `name` at `version` with the `fields` added.
pub fn package_json(name: &str, version: &str, fields: Value) -> String {
    let mut json = json!({ "name": name, "version": version, "keywords": ["pi-package"] });
    for (key, value) in fields.as_object().into_iter().flatten() {
        json[key] = value.clone();
    }
    json.to_string()
}

fn tarball(files: &[(String, String)]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for (path, text) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(text.len() as u64);
        header.set_mode(0o644);
        archive
            .append_data(&mut header, format!("package/{path}"), text.as_bytes())
            .unwrap();
    }
    let mut encoder = archive.into_inner().unwrap();
    encoder.flush().unwrap();
    encoder.finish().unwrap()
}

fn integrity_of(bytes: &[u8]) -> String {
    format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode(Sha512::digest(bytes))
    )
}

fn file_name(name: &str, version: &str) -> String {
    format!("{}-{version}.tgz", name.replace('/', "__"))
}

impl Registry {
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state: Shared = Arc::new(Mutex::new(State_ {
            base: url.clone(),
            ..Default::default()
        }));
        let app = Router::new()
            .route("/-/v1/search", get(search))
            .route("/tarballs/{file}", get(download))
            .route("/{name}", get(packument))
            .with_state(state.clone());
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self { url, state }
    }

    /// Publishes `name` at `version` with `files` (a `package.json` is made when none is given), as the latest.
    pub fn publish(&self, name: &str, version: &str, files: &[(&str, &str)]) {
        let mut files: Vec<(String, String)> = files
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string()))
            .collect();
        if !files.iter().any(|(path, _)| path == "package.json") {
            files.push((
                "package.json".into(),
                package_json(name, version, json!({})),
            ));
        }
        let mut state = self.state.lock().unwrap();
        let package = state.packages.entry(name.to_string()).or_default();
        package.versions.insert(
            version.to_string(),
            Published {
                files,
                integrity: None,
            },
        );
        package.latest = version.to_string();
        if package.keywords.is_empty() {
            package.keywords = vec!["pi-package".into()];
        }
    }

    /// Sets the keywords and description the search shows for `name`.
    pub fn describe(&self, name: &str, keywords: &[&str], description: &str) {
        let mut state = self.state.lock().unwrap();
        let package = state.packages.get_mut(name).unwrap();
        package.keywords = keywords.iter().map(|k| k.to_string()).collect();
        package.description = description.to_string();
    }

    /// Makes the packument give another integrity than the tarball has.
    pub fn corrupt(&self, name: &str, version: &str) {
        let mut state = self.state.lock().unwrap();
        let wrong = integrity_of(b"something else");
        state
            .packages
            .get_mut(name)
            .unwrap()
            .versions
            .get_mut(version)
            .unwrap()
            .integrity = Some(wrong);
    }

    /// The `text` of every search made.
    pub fn searches(&self) -> Vec<String> {
        self.state.lock().unwrap().searches.clone()
    }

    /// How many packuments were asked for.
    pub fn packuments(&self) -> usize {
        self.state.lock().unwrap().packuments
    }
}

async fn search(
    State(state): State<Shared>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let text = query.get("text").cloned().unwrap_or_default();
    let mut state = state.lock().unwrap();
    state.searches.push(text.clone());
    let Some(words) = text.strip_prefix("keywords:pi-package") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let objects: Vec<Value> = state
        .packages
        .iter()
        .filter(|(_, package)| package.keywords.iter().any(|k| k == "pi-package"))
        .filter(|(name, package)| {
            words
                .split_whitespace()
                .all(|word| name.contains(word) || package.description.contains(word))
        })
        .map(|(name, package)| {
            json!({
                "package": {
                    "name": name, "version": package.latest, "description": package.description,
                    "keywords": package.keywords, "date": "2026-01-01T00:00:00.000Z",
                    "links": { "npm": format!("https://www.npmjs.com/package/{name}") }
                },
                "downloads": { "weekly": 42 }
            })
        })
        .collect();
    axum::Json(json!({ "objects": objects, "total": objects.len() })).into_response()
}

async fn packument(State(state): State<Shared>, Path(name): Path<String>) -> Response {
    let mut state = state.lock().unwrap();
    state.packuments += 1;
    let base = state.base.clone();
    let Some(package) = state.packages.get(&name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let versions: serde_json::Map<String, Value> = package
        .versions
        .iter()
        .map(|(version, published)| {
            let bytes = tarball(&published.files);
            let manifest: Value = published
                .files
                .iter()
                .find(|(path, _)| path == "package.json")
                .and_then(|(_, text)| serde_json::from_str(text).ok())
                .unwrap_or_default();
            let mut entry = manifest;
            entry["dist"] = json!({
                "tarball": format!("{base}/tarballs/{}", file_name(&name, version)),
                "integrity": published.integrity.clone().unwrap_or_else(|| integrity_of(&bytes)),
            });
            (version.clone(), entry)
        })
        .collect();
    axum::Json(json!({
        "name": name, "description": package.description,
        "dist-tags": { "latest": package.latest, "next": package.versions.keys().last() },
        "versions": versions,
    }))
    .into_response()
}

async fn download(State(state): State<Shared>, Path(file): Path<String>) -> Response {
    let state = state.lock().unwrap();
    for (name, package) in &state.packages {
        for (version, published) in &package.versions {
            if file_name(name, version) == file {
                return tarball(&published.files).into_response();
            }
        }
    }
    StatusCode::NOT_FOUND.into_response()
}
