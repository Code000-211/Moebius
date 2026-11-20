//! The Tauri side of the global shortcut: registering it, the small window it opens over everything, and the commands
//! that window calls (what the shortcut is, changing it, and finishing). What is decided without a window, such as
//! which key combinations are accepted, where the choice is kept and what the main window is told to open, is the
//! library's `quick`.
//!
//! The small window is the web app's `/quick` page on the proxy of the connected server, in a window without a frame
//! that stays over the others. It is made the first time the shortcut is pressed and then kept hidden, so it opens at
//! once; it is made again for a server of another address. Pressing the shortcut while it is shown closes it. With no
//! server connected the shortcut brings the main window with the connect page instead, as there is nothing to ask yet.
//! A shortcut another program already holds is not registered: the window says so and a person picks another.

use std::sync::Mutex;

use moebius_desktop::links::{self, Target};
use moebius_desktop::quick::{self, PAGE, Store};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::servers::Servers;
use crate::window::{Connected, open_in_browser};

/// The label of the small window, which `capabilities/quick.json` names.
pub const QUICK: &str = "quick";

/// What the small window's page gets as `window.moebiusDesktop`: only `quick`, which calls the commands below.
pub const QUICK_BRIDGE: &str = r#"
(() => {
  const call = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);
  Object.defineProperty(window, "moebiusDesktop", {
    value: Object.freeze({
      quick: Object.freeze({
        shortcut: () => call("quick_shortcut"),
        setShortcut: (shortcut) => call("set_quick_shortcut", { shortcut }),
        finish: (path) => call("quick_finish", { path: path ?? null }),
      }),
    }),
  });
})();
"#;

/// What the app knows of its shortcut: the one in force (or the one it could not take), kept in the app's state.
pub struct Quick {
    store: Store,
    state: Mutex<Slot>,
}

struct Slot {
    /// The shortcut a person chose, or the default.
    shortcut: String,
    /// Why it is not registered, such as another program holding it.
    error: Option<String>,
    /// The origin of the proxy the small window was made for.
    shown_for: Option<String>,
}

/// The answer to `quick_shortcut`.
#[derive(Serialize)]
pub struct Info {
    shortcut: String,
    error: Option<String>,
}

/// Loads the chosen shortcut and registers it; called once when the app starts. A shortcut that cannot be registered
/// leaves the app running, with the reason for the small window to show.
pub fn setup(app: &tauri::App) {
    let dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let store = Store::new(&dir);
    let shortcut = store.load();
    let error = register(app.handle(), &shortcut).err();
    app.manage(Quick {
        store,
        state: Mutex::new(Slot {
            shortcut,
            error,
            shown_for: None,
        }),
    });
}

/// Registers `shortcut` to toggle the small window.
fn register(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    let parsed: Shortcut = shortcut.parse().map_err(|e| format!("{e}"))?;
    app.global_shortcut()
        .on_shortcut(parsed, |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                let app = app.clone();
                // Windows are made away from the thread that gets the key, as Tauri asks.
                tauri::async_runtime::spawn(toggle(app));
            }
        })
        .map_err(|e| format!("{shortcut} could not be taken: {e}"))
}

/// Shows the small window, or hides it when it is shown already.
async fn toggle(app: AppHandle) {
    if let Some(window) = app.get_webview_window(QUICK)
        && window.is_visible().unwrap_or(false)
    {
        let _ = window.hide();
        return;
    }
    if let Err(e) = show(&app).await {
        eprintln!("moebius-desktop: cannot open the quick window: {e}");
    }
}

