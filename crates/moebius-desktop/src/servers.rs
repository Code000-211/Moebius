//! The commands of the rail of servers, on the Tauri side: they turn the library's [`Rail`] into what the window does
//! (show a server's web app, or the connect page again) and what the pages ask of it. The connect page calls `connect`
//! for the first server; the web app's rail calls them all. Tokens go in and never come out: no command returns one.
//!
//! An error is a sentence for the person, except that a host with no server set up says so with [`NO_SERVER`] before its
//! sentence, which the rail answers by asking whether to set one up.

use moebius_desktop::local::{self, Local};
use moebius_desktop::rail::secrets::Keychain;
use moebius_desktop::rail::{self, Entry, Options, Rail, RailError};
use std::sync::Arc;
use tauri::{Manager, Url};
use tokio::sync::Mutex;

use crate::window::{Connected, MAIN};

/// What an error starts with when the SSH host has no server set up, so that the page can offer to set one up.
pub const NO_SERVER: &str = "no-server:";

/// The window's rail, and the connect page's address for when no server is open.
pub struct Servers {
    rail: Mutex<Rail>,
    connect_page: std::sync::Mutex<Option<Url>>,
    /// What the rail last showed, for a call that comes while the rail is busy opening a server (which can take a while
    /// over SSH) and must not wait for it.
    shown: std::sync::Mutex<Vec<Entry>>,
}

impl Servers {
    /// The rail of the servers saved in the app's configuration directory, with their tokens in the system's keychain.
    pub fn new(app: &tauri::AppHandle) -> Self {
        let dir = app
            .path()
            .app_config_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("moebius-desktop"));
        Self {
            rail: Mutex::new(Rail::new(&dir, Arc::new(Keychain), Options::standard())),
            connect_page: std::sync::Mutex::new(None),
            shown: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Notes the address of the connect page, which the window shows first.
    pub fn note_connect_page(&self, url: Url) {
        *self.connect_page.lock().expect("not poisoned") = Some(url);
    }
}

fn say(error: RailError) -> String {
    match error {
        RailError::NoServer(host) => format!("{NO_SERVER} no server is set up on {host}"),
        other => other.to_string(),
    }
}

/// Notes the page the window is on, if it is a page of the server that is open.
fn note_page(window: &tauri::WebviewWindow, rail: &mut Rail) {
    let (Some(origin), Ok(url)) = (rail.open_origin(), window.url()) else {
        return;
    };
    if url.as_str().starts_with(&origin) {
        let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
        rail.note_page(&format!("{}{query}", url.path()));
    }
}

/// Shows the address of a server's web app in the window.
fn show(app: &tauri::AppHandle, rail: &Rail, url: &str) -> Result<(), String> {
    let window = app.get_webview_window(MAIN).ok_or("the window is gone")?;
    app.state::<Connected>().set(rail.open_origin());
    match rail.open_server() {
        Some(server) => crate::notifications::start(app, server),
        None => crate::notifications::stop(app),
    }
    window
        .navigate(url.parse().map_err(|e| format!("{e}"))?)
        .map_err(|e| e.to_string())
}

/// Shows the connect page, when no server is left to show.
fn show_connect_page(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app.get_webview_window(MAIN).ok_or("the window is gone")?;
    app.state::<Connected>().set(None);
    crate::notifications::stop(app);
    let page = app
        .state::<Servers>()
        .connect_page
        .lock()
        .map_err(|_| "the app's state is broken")?
        .clone()
        .ok_or("the connect page is not known")?;
    window.navigate(page).map_err(|e| e.to_string())
}

/// Opens the first server of the rail that answers, the one open at the last close first; the connect page stays when
/// none does.
pub async fn open_remembered(app: tauri::AppHandle) {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    for id in rail.startup_order() {
        if let Ok(url) = rail.open(&id).await {
            let _ = show(&app, &rail, &url);
            return;
        }
    }
}

/// Connects to the server at `address` with the access token `token`: checks it, keeps its token in the keychain and
/// shows its web app in the window. A server it refuses leaves the window as it is, with the reason.
#[tauri::command]
pub async fn connect(app: tauri::AppHandle, address: String, token: String) -> Result<(), String> {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    if let Some(window) = app.get_webview_window(MAIN) {
        note_page(&window, &mut rail);
    }
    let id = rail.add(&address, &token).await.map_err(say)?;
    let url = rail.open(&id).await.map_err(say)?;
    show(&app, &rail, &url)
}

/// Connects to the server of the SSH host `host`, setting it up there first when `set_up` (the person said yes).
#[tauri::command]
pub async fn connect_ssh(app: tauri::AppHandle, host: String, set_up: bool) -> Result<(), String> {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    if let Some(window) = app.get_webview_window(MAIN) {
        note_page(&window, &mut rail);
    }
    let id = rail.add_ssh(&host, set_up).await.map_err(say)?;
    let url = rail.open(&id).await.map_err(say)?;
    show(&app, &rail, &url)
}

/// What the rail shows: each server, the open one marked, with its Inbox's count and whether it answers. While the rail
/// is busy opening a server it answers with what it showed last.
#[tauri::command]
pub async fn servers(app: tauri::AppHandle) -> Vec<Entry> {
    let servers = app.state::<Servers>();
    let targets = match servers.rail.try_lock() {
        Ok(rail) => rail.targets(),
        Err(_) => return servers.shown.lock().map(|e| e.clone()).unwrap_or_default(),
    };
    let entries = rail::poll(targets).await;
    if let Ok(mut shown) = servers.shown.lock() {
        *shown = entries.clone();
    }
    entries
}

/// Shows the server `id` in the window, on the page it was left on.
#[tauri::command]
pub async fn switch_server(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    if let Some(window) = app.get_webview_window(MAIN) {
        note_page(&window, &mut rail);
    }
    let url = rail.open(&id).await.map_err(say)?;
    show(&app, &rail, &url)
}

/// Removes the server `id`, its token with it. When it was the open one, the window shows the next server that
/// answers, or the connect page.
#[tauri::command]
pub async fn remove_server(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    if !rail.remove(&id).map_err(say)? {
        return Ok(());
    }
    for next in rail.startup_order() {
        if let Ok(url) = rail.open(&next).await {
            return show(&app, &rail, &url);
        }
    }
    show_connect_page(&app)
}

/// What this machine has of a server of its own, for the connect page's offer: `not-set-up`, `stopped`, `running`, or
/// `unavailable: <why>`. It only looks.
#[tauri::command]
pub async fn local_state() -> String {
    let local = match Local::of_this_machine() {
        Ok(local) => local,
        Err(reason) => return format!("unavailable: {reason}"),
    };
    match local.state().await {
        local::State::Unavailable(reason) => format!("unavailable: {reason}"),
        local::State::NotSetUp => "not-set-up".to_string(),
        local::State::Stopped => "stopped".to_string(),
        local::State::Running => "running".to_string(),
    }
}

/// Sets up the server of this machine where it is not, starts it where it does not run, and adds it to the rail and
/// shows it. The token is read by the app and never goes to the page.
#[tauri::command]
pub async fn local_connect(app: tauri::AppHandle) -> Result<(), String> {
    let local = Local::of_this_machine()?;
    let server = local.start().await.map_err(|e| e.to_string())?;
    connect(app, server.url, server.token).await
}
