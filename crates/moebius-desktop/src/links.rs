//! Where an address the web app opens goes: the window keeps its own pages (the connect page and the web app on the
//! proxy's origin), and sends everything else, such as a pull request on a forge or a link in a chat, to the system's
//! browser. The window has no frame of the system, so a page of another site in it could not be closed or left.

/// Where an address goes.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// The window shows it.
    Window,
    /// The system's browser opens it, and the window stays where it is.
    Browser,
    /// It is not opened at all, such as a `file:` address.
    Nowhere,
}

/// Where `url` goes while the window shows the web app of the proxy at `origin` (`http://127.0.0.1:<port>`), or the
/// connect page when no server is connected.
pub fn target(url: &str, origin: Option<&str>) -> Target {
    let (scheme, rest) = url.split_once(':').unwrap_or(("", url));
    let host = rest
        .trim_start_matches("//")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let ours = origin.is_some_and(|origin| {
        url == origin
            || url
                .strip_prefix(origin)
                .is_some_and(|rest| rest.starts_with(['/', '?', '#']))
    });
    match scheme.to_ascii_lowercase().as_str() {
        // The app's own pages: `tauri://localhost` on Linux, `http://tauri.localhost` on Windows.
        "tauri" => Target::Window,
        "http" | "https" if host == "tauri.localhost" || ours => Target::Window,
        // What the web app's own frames load: a plugin's page and its scripts.
        "about" | "blob" | "data" => Target::Window,
        "http" | "https" | "mailto" => Target::Browser,
        _ => Target::Nowhere,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: Option<&str> = Some("http://127.0.0.1:41234");

    #[test]
    fn the_window_keeps_the_web_app_and_its_own_pages() {
        for url in [
            "http://127.0.0.1:41234/",
            "http://127.0.0.1:41234/board/3?card=4",
            "http://127.0.0.1:41234",
            "tauri://localhost/index.html",
            "http://tauri.localhost/index.html",
            "about:blank",
            "blob:http://127.0.0.1:41234/5a1c",
        ] {
            assert_eq!(target(url, ORIGIN), Target::Window, "{url}");
        }
    }

    #[test]
    fn everything_else_opens_in_the_browser_or_not_at_all() {
        for url in [
            "https://github.com/acme/shop/pull/19",
            "http://127.0.0.1:41235/",
            "http://127.0.0.1:412345/",
            "http://127.0.0.1:41234.evil.example/",
            "mailto:someone@example.com",
        ] {
            assert_eq!(target(url, ORIGIN), Target::Browser, "{url}");
        }
        // Before a server is connected, no address on the loopback is the web app.
        assert_eq!(target("http://127.0.0.1:41234/", None), Target::Browser);
        for url in ["file:///etc/passwd", "javascript:alert(1)", "smb://share"] {
            assert_eq!(target(url, ORIGIN), Target::Nowhere, "{url}");
        }
    }
}
