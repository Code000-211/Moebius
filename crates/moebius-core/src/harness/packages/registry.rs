//! The npm registry (or a mirror, `[packages] npm_registry`): the search that makes up the catalog of pi.dev, the
//! metadata of a package (its packument) and its tarballs. The catalog is every package with the keyword `pi-package`,
//! so a search is the registry's own with that keyword added.

use std::io::Read;
use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::PackagesError;
use super::source::Wanted;
use crate::server::setup::without_credentials;

/// The largest tarball that is downloaded.
const TARBALL_LIMIT: u64 = 100 * 1024 * 1024;

/// The kinds of package the catalog tells apart, by the keywords a package has (`pi-extension` or `extension`).
pub const KINDS: [&str; 4] = ["extension", "skill", "theme", "prompt"];

/// A package of the catalog, as a search finds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub name: String,
    pub version: String,
    pub description: String,
    /// What the package is, from its keywords: any of `extension`, `skill`, `theme` and `prompt`.
    pub kinds: Vec<String>,
    pub weekly_downloads: Option<u64>,
    /// The registry's page of the package, and its homepage and repository when it names them.
    pub npm: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    /// When this version was published.
    pub date: Option<String>,
}

/// The version of a package a source resolves to, with what the registry says about its tarball.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub version: String,
    pub tarball: String,
    /// The `sha512-...` token of `dist.integrity`.
    pub integrity: String,
    /// The version's entry of the packument: its `package.json` as published.
    pub manifest: Value,
}

pub struct Registry {
    base: String,
    client: reqwest::blocking::Client,
}

