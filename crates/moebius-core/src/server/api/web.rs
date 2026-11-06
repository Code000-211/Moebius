//! The web app: the files built in `web/dist`, embedded in the binary and served at `/` from the address of the API.
//!
//! Every `GET` of a path outside `/v1/` is answered here, without a token (the page holds no data; the data is behind
//! `/v1/`): the file of that name, or `index.html` for any other path, since the app routes in the browser. Without a
//! build of the web app the binary embeds nothing, and every such path answers a short page that says how to build it.
//! [`serve`] takes the files as a function, so the tests give it their own.
//!
//! Every file is answered with a content security policy (`web/csp.json`, which the end-to-end tests' fake server reads
//! too): the app may load scripts, styles, fonts and images from its own address only, talk to its own address, and
//! frame only its own address, and the frame a plugin's page runs in (`plugin-frame.html`, sandboxed by the app) gets a
//! policy of its own that forbids it every network request. The fonts are readable from that frame, which has no origin.

use std::borrow::Cow;

use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;

/// What `pnpm -C web build` wrote to `web/dist`; empty when the web app was not built before the binary was.
#[derive(RustEmbed)]
// Relative to this crate's Cargo.toml (rust-embed does not expand `$CARGO_MANIFEST_DIR` without a feature).
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Dist;

/// A file of the web app.
pub(super) struct Asset {
    pub bytes: Cow<'static, [u8]>,
}

/// The embedded file at `path`, relative to `web/dist`.
fn embedded(path: &str) -> Option<Asset> {
    Dist::get(path).map(|file| Asset { bytes: file.data })
}

/// The two policies, `app` and `frame`, as `web/csp.json` says them.
fn policies() -> &'static serde_json::Value {
    static POLICIES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    POLICIES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../../../web/csp.json"))
            .expect("web/csp.json is JSON")
    })
}

/// Whether `host` is only a hostname or address with an optional port, so that it is safe to write into a policy.
fn is_plain_host(host: &str) -> bool {
    let (name, port) = if host.starts_with('[') {
        match host.split_once(']') {
            Some((v6, rest)) => (
                &v6[1..],
                match rest {
                    "" => None,
                    _ => match rest.strip_prefix(':') {
                        Some(port) => Some(port),
                        None => return false,
                    },
                },
            ),
            None => return false,
        }
    } else {
        match host.split_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (host, None),
        }
    };
    let name_ok = if host.starts_with('[') {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
    } else {
        !name.is_empty()
            && name.len() <= 253
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    };
    let port_ok =
        port.is_none_or(|p| !p.is_empty() && p.len() <= 5 && p.chars().all(|c| c.is_ascii_digit()));
    name_ok && port_ok
}

/// The content security policy for the file called `name`. The app's also names the WebSocket schemes of `host` (the
/// `Host` of the request) in `connect-src`: browsers that know only CSP level 2 do not match `ws:` and `wss:` with
/// `'self'`, and the live stream and the terminals are WebSockets. A `host` that is not plainly a host is not written.
fn policy(name: &str, host: Option<&str>) -> String {
    let which = if name == "plugin-frame.html" {
        "frame"
    } else {
        "app"
    };
    let base = policies()[which]
        .as_str()
        .unwrap_or("default-src 'none'")
        .to_string();
    match host.filter(|host| which == "app" && is_plain_host(host)) {
        Some(host) => base.replacen(
            "connect-src 'self'",
            &format!("connect-src 'self' ws://{host} wss://{host}"),
            1,
        ),
        None => base,
    }
}

const NOT_BUILT: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<title>Moebius</title></head><body><h1>The web app was not built</h1>\
<p>Run <code>pnpm -C web build</code>, then build the server again.</p></body></html>";

/// Answers every request no route of the API matched: the web app, or a `404` under `/v1/`.
pub(super) async fn fallback(method: Method, uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path();
    if path == "/v1" || path.starts_with("/v1/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    serve(
        &embedded,
        path,
        headers
            .get(header::HOST)
            .and_then(|host| host.to_str().ok()),
    )
}

/// The response for the `GET` of `path` with the files `files` gives, to a request with the `Host` header `host`.
pub(super) fn serve(
    files: &dyn Fn(&str) -> Option<Asset>,
    path: &str,
    host: Option<&str>,
) -> Response {
    let name = path.trim_start_matches('/');
    let file = if name.is_empty() || name.split('/').any(|part| part == "..") {
        None
    } else {
        files(name).map(|asset| (name, asset))
    };
    let (name, asset) = match file {
        Some(found) => found,
        None => match files("index.html") {
            Some(index) => ("index.html", index),
            None => return Html(NOT_BUILT).into_response(),
        },
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
        asset.bytes.into_owned(),
    )
        .into_response();
    let headers = response.headers_mut();
    if let Ok(value) = policy(name, host).parse() {
        headers.insert(header::CONTENT_SECURITY_POLICY, value);
    }
    // The frame of a plugin's page has no origin, and a font is only read across origins when the server allows it.
    if name.starts_with("fonts/") {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            header::HeaderValue::from_static("*"),
        );
    }
    response
}

