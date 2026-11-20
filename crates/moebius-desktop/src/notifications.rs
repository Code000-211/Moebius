//! The native notifications and the icon's count (ADR 0021): the Tauri side of `moebius_desktop::notices`. Once a
//! server is connected, [`start`] follows its notices; each [`Effect::Notify`] becomes a notification of the system
//! whose click shows the window and opens the notice in the web app, and each [`Effect::Count`] becomes the badge
//! count of the icon (Linux), the number over the taskbar icon (Windows) and the window's title, `(3) Moebius`.
//!
//! Part of the program rather than of the library, as it calls Tauri and the system's notification service; what it
//! decides is in the library and tested there.

use std::sync::Mutex;

use moebius_desktop::badge;
use moebius_desktop::notices::{self, Effect, Notice, Watcher};
use moebius_desktop::server::Server;
#[cfg(windows)]
use tauri::image::Image;
use tauri::{AppHandle, Manager};

/// The watch of the connected server's notices; a server connected later replaces it, which stops the earlier one.
#[derive(Default)]
pub struct Watching(Mutex<Option<Watcher>>);

/// Follows the notices of `server` until the next server replaces it.
pub fn start(app: &AppHandle, server: Server) {
    let label = moebius_desktop::rail::short_address(&server.url);
    let (watcher, effects) = notices::watch(server);
    let mut effects = notices::coalesce(effects, notices::BURST);
    if let Ok(mut watching) = app.state::<Watching>().0.lock() {
        *watching = Some(watcher);
    }
    count(app, 0);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(effect) = effects.recv().await {
            match effect {
                Effect::Count(unread) => count(&app, unread),
                Effect::Notify(notice) => notify(&app, &notice),
                Effect::Digest(notices) => {
                    let (title, body, path) = notices::digest_text(&notices, &label);
                    let app = app.clone();
                    show(&title, &body, move || open(&app, &path));
                }
            }
        }
    });
}

/// Stops following notices, when no server is open, and clears the count.
pub fn stop(app: &AppHandle) {
    if let Ok(mut watching) = app.state::<Watching>().0.lock() {
        *watching = None;
    }
    count(app, 0);
}

/// Puts the count on the icon and in the title.
fn count(app: &AppHandle, unread: usize) {
    let Some(window) = crate::pages::main_window(app) else {
        return;
    };
    let _ = window.set_badge_count((unread > 0).then_some(unread as i64));
    #[cfg(windows)]
    {
        let overlay = (unread > 0)
            .then(|| Image::new_owned(badge::overlay(unread), badge::SIDE, badge::SIDE));
        let _ = window.set_overlay_icon(overlay);
    }
    let _ = window.set_title(&badge::title(unread));
}

/// Shows the notification of `notice`; choosing it opens the notice in the window.
fn notify(app: &AppHandle, notice: &Notice) {
    let (title, body) = notice.text();
    let (app, path) = (app.clone(), notice.path());
    show(&title, &body, move || open(&app, &path));
}

/// Brings the window forward and goes to `path` in the web app, as a link of the web app does, without loading it again.
fn open(app: &AppHandle, path: &str) {
    let Some(window) = crate::pages::main_window(app) else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    if let (Ok(path), Some(webview)) = (serde_json::to_string(path), crate::pages::front(app)) {
        let _ = webview.eval(format!(
            "history.pushState({{}}, '', {path}); dispatchEvent(new PopStateEvent('popstate'));"
        ));
    }
}

/// How long a notification's answer is waited for. A notification stays in the system's notification service for as
/// long as that keeps it; after this the app lets go of it (and of the connection it waits on), so that a service that
/// never says a notification was closed does not hold something of the app for each one.
#[cfg(target_os = "linux")]
const ANSWER_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

/// The system's notification service on Linux: a notification with the one action `default`, which choosing it
/// answers. Waiting for the answer is a task of the app's runtime, not a thread of its own, and ends when the
/// notification is closed or after [`ANSWER_WAIT`].
#[cfg(target_os = "linux")]
fn show(title: &str, body: &str, on_click: impl FnOnce() + Send + 'static) {
    let (title, body) = (title.to_string(), body.to_string());
    tauri::async_runtime::spawn(async move {
        let shown = notify_rust::Notification::new()
            .appname("Moebius")
            .summary(&title)
            .body(&body)
            .action("default", "Open")
            .show_async()
            .await;
        if let Ok(handle) = shown {
            let waiting = handle.wait_for_action_async(|response| {
                if matches!(response, notify_rust::NotificationResponse::Default) {
                    on_click();
                }
            });
            let _ = tokio::time::timeout(ANSWER_WAIT, waiting).await;
        }
    });
}

/// Windows' toasts, which call back when one is activated.
#[cfg(windows)]
fn show(title: &str, body: &str, on_click: impl FnOnce() + Send + 'static) {
    let on_click = std::sync::Mutex::new(Some(on_click));
    let _ = tauri_winrt_notification::Toast::new("io.github.code000-211.moebius")
        .title(title)
        .text1(body)
        .on_activated(move |_| {
            if let Some(on_click) = on_click.lock().ok().and_then(|mut once| once.take()) {
                on_click();
            }
            Ok(())
        })
        .show();
}

/// There is no macOS build (ADR 0021): the app shows no notification there.
#[cfg(not(any(target_os = "linux", windows)))]
fn show(_: &str, _: &str, _: impl FnOnce() + Send + 'static) {}
