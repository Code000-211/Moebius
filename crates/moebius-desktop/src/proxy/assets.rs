//! The web app of this release: `web/dist`, embedded in the app when it is built, and served as the server serves it
//! (`server::api::web` of `moebius-core`), with the same content security policies from `web/csp.json`: the file of
//! that name, or `index.html` for any other path, since the web app routes in the window.

use std::borrow::Cow;

use axum::http::{HeaderValue, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;

/// What `pnpm -C web build` wrote to `web/dist`; empty when the web app was not built before the app was.
#[derive(RustEmbed)]
// Relative to this crate's Cargo.toml.
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Dist;

/// Where the files of the web app come from: the embedded build, or a test's own files.
pub type Files = fn(&str) -> Option<Cow<'static, [u8]>>;

/// The embedded file at `path`, relative to `web/dist`.
pub fn embedded(path: &str) -> Option<Cow<'static, [u8]>> {
    Dist::get(path).map(|file| file.data)
}

const NOT_BUILT: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<title>Moebius</title></head><body><h1>The web app was not built</h1>\
<p>Run <code>pnpm -C web build</code>, then build the app again.</p></body></html>";

/// The two policies, `app` and `frame`, as `web/csp.json` says them.
fn policies() -> &'static serde_json::Value {
    static POLICIES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    POLICIES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../web/csp.json"))
            .expect("web/csp.json is JSON")
    })
}

/// The policy for the file called `name`; the app's names the WebSockets of `host`, the proxy's own address, as the
/// server's does for browsers that know only CSP level 2.
fn policy(name: &str, host: &str) -> String {
    let which = if name == "plugin-frame.html" {
        "frame"
    } else {
        "app"
    };
    let base = policies()[which].as_str().unwrap_or("default-src 'none'");
    if which == "app" {
        base.replacen(
            "connect-src 'self'",
            &format!("connect-src 'self' ws://{host}"),
            1,
        )
    } else {
        base.to_string()
    }
}

/// The answer to the `GET` of `path` from `files`, to the proxy at `host`.
pub fn serve(files: Files, path: &str, host: &str) -> Response {
    let name = path.trim_start_matches('/');
    let file = if name.is_empty() || name.split('/').any(|part| part == "..") {
        None
    } else {
        files(name).map(|bytes| (name, bytes))
    };
    let (name, bytes) = match file.or_else(|| files("index.html").map(|b| ("index.html", b))) {
        Some(found) => found,
        None => return Html(NOT_BUILT).into_response(),
    };
    let cache = if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut response = (
        [
            (header::CONTENT_TYPE, content_type(name)),
            (header::CACHE_CONTROL, cache),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes.into_owned(),
    )
        .into_response();
    let headers = response.headers_mut();
    if let Ok(value) = policy(name, host).parse() {
        headers.insert(header::CONTENT_SECURITY_POLICY, value);
    }
    // The frame of a plugin's page has no origin, and a font is only read across origins when it is allowed.
    if name.starts_with("fonts/") {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
    }
    response
}

/// The media type of the file called `name`, from its extension.
fn content_type(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
    match extension.to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(name: &str) -> Option<Cow<'static, [u8]>> {
        match name {
            "index.html" => Some(Cow::Borrowed(b"<!doctype html><title>Moebius</title>")),
            "plugin-frame.html" => Some(Cow::Borrowed(b"<!doctype html>")),
            "assets/app.js" => Some(Cow::Borrowed(b"export {}")),
            "fonts/inter.woff2" => Some(Cow::Borrowed(b"font")),
            _ => None,
        }
    }

    fn header_of(response: &Response, name: header::HeaderName) -> String {
        response.headers()[name].to_str().unwrap().to_string()
    }

    #[test]
    fn a_path_that_is_no_file_gets_the_index_and_assets_are_cached_for_long() {
        let host = "127.0.0.1:4000";
        let page = serve(files, "/board/demo", host);
        assert!(header_of(&page, header::CONTENT_TYPE).starts_with("text/html"));
        assert_eq!(header_of(&page, header::CACHE_CONTROL), "no-cache");
        let policy = header_of(&page, header::CONTENT_SECURITY_POLICY);
        assert!(
            policy.contains("connect-src 'self' ws://127.0.0.1:4000"),
            "{policy}"
        );
        let script = serve(files, "/assets/app.js", host);
        assert!(header_of(&script, header::CACHE_CONTROL).contains("immutable"));
        assert!(header_of(&script, header::CONTENT_TYPE).starts_with("text/javascript"));
        let escape = serve(files, "/assets/../../secret", host);
        assert!(header_of(&escape, header::CONTENT_TYPE).starts_with("text/html"));
    }

    #[test]
    fn the_frame_of_a_plugin_gets_its_own_policy_and_may_read_the_fonts() {
        let frame = serve(files, "/plugin-frame.html", "127.0.0.1:4000");
        let policy = header_of(&frame, header::CONTENT_SECURITY_POLICY);
        assert!(policy.contains("connect-src 'none'"), "{policy}");
        let font = serve(files, "/fonts/inter.woff2", "127.0.0.1:4000");
        assert_eq!(header_of(&font, header::ACCESS_CONTROL_ALLOW_ORIGIN), "*");
    }

    #[test]
    fn without_a_build_a_page_says_how_to_make_one() {
        let response = serve(|_| None, "/", "127.0.0.1:4000");
        assert!(header_of(&response, header::CONTENT_TYPE).starts_with("text/html"));
    }
}
