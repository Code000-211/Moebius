//! The sources a package is installed from, written as `pi install` takes them: `npm:<name>[@<version>]` and
//! `git:<host>/<owner>/<repo>[@<ref>]`, and the versions an npm source may ask for.

use semver::Version;

use super::PackagesError;

/// Where a package comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A package of the npm registry, and the version it asks for: nothing (the `latest` tag), a tag, a version, or a
    /// range written `^1.2.3` or `~1.2.3`.
    Npm { name: String, spec: String },
    /// A git repository and the branch, tag or commit it asks for, nothing for the default branch.
    Git {
        /// What follows `git:` as it was written.
        written: String,
        url: String,
        reference: Option<String>,
    },
}

/// The version an npm source asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wanted {
    /// The version a tag of the package names (`latest` unless the source says another).
    Tag(String),
    Exact(Version),
    /// `^x.y.z`: the same leftmost non-zero part.
    Caret(Version),
    /// `~x.y.z`: the same major and minor.
    Tilde(Version),
}

impl Source {
    /// Reads a source such as `npm:@acme/tools@^1.2.0` or `git:github.com/acme/tools@v1`.
    pub fn parse(text: &str) -> Result<Self, PackagesError> {
        let text = text.trim();
        if let Some(rest) = text.strip_prefix("npm:") {
            let (name, spec) = match rest.strip_prefix('@') {
                Some(scoped) => match scoped.split_once('@') {
                    Some((name, spec)) => (format!("@{name}"), spec),
                    None => (rest.to_string(), ""),
                },
                None => rest
                    .split_once('@')
                    .map_or((rest.to_string(), ""), |(n, s)| (n.to_string(), s)),
            };
            if !valid_package_name(&name) {
                return Err(PackagesError::Invalid(format!(
                    "{name:?} is not a valid npm package name"
                )));
            }
            let source = Source::Npm {
                name,
                spec: spec.trim().to_string(),
            };
            source.wanted()?;
            return Ok(source);
        }
        if let Some(rest) = text.strip_prefix("git:") {
            let (location, reference) = split_reference(rest);
            if location.is_empty()
                || location.starts_with('-')
                || location.contains(char::is_whitespace)
            {
                return Err(PackagesError::Invalid(format!(
                    "{rest:?} is not a git repository"
                )));
            }
            if reference.is_some_and(|r| r.is_empty() || r.starts_with('-')) {
                return Err(PackagesError::Invalid(format!(
                    "{rest:?} names no valid ref after @"
                )));
            }
            let url = if location.contains("://") || location.starts_with("git@") {
                location.to_string()
            } else {
                format!("https://{location}")
            };
            return Ok(Source::Git {
                written: rest.to_string(),
                url,
                reference: reference.map(str::to_string),
            });
        }
        Err(PackagesError::Invalid(format!(
            "{text:?} is not a source: write npm:<name>[@<version>] or git:<host>/<owner>/<repo>[@<ref>]"
        )))
    }

    /// The source as `packages.lock` and `pi install` write it.
    pub fn text(&self) -> String {
        match self {
            Source::Npm { name, spec } if spec.is_empty() => format!("npm:{name}"),
            Source::Npm { name, spec } => format!("npm:{name}@{spec}"),
            Source::Git { written, .. } => format!("git:{written}"),
        }
    }

    /// What an npm source asks for; an error for a version that is none of the forms.
    pub fn wanted(&self) -> Result<Wanted, PackagesError> {
        match self {
            Source::Npm { spec, .. } => Wanted::parse(spec),
            Source::Git { .. } => Err(PackagesError::Invalid("a git source has no version".into())),
        }
    }
}

/// The text of `rest` before a final `@ref`, and the ref. An `@` counts only after the last `/`, so that `git@host:x/y`
/// and `https://user@host/x/y` have none.
fn split_reference(rest: &str) -> (&str, Option<&str>) {
    let slash = rest.rfind('/').map_or(0, |i| i + 1);
    match rest[slash..].rfind('@') {
        Some(at) => (&rest[..slash + at], Some(&rest[slash + at + 1..])),
        None => (rest, None),
    }
}

impl Wanted {
    /// Reads the version of an npm source: nothing, a tag, a version, or a `^` or `~` range.
    pub fn parse(spec: &str) -> Result<Self, PackagesError> {
        let invalid = || {
            PackagesError::Invalid(format!(
                "{spec:?} is not a version: write a version, a ^ or ~ range, or a tag"
            ))
        };
        if spec.is_empty() {
            return Ok(Wanted::Tag("latest".into()));
        }
        if let Some(rest) = spec.strip_prefix('^') {
            return Version::parse(rest)
                .map(Wanted::Caret)
                .map_err(|_| invalid());
        }
        if let Some(rest) = spec.strip_prefix('~') {
            return Version::parse(rest)
                .map(Wanted::Tilde)
                .map_err(|_| invalid());
        }
        if let Ok(version) = Version::parse(spec.strip_prefix('=').unwrap_or(spec)) {
            return Ok(Wanted::Exact(version));
        }
        let tag = spec.starts_with(|c: char| c.is_ascii_alphabetic())
            && spec
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if tag {
            Ok(Wanted::Tag(spec.to_string()))
        } else {
            Err(invalid())
        }
    }

