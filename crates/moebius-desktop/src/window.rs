//! The Tauri side of the app: the one window, without a system frame, that shows the connect page and then the web app
//! through a server's proxy (the rail of servers is in `servers`, the window's webviews in `pages`). It is part of the program rather than of the
//! library, so that the library's tests do not link Tauri (on Windows a test executable without the program's
//! manifest cannot load what Tauri needs from the system).
//!
//! Every page of the window gets [`BRIDGE`] before its own scripts: `window.moebiusDesktop`, what the web app calls to
//! move the window, since it draws the window's controls on its own ground. The bridge calls Tauri's window commands,
//! which `capabilities/web-app.json` allows the web app on the loopback, and the rail's commands (`servers`), which both
//! capabilities allow. An address the window does not keep (`links::target`) opens in the system's
//! browser. The tray icon (`tray_icon`) is made with the window and follows the server that is open.

use std::sync::{Arc, Mutex};

use moebius_desktop::instance::{self, Start};
use tauri::Manager;
use tauri::window::WindowBuilder;

use crate::pages;
use crate::quick_window;
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
      renameServer: (id, name) => window.__TAURI_INTERNALS__.invoke("rename_server", { id, name }),
      connect: (address, token, sessionOnly) =>
        window.__TAURI_INTERNALS__.invoke("connect", { address, token, sessionOnly }),
      connectSsh: (host, setUp) => window.__TAURI_INTERNALS__.invoke("connect_ssh", { host, setUp }),
    }),
  });
})();
"#;

/// The origin of the web app the window shows, once a server is connected; the window's handlers read it.
#[derive(Clone, Default)]
pub struct Connected(Arc<Mutex<Option<String>>>);

impl Connected {
    pub(crate) fn origin(&self) -> Option<String> {
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
pub(crate) fn open_in_browser(url: &str) {
    let _ = tauri_plugin_opener::open_url(url, None::<&str>);
}

/// Runs the app until its window is closed.
pub fn run() {
    let connected = Connected::default();
    tauri::Builder::default()
        .manage(connected)
        .manage(pages::Pages::default())
        .manage(crate::notifications::Watching::default())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            servers::connect,
            servers::connect_ssh,
            servers::startup_notice,
            servers::servers,
            servers::switch_server,
            servers::remove_server,
            servers::rename_server,
            servers::local_state,
            servers::local_connect,
            quick_window::quick_shortcut,
            quick_window::set_quick_shortcut,
            quick_window::quick_finish
        ])
        .setup(move |app| {
            // The app runs once: a later start has asked the first to show its window, and ends before it makes a
            // tray icon or takes the shortcut. A lock that cannot be made does not keep the app from starting.
            let dir = app.path().app_config_dir()?;
            match instance::acquire(&dir) {
                Ok(Start::Second) => std::process::exit(0),
                Ok(Start::First(instance)) => {
                    let shown = app.handle().clone();
                    std::thread::spawn(move || {
                        instance.wait(|| {
                            crate::tray_icon::bring_up(&shown);
                        });
                    });
                }
                Err(e) => eprintln!("moebius-desktop: cannot check for a running app: {e}"),
            }
            crate::tray_icon::create(app)?;
            quick_window::setup(app);
            let window = WindowBuilder::new(app, MAIN)
                .title("Moebius")
                .inner_size(1280.0, 800.0)
                .min_inner_size(720.0, 480.0)
                .decorations(false)
                .build()?;
            // The window's first page is the connect page, until a remembered server opens; each server opened gets a
            // webview of its own (`pages`).
            pages::make_connect(app.handle(), &window)?;
            app.manage(Servers::new(app.handle()));
            // The app ends with its main window, though the small window of the shortcut is only ever hidden.
            let exiting = app.handle().clone();
            window.on_window_event(move |event| {
                if matches!(event, tauri::WindowEvent::Destroyed) {
                    exiting.exit(0);
                }
            });
            tauri::async_runtime::spawn(servers::open_remembered(app.handle().clone()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the desktop app runs");
}