async fn show(app: &AppHandle) -> tauri::Result<()> {
    let servers = app.state::<Servers>();
    let Some((origin, entry)) = servers.entry_at(PAGE).await else {
        // Nothing to ask before a server is connected: the main window comes forward with the connect page.
        return bring_forward(app);
    };
    let url = entry
        .parse()
        .map_err(|e| tauri::Error::Io(std::io::Error::other(e)))?;
    let quick = app.state::<Quick>();
    let stale = quick
        .state
        .lock()
        .expect("the shortcut's state")
        .shown_for
        .as_deref()
        != Some(&origin);
    match app.get_webview_window(QUICK) {
        Some(window) => {
            if stale {
                window.navigate(url)?;
                // Only once it is on its way: a refused navigation is tried again at the next press.
                quick.state.lock().expect("the shortcut's state").shown_for = Some(origin);
            }
            window.show()?;
            window.set_focus()?;
            if !stale {
                window.eval(quick::shown_script())?;
            }
        }
        None => {
            // Read at each navigation, not kept: the window outlives the server it was made for.
            let navigating = app.state::<Connected>().inner().clone();
            quick.state.lock().expect("the shortcut's state").shown_for = Some(origin);
            WebviewWindowBuilder::new(app, QUICK, WebviewUrl::External(url))
                .title("Moebius")
                .inner_size(640.0, 230.0)
                .resizable(false)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .center()
                .focused(true)
                .initialization_script(QUICK_BRIDGE)
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
                .on_new_window(|url, _| {
                    if links::target(url.as_str(), None) == Target::Browser {
                        open_in_browser(url.as_str());
                    }
                    tauri::webview::NewWindowResponse::Deny
                })
                .build()?;
        }
    }
    Ok(())
}

/// Brings the main window to the front, from the tray or the back of other windows.
fn bring_forward(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = crate::pages::main_window(app) {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

/// `quick_shortcut`: the shortcut in force, and why it is not registered when it is not.
#[tauri::command]
pub fn quick_shortcut(quick: State<'_, Quick>) -> Info {
    let slot = quick.state.lock().expect("the shortcut's state");
    Info {
        shortcut: slot.shortcut.clone(),
        error: slot.error.clone(),
    }
}

/// `set_quick_shortcut`: takes `shortcut` in place of the one in force and keeps it for the next start. The new one is
/// registered first, so one that is refused leaves the old one working. The answer is the shortcut as written.
#[tauri::command]
pub fn set_quick_shortcut(
    app: AppHandle,
    quick: State<'_, Quick>,
    shortcut: String,
) -> Result<String, String> {
    let written = quick::normalize(&shortcut).map_err(|e| e.to_string())?;
    let (old, old_error) = {
        let slot = quick.state.lock().expect("the shortcut's state");
        (slot.shortcut.clone(), slot.error.clone())
    };
    if written != old || old_error.is_some() {
        register(&app, &written)?;
        if let Err(e) = quick.store.save(&written) {
            // Kept for this run only would surprise at the next start: the old one stays.
            unregister(&app, &written);
            if old_error.is_none() && written == old {
                // It was in force and is not any longer; say so rather than leave a stale state.
                let mut slot = quick.state.lock().expect("the shortcut's state");
                slot.error = Some(format!("{old} was released: {e}"));
            }
            return Err(e.to_string());
        }
        if written != old && old_error.is_none() {
            unregister(&app, &old);
        }
    }
    let mut slot = quick.state.lock().expect("the shortcut's state");
    slot.shortcut = written.clone();
    slot.error = None;
    Ok(written)
}

fn unregister(app: &AppHandle, shortcut: &str) {
    if let Ok(parsed) = shortcut.parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(parsed);
    }
}

/// `quick_finish`: hides the small window and, when `path` is given, shows that page of the web app in the main
/// window, which comes forward.
#[tauri::command]
pub fn quick_finish(app: AppHandle, path: Option<String>) -> Result<(), String> {
    let script = path
        .as_deref()
        .map(quick::open_script)
        .transpose()
        .map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window(QUICK) {
        window.hide().map_err(|e| e.to_string())?;
    }
    if let Some(script) = script {
        bring_forward(&app).map_err(|e| e.to_string())?;
        if let Some(window) = crate::pages::front(&app) {
            window.eval(script).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
