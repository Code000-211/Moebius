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
use tauri::Manager;
use tokio::sync::Mutex;

use crate::pages;
use crate::window::Connected;

/// What an error starts with when the SSH host has no server set up, so that the page can offer to set one up.
pub const NO_SERVER: &str = "no-server:";

/// The window's rail.
pub struct Servers {
    rail: Mutex<Rail>,
    /// What the rail last showed, for a call that comes while the rail is busy opening a server (which can take a while
    /// over SSH) and must not wait for it.
    shown: std::sync::Mutex<Vec<Entry>>,
    /// Why no remembered server was opened at the start (the keychain did not answer), for the connect page to show.
    notice: std::sync::Mutex<Option<String>>,
    /// Whether the start's try at the remembered servers is over, which the connect page waits for before it asks for
    /// the notice.
    started: tokio::sync::watch::Sender<bool>,
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
            shown: std::sync::Mutex::new(Vec::new()),
            notice: std::sync::Mutex::new(None),
            started: tokio::sync::watch::channel(false).0,
        }
    }
}

fn say(error: RailError) -> String {
    match error {
        RailError::NoServer(host) => format!("{NO_SERVER} no server is set up on {host}"),
        other => other.to_string(),
    }
}

impl Servers {
    /// The origin of the open server's web app and the address that opens its page `path` in a second window, or none
    /// when no server is open. It waits while the rail is busy opening a server, which is what the person waits for.
    pub async fn entry_at(&self, path: &str) -> Option<(String, String)> {
        let rail = self.rail.lock().await;
        Some((rail.open_origin()?, rail.open_entry_at(path)?))
    }
}

/// Shows the web app of the server that is open in the window (`pages`), and points what follows a server at it.
fn show(app: &tauri::AppHandle, rail: &mut Rail, url: &str) -> Result<(), String> {
    let (Some(id), Some(origin)) = (rail.open_id().map(str::to_string), rail.open_origin()) else {
        return Err("no server is open".to_string());
    };
    app.state::<Connected>().set(Some(origin.clone()));
    match rail.open_server() {
        Some(server) => crate::notifications::start(app, server),
        None => crate::notifications::stop(app),
    }
    crate::tray_icon::follow(app, rail.open_server());
    pages::show_server(app, rail, &id, &origin, url)
}

/// Shows the connect page, when no server is left to show.
fn show_connect_page(app: &tauri::AppHandle) -> Result<(), String> {
    app.state::<Connected>().set(None);
    crate::notifications::stop(app);
    crate::tray_icon::follow(app, None);
    pages::show_connect(app)
}

/// Opens the first server of the rail that answers, the one open at the last close first; the connect page stays when
/// none does. A keychain that does not answer ends the tries, since every server needs it: the connect page says so.
pub async fn open_remembered(app: tauri::AppHandle) {
    let servers = app.state::<Servers>();
    open_first(&app, &servers).await;
    servers.started.send_replace(true);
}

async fn open_first(app: &tauri::AppHandle, servers: &Servers) {
    let mut rail = servers.rail.lock().await;
    for id in rail.startup_order() {
        match rail.open(&id).await {
            Ok(url) => {
                let _ = show(app, &mut rail, &url);
                return;
            }
            Err(error @ RailError::Keychain(_)) => {
                if let Ok(mut notice) = servers.notice.lock() {
                    *notice = Some(error.to_string());
                }
                return;
            }
            Err(_) => {}
        }
    }
}

/// What kept the remembered servers from opening at the start, or none: the connect page shows it above its form. It
/// answers once the start's try is over, which can take as long as the keychain's time limit.
#[tauri::command]
pub async fn startup_notice(app: tauri::AppHandle) -> Option<String> {
    let servers = app.state::<Servers>();
    let mut over = servers.started.subscribe();
    over.wait_for(|over| *over).await.ok()?;
    servers.notice.lock().ok()?.clone()
}

/// Connects to the server at `address` with the access token `token`: checks it, keeps its token in the keychain and
/// shows its web app in the window. With `session_only` the token stays in memory and is asked for again at the next
/// start. A server it refuses leaves the window as it is, with the reason.
#[tauri::command]
pub async fn connect(
    app: tauri::AppHandle,
    address: String,
    token: String,
    session_only: Option<bool>,
) -> Result<(), String> {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    let id = rail
        .add(&address, &token, session_only.unwrap_or(false))
        .await
        .map_err(say)?;
    let url = rail.open(&id).await.map_err(say)?;
    show(&app, &mut rail, &url)
}

/// Connects to the server of the SSH host `host`, setting it up there first when `set_up` (the person said yes).
#[tauri::command]
pub async fn connect_ssh(app: tauri::AppHandle, host: String, set_up: bool) -> Result<(), String> {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    let id = rail.add_ssh(&host, set_up).await.map_err(say)?;
    let url = rail.open(&id).await.map_err(say)?;
    show(&app, &mut rail, &url)
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
    let entries = rail::poll(targets, crate::tray_icon::figures(&app)).await;
    if let Ok(mut shown) = servers.shown.lock() {
        *shown = entries.clone();
    }
    entries
}

/// Shows the server `id` in the window: its webview, on the page it was left on, when it has one.
#[tauri::command]
pub async fn switch_server(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    let url = rail.open(&id).await.map_err(say)?;
    show(&app, &mut rail, &url)
}

/// Removes the server `id`, its token with it. When it was the open one, the window shows the next server that
/// answers, or the connect page.
#[tauri::command]
pub async fn remove_server(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    let was_open = rail.remove(&id).await.map_err(say);
    pages::forget(&app, &id);
    if !was_open? {
        return Ok(());
    }
    for next in rail.startup_order() {
        if let Ok(url) = rail.open(&next).await {
            return show(&app, &mut rail, &url);
        }
    }
    show_connect_page(&app)
}

/// Names the server `id` `name`, which the rail shows from then on; an empty name takes the server's own back.
#[tauri::command]
pub async fn rename_server(app: tauri::AppHandle, id: String, name: String) -> Result<(), String> {
    let servers = app.state::<Servers>();
    let mut rail = servers.rail.lock().await;
    rail.rename(&id, &name).map_err(say)
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
/// shows it, its token in the keychain or, with `session_only`, in memory. The token is read by the app and never goes
/// to the page.
#[tauri::command]
pub async fn local_connect(
    app: tauri::AppHandle,
    session_only: Option<bool>,
) -> Result<(), String> {
    let local = Local::of_this_machine()?;
    let server = local.start().await.map_err(|e| e.to_string())?;
    connect(app, server.url, server.token, session_only).await
}
