//! The web app: the files built in `web/dist`, embedded in the binary and served at `/` from the address of the API.
//!
//! Every `GET` of a path outside `/v1/` is answered here, without a token (the page holds no data; the data is behind
//! `/v1/`): the file of that name, or `index.html` for any other path, since the app routes in the browser. Without a
//! build of the web app the binary embeds nothing, and every such path answers a short page that says how to build it.
//! [`serve`] takes the files as a function, so the tests give it their own.

use std::borrow::Cow;

use axum::http::{Method, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;

/// What `pnpm -C web build` wrote to `web/dist`; empty when the web app was not built before the binary was.
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../web/dist"]
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

const NOT_BUILT: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<title>Moebius</title></head><body><h1>The web app was not built</h1>\
<p>Run <code>pnpm -C web build</code>, then build the server again.</p></body></html>";

/// Answers every request no route of the API matched: the web app, or a `404` under `/v1/`.
pub(super) async fn fallback(method: Method, uri: Uri) -> Response {
    let path = uri.path();
    if path == "/v1" || path.starts_with("/v1/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    serve(&embedded, path)
}

/// The response for the `GET` of `path` with the files `files` gives.
pub(super) fn serve(files: &dyn Fn(&str) -> Option<Asset>, path: &str) -> Response {
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
    (
        [
            (header::CONTENT_TYPE, content_type(name)),
            (header::CACHE_CONTROL, cache),
        ],
        asset.bytes.into_owned(),
    )
        .into_response()
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

    #[tokio::test]
    async fn the_index_is_served_at_the_root_and_never_cached() {
        let response = serve(&built, "/");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-cache");
        assert!(header_of(&response, header::CONTENT_TYPE).starts_with("text/html"));
        assert_eq!(body(response).await, "<html>app</html>");
    }

    #[tokio::test]
    async fn files_get_their_media_type_and_assets_are_cached_for_long() {
        let script = serve(&built, "/assets/app-1a2b.js");
        assert!(header_of(&script, header::CONTENT_TYPE).starts_with("text/javascript"));
        assert!(header_of(&script, header::CACHE_CONTROL).contains("immutable"));
        let icon = serve(&built, "/favicon.svg");
        assert_eq!(header_of(&icon, header::CONTENT_TYPE), "image/svg+xml");
        assert_eq!(header_of(&icon, header::CACHE_CONTROL), "no-cache");
    }

    #[tokio::test]
    async fn a_path_that_is_no_file_gets_the_index_for_the_app_to_route() {
        for path in ["/cards/3", "/assets/missing.js", "/../secret"] {
            let response = serve(&built, path);
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(body(response).await, "<html>app</html>", "{path}");
        }
    }

    #[tokio::test]
    async fn without_a_build_a_page_says_how_to_make_one() {
        for path in ["/", "/cards/3"] {
            let response = serve(&|_| None, path);
            assert_eq!(response.status(), StatusCode::OK);
            assert!(header_of(&response, header::CONTENT_TYPE).starts_with("text/html"));
            assert!(body(response).await.contains("pnpm -C web build"));
        }
    }
}