impl Registry {
    /// A client of the registry at `base`.
    pub fn new(base: &str) -> Result<Self, PackagesError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent(concat!("moebius/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| PackagesError::Registry(e.without_url().to_string()))?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            client,
        })
    }

    fn get(
        &self,
        url: reqwest::Url,
        what: &str,
    ) -> Result<reqwest::blocking::Response, PackagesError> {
        let shown = without_credentials(url.as_str());
        let response = self.client.get(url).send().map_err(|e| {
            PackagesError::Registry(format!("cannot reach {shown}: {}", e.without_url()))
        })?;
        match response.status().as_u16() {
            200 => Ok(response),
            404 => Err(PackagesError::NotFound(what.to_string())),
            status => Err(PackagesError::Registry(format!(
                "{shown} answered {status}"
            ))),
        }
    }

    fn json(&self, url: reqwest::Url, what: &str) -> Result<Value, PackagesError> {
        self.get(url, what)?.json().map_err(|e| {
            PackagesError::Registry(format!("{what} is not JSON: {}", e.without_url()))
        })
    }

    fn url(&self, path: &str) -> Result<reqwest::Url, PackagesError> {
        reqwest::Url::parse(&format!("{}/{path}", self.base)).map_err(|_| {
            PackagesError::Registry(format!(
                "{} is not a registry URL",
                without_credentials(&self.base)
            ))
        })
    }

    /// The packages of the catalog that match `text` (every package for no text), of the kind `kind` if given, at most
    /// `limit`, in the order of the registry's own relevance.
    pub fn search(
        &self,
        text: &str,
        kind: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, PackagesError> {
        if let Some(kind) = kind
            && !KINDS.contains(&kind)
        {
            return Err(PackagesError::Invalid(format!(
                "{kind:?} is not a type of package: {}",
                KINDS.join(", ")
            )));
        }
        let mut url = self.url("-/v1/search")?;
        let size = if kind.is_some() {
            250
        } else {
            limit.clamp(1, 250)
        };
        url.query_pairs_mut()
            .append_pair(
                "text",
                format!("keywords:pi-package {}", text.trim()).trim(),
            )
            .append_pair("size", &size.to_string());
        let answer = self.json(url, "the search")?;
        let hits = answer["objects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|object| {
                let package = &object["package"];
                let text = |value: &Value| value.as_str().map(String::from);
                let mut kinds: Vec<String> = package["keywords"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|keyword| keyword.strip_prefix("pi-").unwrap_or(keyword))
                    .filter(|keyword| KINDS.contains(keyword))
                    .map(String::from)
                    .collect();
                kinds.sort();
                kinds.dedup();
                Some(SearchHit {
                    name: text(&package["name"])?,
                    version: text(&package["version"]).unwrap_or_default(),
                    description: text(&package["description"]).unwrap_or_default(),
                    kinds,
                    weekly_downloads: object["downloads"]["weekly"].as_u64(),
                    npm: text(&package["links"]["npm"]),
                    homepage: text(&package["links"]["homepage"]),
                    repository: text(&package["links"]["repository"]),
                    date: text(&package["date"]),
                })
            });
        Ok(hits
            .filter(|hit| kind.is_none_or(|kind| hit.kinds.iter().any(|k| k == kind)))
            .take(limit)
            .collect())
    }

    /// The version of `name` that `wanted` asks for: the version a tag names, or the newest release a range allows.
    pub fn resolve(&self, name: &str, wanted: &Wanted) -> Result<Resolved, PackagesError> {
        let packument = self.json(
            self.url(&name.replace('/', "%2F"))?,
            &format!("package {name}"),
        )?;
        let versions = packument["versions"].as_object();
        let known = |version: &str| versions.is_some_and(|versions| versions.contains_key(version));
        let version = match wanted {
            Wanted::Tag(tag) => packument["dist-tags"][tag.as_str()]
                .as_str()
                .map(String::from)
                .ok_or_else(|| PackagesError::NotFound(format!("the tag {tag} of {name}")))?,
            Wanted::Exact(exact) => Some(exact.to_string())
                .filter(|version| known(version))
                .ok_or_else(|| PackagesError::NotFound(format!("{name}@{exact}")))?,
            range => versions
                .into_iter()
                .flat_map(|versions| versions.keys())
                .filter_map(|key| Some((Version::parse(key).ok()?, key)))
                .filter(|(version, _)| range.allows(version))
                .max()
                .map(|(_, key)| key.clone())
                .ok_or_else(|| PackagesError::NotFound(format!("a version of {name} that fits")))?,
        };
        let manifest = packument["versions"][version.as_str()].clone();
        if manifest.is_null() {
            return Err(PackagesError::NotFound(format!("{name}@{version}")));
        }
        let dist = &manifest["dist"];
        let tarball = dist["tarball"]
            .as_str()
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            .ok_or_else(|| PackagesError::Registry(format!("{name}@{version} has no tarball")))?;
        let integrity = dist["integrity"]
            .as_str()
            .and_then(|all| {
                all.split_whitespace()
                    .find(|token| token.starts_with("sha512-"))
            })
            .ok_or_else(|| {
                PackagesError::Registry(format!(
                    "{name}@{version} has no sha512 integrity, so its download cannot be checked"
                ))
            })?;
        Ok(Resolved {
            version,
            tarball: tarball.to_string(),
            integrity: integrity.to_string(),
            manifest,
        })
    }

    /// The bytes of the tarball at `url`, at most [`TARBALL_LIMIT`].
    pub fn download(&self, url: &str) -> Result<Vec<u8>, PackagesError> {
        let url = reqwest::Url::parse(url)
            .map_err(|_| PackagesError::Registry(format!("{url:?} is not a tarball URL")))?;
        let shown = without_credentials(url.as_str());
        let mut bytes = vec![];
        self.get(url, "the tarball")?
            .take(TARBALL_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| PackagesError::Registry(format!("cannot read {shown}: {e}")))?;
        if bytes.len() as u64 > TARBALL_LIMIT {
            return Err(PackagesError::Registry(format!(
                "{shown} is larger than {TARBALL_LIMIT} bytes"
            )));
        }
        Ok(bytes)
    }

    /// The registry's address for `npm install --registry`.
    pub fn base(&self) -> &str {
        &self.base
    }
}