/// The media type of the file called `name`, from its extension.
fn content_type(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
    match extension.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_of(response: &Response, name: header::HeaderName) -> String {
        response.headers()[name].to_str().unwrap().to_string()
    }

    async fn body(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn built(name: &str) -> Option<Asset> {
        let content: &'static [u8] = match name {
            "index.html" => b"<html>app</html>",
            "assets/app-1a2b.js" => b"console.log(1)",
            "favicon.svg" => b"<svg/>",
            _ => return None,
        };
        Some(Asset {
            bytes: Cow::Borrowed(content),
        })
    }

    fn built_with_frame(name: &str) -> Option<Asset> {
        let content: &'static [u8] = match name {
            "plugin-frame.html" => b"<html>frame</html>",
            "fonts/inter/Inter.woff2" => b"font",
            _ => return built(name),
        };
        Some(Asset {
            bytes: Cow::Borrowed(content),
        })
    }

    #[tokio::test]
    async fn every_file_has_the_policy_of_the_app_and_the_frame_of_a_plugin_its_own() {
        for path in ["/", "/assets/app-1a2b.js", "/cards/3", "/favicon.svg"] {
            let response = serve(&built_with_frame, path, None);
            let policy = header_of(&response, header::CONTENT_SECURITY_POLICY);
            assert!(policy.contains("script-src 'self';"), "{path}: {policy}");
            assert!(!policy.contains("unsafe-eval"), "{path}: {policy}");
            assert!(
                !policy.contains("script-src 'self' 'unsafe-inline'"),
                "{path}: {policy}"
            );
            assert!(
                policy.contains("frame-src 'self'") && policy.contains("frame-ancestors 'none'")
            );
            assert_eq!(
                header_of(&response, header::X_CONTENT_TYPE_OPTIONS),
                "nosniff"
            );
        }
        let frame = serve(&built_with_frame, "/plugin-frame.html", None);
        let policy = header_of(&frame, header::CONTENT_SECURITY_POLICY);
        assert!(policy.contains("default-src 'none'") && policy.contains("connect-src 'none'"));
        assert!(
            policy.contains("script-src 'self' blob:") && policy.contains("frame-ancestors 'self'")
        );
        // The frame has no origin, so only the fonts are opened to it.
        let font = serve(&built_with_frame, "/fonts/inter/Inter.woff2", None);
        assert_eq!(header_of(&font, header::ACCESS_CONTROL_ALLOW_ORIGIN), "*");
        assert!(
            frame
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
        assert!(
            serve(&built_with_frame, "/", None)
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
    }

    #[tokio::test]
    async fn the_policy_of_the_app_names_the_websockets_of_the_host_it_was_asked_at_and_only_a_plain_host()
     {
        for host in [
            "127.0.0.1:4100",
            "localhost",
            "example.com:8080",
            "[::1]:4100",
        ] {
            let response = serve(&built_with_frame, "/", Some(host));
            let policy = header_of(&response, header::CONTENT_SECURITY_POLICY);
            assert!(
                policy.contains(&format!("connect-src 'self' ws://{host} wss://{host};")),
                "{host}: {policy}"
            );
        }
        let base = header_of(
            &serve(&built_with_frame, "/", None),
            header::CONTENT_SECURITY_POLICY,
        );
        for host in [
            "",
            "evil.com; script-src *",
            "a b",
            "x.com:80:80",
            "host:port",
            "ho'st",
            "a,b",
            "[::1",
            "h:",
        ] {
            let response = serve(&built_with_frame, "/", Some(host));
            assert_eq!(
                header_of(&response, header::CONTENT_SECURITY_POLICY),
                base,
                "{host:?}"
            );
        }
        // The frame of a plugin stays closed whatever the host.
        let frame = serve(
            &built_with_frame,
            "/plugin-frame.html",
            Some("127.0.0.1:4100"),
        );
        let policy = header_of(&frame, header::CONTENT_SECURITY_POLICY);
        assert!(policy.contains("connect-src 'none'") && !policy.contains("ws://"));
    }

    #[tokio::test]
    async fn the_index_is_served_at_the_root_and_never_cached() {
        let response = serve(&built, "/", None);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-cache");
        assert!(header_of(&response, header::CONTENT_TYPE).starts_with("text/html"));
        assert_eq!(body(response).await, "<html>app</html>");
    }

    #[tokio::test]
    async fn files_get_their_media_type_and_assets_are_cached_for_long() {
        let script = serve(&built, "/assets/app-1a2b.js", None);
        assert!(header_of(&script, header::CONTENT_TYPE).starts_with("text/javascript"));
        assert!(header_of(&script, header::CACHE_CONTROL).contains("immutable"));
        let icon = serve(&built, "/favicon.svg", None);
        assert_eq!(header_of(&icon, header::CONTENT_TYPE), "image/svg+xml");
        assert_eq!(header_of(&icon, header::CACHE_CONTROL), "no-cache");
    }

    #[tokio::test]
    async fn a_path_that_is_no_file_gets_the_index_for_the_app_to_route() {
        for path in ["/cards/3", "/assets/missing.js", "/../secret"] {
            let response = serve(&built, path, None);
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(body(response).await, "<html>app</html>", "{path}");
        }
    }

    #[tokio::test]
    async fn without_a_build_a_page_says_how_to_make_one() {
        for path in ["/", "/cards/3"] {
            let response = serve(&|_| None, path, None);
            assert_eq!(response.status(), StatusCode::OK);
            assert!(header_of(&response, header::CONTENT_TYPE).starts_with("text/html"));
            assert!(body(response).await.contains("pnpm -C web build"));
        }
    }
}
