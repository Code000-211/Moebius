//! The tray icon (ADR 0021, D5): a menu the system draws, with the server's state as lines (the agents working, the
//! Inbox, today's tokens against the budget), then New issue, Ask Moebius, Open Moebius and Pause all agents, which
//! says Resume all agents while they are paused. What it shows comes from `moebius_desktop::tray`, which follows the
//! server's events; this module is only the Tauri side of it, in the program so that the library's tests do not link
//! Tauri (see `window`).

use std::sync::Mutex;

use moebius_desktop::server::Server;
use moebius_desktop::tray::{self, Figures, Lines};
use tauri::menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::window::MAIN;

const NEW_ISSUE: &str = "new-issue";
const ASK: &str = "ask-moebius";
const OPEN: &str = "open-moebius";
const PAUSE: &str = "pause-agents";

/// Opens the form of a new issue, which the web app opens on `C` from any page.
const SHOW_NEW_ISSUE: &str =
    "window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', bubbles: true }));";
/// Opens a new chat with the Moebius agent, as the sidebar's "+" does.
const SHOW_CHAT: &str =
    "history.pushState(null, '', '/chat'); window.dispatchEvent(new PopStateEvent('popstate'));";

/// The tray's menu items, the server whose state they show, and the task that follows it.
pub struct Tray {
    working: MenuItem<tauri::Wry>,
    inbox: MenuItem<tauri::Wry>,
    tokens: MenuItem<tauri::Wry>,
    pause: MenuItem<tauri::Wry>,
    server: Mutex<Option<Server>>,
    /// The figures last read; none while there is no server or it cannot be reached.
    figures: Mutex<Option<Figures>>,
    following: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

/// Makes the tray icon, with its menu showing no server yet.
pub fn create(app: &mut tauri::App) -> tauri::Result<()> {
    let line = |id: &str, text: &str| MenuItemBuilder::with_id(id, text).enabled(false).build(app);
    let item = |id: &str, text: &str| MenuItemBuilder::with_id(id, text).build(app);
    let offline = tray::offline();
    let state = Tray {
        working: line("working", &offline.working)?,
        inbox: line("inbox", &offline.inbox)?,
        tokens: line("tokens", &offline.tokens)?,
        pause: item(PAUSE, &offline.pause)?,
        server: Mutex::default(),
        figures: Mutex::default(),
        following: Mutex::default(),
    };
    state.pause.set_enabled(false)?;
    let menu: Menu<_> = MenuBuilder::new(app)
        .items(&[&state.working, &state.inbox, &state.tokens])
        .item(&PredefinedMenuItem::separator(app)?)
        .items(&[
            &item(NEW_ISSUE, "New issue")?,
            &item(ASK, "Ask Moebius")?,
            &item(OPEN, "Open Moebius")?,
        ])
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&state.pause)
        .build()?;
    let mut icon = TrayIconBuilder::with_id("moebius")
        .menu(&menu)
        .tooltip(offline.tooltip)
        .on_menu_event(|app, event| chosen(app, event.id().as_ref()));
    if let Some(window_icon) = app.default_window_icon() {
        icon = icon.icon(window_icon.clone());
    }
    icon.build(app)?;
    app.manage(state);
    Ok(())
}

/// Shows the figures of `server` in the tray from now on, in place of those of the server it followed before; none
/// when no server is shown (the connect page), which leaves the tray with no server to ask.
pub fn follow(app: &AppHandle, server: Option<Server>) {
    let tray = app.state::<Tray>();
    *lock(&tray.server) = server.clone();
    // The figures of the server followed before are not the new one's: the rail asks the server until they are read.
    *lock(&tray.figures) = None;
    let task = server.map(|server| {
        let handle = app.clone();
        tauri::async_runtime::spawn(tray::follow(server, move |figures| {
            show(&handle, figures);
        }))
    });
    let following = task.is_some();
    if let Some(earlier) = std::mem::replace(&mut *lock(&tray.following), task) {
        earlier.abort();
    }
    if !following {
        show(app, None);
    }
    let _ = tray.pause.set_enabled(following);
}

/// What the tray last read of the server it follows, which the rail shows of that server in place of asking it.
pub fn figures(app: &AppHandle) -> Option<Figures> {
    lock(&app.state::<Tray>().figures).clone()
}

/// Puts `figures` into the menu and the tooltip; none means the server cannot be reached.
fn show(app: &AppHandle, figures: Option<Figures>) {
    let tray = app.state::<Tray>();
    let lines: Lines = figures.as_ref().map_or_else(tray::offline, Figures::lines);
    *lock(&tray.figures) = figures;
    let _ = tray.working.set_text(&lines.working);
    let _ = tray.inbox.set_text(&lines.inbox);
    let _ = tray.tokens.set_text(&lines.tokens);
    let _ = tray.pause.set_text(&lines.pause);
    if let Some(icon) = app.tray_by_id("moebius") {
        let _ = icon.set_tooltip(Some(lines.tooltip));
    }
}

/// Does what the menu item `id` says.
fn chosen(app: &AppHandle, id: &str) {
    match id {
        OPEN => {
            bring_up(app);
        }
        NEW_ISSUE => bring_up_and(app, SHOW_NEW_ISSUE),
        ASK => bring_up_and(app, SHOW_CHAT),
        PAUSE => pause_or_resume(app),
        _ => {}
    }
}

/// Shows the main window and runs `script` in it, when it shows a server's web app, which the script is for.
fn bring_up_and(app: &AppHandle, script: &str) {
    if let Some(window) = bring_up(app)
        && connected(app)
    {
        let _ = window.eval(script);
    }
}

/// Whether the window shows a server's web app, which the tray's other items can ask things of.
fn connected(app: &AppHandle) -> bool {
    lock(&app.state::<Tray>().server).is_some()
}

/// Shows the main window in front, also when it was minimized.
pub fn bring_up(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    let window = app.get_webview_window(MAIN)?;
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    Some(window)
}

/// Pauses all agents of the server, or resumes them while they are paused. The menu follows the answer the server's
/// events bring, not this click.
fn pause_or_resume(app: &AppHandle) {
    let tray = app.state::<Tray>();
    let Some(server) = lock(&tray.server).clone() else {
        return;
    };
    let pause = !lock(&tray.figures).as_ref().is_some_and(|f| f.paused);
    tauri::async_runtime::spawn(async move {
        // A server that refuses leaves the menu as it was: the tray shows what the server says.
        let _ = tray::set_paused(&server, pause).await;
    });
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}
