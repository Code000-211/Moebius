//! The Tauri side of the app: the one window, without a system frame, that shows the connect page and then the web app
//! through the proxy, and the `connect` command the connect page calls. It is part of the program rather than of the
//! library, so that the library's tests do not link Tauri (on Windows a test executable without the program's
//! manifest cannot load what Tauri needs from the system).
//!
//! Every page of the window gets [`BRIDGE`] before its own scripts: `window.moebiusDesktop`, what the web app calls to
//! move the window, since it draws the window's controls on its own ground. The bridge calls Tauri's window commands,
//! which `capabilities/web-app.json` allows the web app on the loopback; `connect` is allowed to the connect page only
//! (`capabilities/connect.json`). An address the window does not keep (`links::target`) opens in the system's
//! browser.

use std::sync::{Arc, Mutex};

use moebius_desktop::links::{self, Target};
use moebius_desktop::proxy::{self, Proxy};
use moebius_desktop::server::{self, Server};
use tauri::webview::NewWindowResponse;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

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
    }),
  });
})();
"#;

/// The proxy of the server the window shows, once one is connected; the window's handlers read its origin.
#[derive(Clone, Default)]
struct Connected(Arc<Mutex<Option<Proxy>>>);

impl Connected {
    fn origin(&self) -> Option<String> {
        self.0.lock().ok()?.as_ref().map(Proxy::origin)
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
        .invoke_handler(tauri::generate_handler![connect])
        .setup(move |app| {
            let navigating = connected.clone();
            let opening = connected.clone();
            let handle = app.handle().clone();
            WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the desktop app runs");
}

/// Connects to the server at `address` with the access token `token`: checks it, starts the proxy and shows the web app
/// in the window. A server it refuses leaves the window on the connect page with the reason.
#[tauri::command]
async fn connect(app: tauri::AppHandle, address: String, token: String) -> Result<(), String> {
    let server = Server::new(&address, &token);
    server::check(&server).await.map_err(|e| e.to_string())?;
    let proxy = proxy::start(server, proxy::embedded)
        .await
        .map_err(|e| format!("cannot start the app's own address: {e}"))?;
    let url = proxy.entry_url().parse().map_err(|e| format!("{e}"))?;
    let window = app.get_webview_window(MAIN).ok_or("the window is gone")?;
    // The proxy of an earlier server is dropped, which stops it and everything it relays.
    let connected = app.state::<Connected>();
    *connected
        .0
        .lock()
        .map_err(|_| "the app's state is broken")? = Some(proxy);
    window.navigate(url).map_err(|e| e.to_string())
}
