//! The Tauri side of the app: the one window, without a system frame, that shows the connect page and then the web app
//! through a server's proxy (the rail of servers is in `servers`). It is part of the program rather than of the
//! library, so that the library's tests do not link Tauri (on Windows a test executable without the program's
//! manifest cannot load what Tauri needs from the system).
//!
//! Every page of the window gets [`BRIDGE`] before its own scripts: `window.moebiusDesktop`, what the web app calls to
//! move the window, since it draws the window's controls on its own ground. The bridge calls Tauri's window commands,
//! which `capabilities/web-app.json` allows the web app on the loopback, and the rail's commands (`servers`), which both
//! capabilities allow. An address the window does not keep (`links::target`) opens in the system's
//! browser.

use std::sync::{Arc, Mutex};

use moebius_desktop::links::{self, Target};
use tauri::webview::NewWindowResponse;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use crate::servers::{self, Servers};

/// The label of the main window, which the capabilities name.
pub const MAIN: &str = "main";

/// What every page of the window gets first: `window.moebiusDesktop`, the window's controls for the web app.
pub const BRIDGE: &str = r#"
(() => {
  const invoke = (command) => window.__TAURI_INTERNALS__.invoke(command, { label: "main" });
  Object.defineProperty(window, "moebiusDesktop", {
    value: Object.freeze({
      minimize: () => invoke("plugin:window|minimize"),
      toggleMaximize: () => invoke("plugin:window|toggle_maximize"),
      close: () => invoke("plugin:window|close"),
      // The rail of servers (no command returns a token).
      servers: () => window.__TAURI_INTERNALS__.invoke("servers"),
      switchServer: (id) => window.__TAURI_INTERNALS__.invoke("switch_server", { id }),
      removeServer: (id) => window.__TAURI_INTERNALS__.invoke("remove_server", { id }),
      connect: (address, token) => window.__TAURI_INTERNALS__.invoke("connect", { address, token }),
      connectSsh: (host, setUp) => window.__TAURI_INTERNALS__.invoke("connect_ssh", { host, setUp }),
    }),
  });
})();
"#;

/// The origin of the web app the window shows, once a server is connected; the window's handlers read it.
#[derive(Clone, Default)]
pub struct Connected(Arc<Mutex<Option<String>>>);

impl Connected {
    fn origin(&self) -> Option<String> {
        self.0.lock().ok()?.clone()
    }

    /// Sets the origin of the server now shown, or none when the window shows the connect page.
    pub fn set(&self, origin: Option<String>) {
        if let Ok(mut current) = self.0.lock() {
            *current = origin;
        }
    }
}

/// Opens `url` in the system's browser; a browser that does not start leaves the window as it is.
fn open_in_browser(url: &str) {
    let _ = tauri_plugin_opener::open_url(url, None::<&str>);
}

/// Runs the app until its window is closed.
pub fn run() {
    let connected = Connected::default();
    tauri::Builder::default()
        .manage(connected.clone())
        .invoke_handler(tauri::generate_handler![
            servers::connect,
            servers::connect_ssh,
            servers::servers,
            servers::switch_server,
            servers::remove_server,
            servers::local_state,
            servers::local_connect
        ])
        .setup(move |app| {
            let navigating = connected.clone();
            let opening = connected.clone();
            let handle = app.handle().clone();
            let window = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
                .title("Moebius")
                .inner_size(1280.0, 800.0)
                .min_inner_size(720.0, 480.0)
                .decorations(false)
                .initialization_script(BRIDGE)
                .on_navigation(move |url| {
                    match links::target(url.as_str(), navigating.origin().as_deref()) {
                        Target::Window => true,
                        Target::Browser => {
                            open_in_browser(url.as_str());
                            false
                        }
                        Target::Nowhere => false,
                    }
                })
                .on_new_window(move |url, _| {
                    match links::target(url.as_str(), opening.origin().as_deref()) {
                        // A page of the web app asked for a window of its own: the one window shows it.
                        Target::Window => {
                            if let Some(window) = handle.get_webview_window(MAIN) {
                                let _ = window.navigate(url);
                            }
                        }
                        Target::Browser => open_in_browser(url.as_str()),
                        Target::Nowhere => {}
                    }
                    NewWindowResponse::Deny
                })
                .build()?;
            // The rail of servers: the window's first page is the connect page, until a remembered server opens.
            let servers = Servers::new(app.handle());
            servers.note_connect_page(window.url()?);
            app.manage(servers);
            tauri::async_runtime::spawn(servers::open_remembered(app.handle().clone()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the desktop app runs");
}