    /// Whether `version` is one this asks for. A tag asks for the version it names, which the registry resolves, and an
    /// exact version for that version; a range takes releases only, not prereleases.
    pub fn allows(&self, version: &Version) -> bool {
        match self {
            Wanted::Tag(_) => true,
            Wanted::Exact(exact) => exact == version,
            Wanted::Caret(base) => {
                version.pre.is_empty()
                    && version >= base
                    && if base.major > 0 {
                        version.major == base.major
                    } else if base.minor > 0 {
                        version.major == 0 && version.minor == base.minor
                    } else {
                        version.major == 0 && version.minor == 0 && version.patch == base.patch
                    }
            }
            Wanted::Tilde(base) => {
                version.pre.is_empty()
                    && version >= base
                    && version.major == base.major
                    && version.minor == base.minor
            }
        }
    }
}

/// Whether `name` is an npm package name this uses as a directory: lowercase letters, digits, `.`, `-` and `_`, not
/// starting with `.` or `_`, with an optional `@scope/` of the same.
pub fn valid_package_name(name: &str) -> bool {
    let part = |part: &str| {
        !part.is_empty()
            && part.len() <= 214
            && !part.starts_with(['.', '_'])
            && part.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_')
            })
    };
    match name.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(s, n)| part(s) && part(n)),
        None => part(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn npm_sources_have_a_name_and_a_version() {
        let parse = |text: &str| Source::parse(text).unwrap();
        assert_eq!(
            parse("npm:pi-web-access"),
            Source::Npm {
                name: "pi-web-access".into(),
                spec: String::new()
            }
        );
        assert_eq!(
            parse("npm:@thinkrail/pi-ask-user-question@^1.2.0"),
            Source::Npm {
                name: "@thinkrail/pi-ask-user-question".into(),
                spec: "^1.2.0".into()
            }
        );
        assert_eq!(parse("npm:@a/b").text(), "npm:@a/b");
        assert_eq!(parse("npm:a@1.0.0").text(), "npm:a@1.0.0");
        for bad in [
            "npm:",
            "npm:Upper",
            "npm:a@>=1",
            "npm:../x",
            "pi-mcp-adapter",
            "npm:@a",
            "./local",
        ] {
            assert!(Source::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn git_sources_have_a_url_and_a_ref() {
        let parse = |text: &str| Source::parse(text).unwrap();
        assert_eq!(
            parse("git:github.com/acme/tools@v1"),
            Source::Git {
                written: "github.com/acme/tools@v1".into(),
                url: "https://github.com/acme/tools".into(),
                reference: Some("v1".into())
            }
        );
        let Source::Git { url, reference, .. } = parse("git:file:///srv/tools.git@main") else {
            panic!()
        };
        assert_eq!(
            (url.as_str(), reference.as_deref()),
            ("file:///srv/tools.git", Some("main"))
        );
        let Source::Git { url, reference, .. } = parse("git:git@github.com:acme/tools.git") else {
            panic!()
        };
        assert_eq!(
            (url.as_str(), reference),
            ("git@github.com:acme/tools.git", None)
        );
        let Source::Git { url, reference, .. } = parse("git:https://user@host/acme/tools") else {
            panic!()
        };
        assert_eq!(
            (url.as_str(), reference),
            ("https://user@host/acme/tools", None)
        );
        assert!(Source::parse("git:-oops").is_err());
        assert!(Source::parse("git:").is_err());
        assert!(Source::parse("git:host/x@").is_err());
    }

    #[test]
    fn a_range_allows_the_releases_it_says() {
        let caret = Wanted::parse("^1.2.3").unwrap();
        assert!(caret.allows(&v("1.2.3")) && caret.allows(&v("1.9.0")));
        assert!(
            !caret.allows(&v("1.2.2"))
                && !caret.allows(&v("2.0.0"))
                && !caret.allows(&v("1.3.0-beta.1"))
        );
        let zero = Wanted::parse("^0.2.1").unwrap();
        assert!(zero.allows(&v("0.2.5")) && !zero.allows(&v("0.3.0")));
        let tilde = Wanted::parse("~1.2.3").unwrap();
        assert!(tilde.allows(&v("1.2.9")) && !tilde.allows(&v("1.3.0")));
        let exact = Wanted::parse("1.0.0").unwrap();
        assert!(exact.allows(&v("1.0.0")) && !exact.allows(&v("1.0.1")));
        assert_eq!(Wanted::parse("").unwrap(), Wanted::Tag("latest".into()));
        assert_eq!(Wanted::parse("next").unwrap(), Wanted::Tag("next".into()));
        assert!(Wanted::parse(">=1.0.0").is_err());
    }
}
