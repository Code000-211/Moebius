//! The Tauri side of the window's pages: the main window holds one webview for the connect page and one for every
//! server that was opened, all of the window's size, of which exactly one is shown (`moebius_desktop::views` keeps the
//! bookkeeping). Switching to a server that was opened is showing its webview, so its page is as it was left, with
//! nothing loaded; a server opened for the first time is loaded out of sight while the page that is shown stays, and is
//! shown once it has drawn its rail, so the window is never blank and the rail never goes away. Webviews not looked at for long
//! are closed, and the page each was on is noted by the rail, which opens that server there again.
//!
//! Every page gets the script that resizes the window from its edges (`edges`), which the child webviews would
//! otherwise leave without one. Every webview is a child of the window, which is why the program uses [`main_window`] and [`front`] where it once
//! asked for the one webview window: a window with several webviews is not a webview window to Tauri.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use moebius_desktop::edges;
use moebius_desktop::links::{self, Target};
use moebius_desktop::rail::Rail;
use moebius_desktop::views::{self, Views};
use tauri::webview::{NewWindowResponse, WebviewBuilder};
use tauri::{AppHandle, Manager, PhysicalPosition, Webview, WebviewUrl, Window};

use crate::window::{BRIDGE, MAIN, open_in_browser};

/// The label of the connect page's webview.
pub const CONNECT: &str = "connect";

/// What a page that was hidden is told when it is shown, so that it reads again what it drew while it was away (the
/// rail's marks, say).
const SHOWN: &str = "window.dispatchEvent(new Event('moebius:shown'));";

/// How long the first load of a server is waited for before its page is shown as it is, so that a page that never
/// draws its rail is seen and not hidden forever.
const LOAD_LIMIT: Duration = Duration::from_secs(20);

/// How often a page that is loading is asked whether it has drawn its rail.
const ASK_EVERY: Duration = Duration::from_millis(40);

/// True once the web app has drawn its shell, which has the rail at its left: the page is worth showing then, and not
/// before, when it is the app's splash.
const DRAWN: &str = "!!document.querySelector('[data-testid=\"rail\"]')";

/// The window's webviews, as the app's state.
pub struct Pages {
    views: Mutex<Views>,
    /// Held while a webview is shown and the others hidden, so that two switches in a row end with the last one's.
    revealing: Mutex<()>,
}

impl Default for Pages {
    fn default() -> Self {
        Self {
            views: Mutex::new(Views::new(views::KEPT_HIDDEN, views::KEPT_FOR)),
            revealing: Mutex::new(()),
        }
    }
}

impl Pages {
    fn lock(&self) -> std::sync::MutexGuard<'_, Views> {
        self.views.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn label(id: &str) -> String {
    format!("server-{id}")
}

/// The main window.
pub fn main_window(app: &AppHandle) -> Option<Window> {
    app.get_window(MAIN)
}

/// The webview that is shown in the main window: a server's web app, or the connect page.
pub fn front(app: &AppHandle) -> Option<Webview> {
    let front = app.state::<Pages>().lock().front().map(label);
    app.get_webview(front.as_deref().unwrap_or(CONNECT))
}

/// Makes the window's first webview, the connect page, and shows it.
pub fn make_connect(app: &AppHandle, window: &Window) -> tauri::Result<Webview> {
    make(
        app,
        window,
        CONNECT,
        WebviewUrl::App("index.html".into()),
        None,
    )
}

/// A webview of the window's size that follows its size. `origin` is the web app it shows, which says which addresses
/// it keeps (`links::target`).
fn make(
    app: &AppHandle,
    window: &Window,
    label: &str,
    url: WebviewUrl,
    origin: Option<String>,
) -> tauri::Result<Webview> {
    let navigating = origin.clone();
    let (opener, own) = (app.clone(), label.to_string());
    let builder = WebviewBuilder::new(label, url)
        .auto_resize()
        .initialization_script(BRIDGE)
        .initialization_script(edges::SCRIPT)
        .on_navigation(
            move |url| match links::target(url.as_str(), navigating.as_deref()) {
                Target::Window => true,
                Target::Browser => {
                    open_in_browser(url.as_str());
                    false
                }
                Target::Nowhere => false,
            },
        )
        .on_new_window(move |url, _| {
            match links::target(url.as_str(), origin.as_deref()) {
                // A page of the web app asked for a window of its own: the webview that asked shows it.
                Target::Window => {
                    if let Some(webview) = opener.get_webview(&own) {
                        let _ = webview.navigate(url);
                    }
                }
                Target::Browser => open_in_browser(url.as_str()),
                Target::Nowhere => {}
            }
            NewWindowResponse::Deny
        });
    window.add_child(builder, PhysicalPosition::new(0, 0), window.inner_size()?)
}

/// Shows the webview of the server `id` and hides the others, when it is the server that is to be shown and its page
/// has drawn its rail. The page is told it is shown when `told`, which a page that was there is, and one that has just
/// drawn itself is not.
fn reveal(app: &AppHandle, id: &str, told: bool) {
    let pages = app.state::<Pages>();
    let _one_at_a_time = pages.revealing.lock().unwrap_or_else(|e| e.into_inner());
    {
        let views = pages.lock();
        if views.shown() != Some(id) || !views.is_drawn(id) {
            return;
        }
    }
    let Some(webview) = app.get_webview(&label(id)) else {
        return;
    };
    let _ = webview.show();
    hide_all_but(app, webview.label());
    pages.lock().set_front(Some(id));
    let _ = webview.set_focus();
    if told {
        let _ = webview.eval(SHOWN);
    }
}

/// Hides every webview of the main window but `keep`, after `keep` is shown, so that the window never shows nothing.
fn hide_all_but(app: &AppHandle, keep: &str) {
    if let Some(window) = main_window(app) {
        for webview in window.webviews() {
            if webview.label() != keep {
                let _ = webview.hide();
            }
        }
    }
}

/// Waits until the page of `webview` has drawn its rail, asking it every so often.
async fn drawn(webview: &Webview) {
    loop {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = Mutex::new(Some(tx));
        let asked = webview.eval_with_callback(DRAWN, move |answer| {
            if let Some(tx) = tx.lock().ok().and_then(|mut tx| tx.take()) {
                let _ = tx.send(answer);
            }
        });
        // A page that is navigating drops the question; it is asked again.
        match (
            asked,
            tokio::time::timeout(Duration::from_millis(500), rx).await,
        ) {
            (Ok(()), Ok(Ok(answer))) if answer == "true" => return,
            _ => {}
        }
        tokio::time::sleep(ASK_EVERY).await;
    }
}

/// Shows the web app of the server `id`, which is served at `origin`: its webview when it has one, else a new one that
/// loads `url` out of sight and is shown once it has drawn its rail. Then lets go of the webviews that were not looked at for long,
/// noting the page of each in `rail`.
pub fn show_server(
    app: &AppHandle,
    rail: &mut Rail,
    id: &str,
    origin: &str,
    url: &str,
) -> Result<(), String> {
    let window = main_window(app).ok_or("the window is gone")?;
    let url: tauri::Url = url.parse().map_err(|e| format!("{e}"))?;
    let name = label(id);
    let there = app.state::<Pages>().lock().show(id, origin, Instant::now())
        && app.get_webview(&name).is_some();
    if there {
        // A webview still loading is shown by the task that waits for it; one that has drawn is shown now.
        reveal(app, id, true);
    } else {
        // What stands there was made for an address that is gone.
        if let Some(old) = app.get_webview(&name) {
            let _ = old.close();
        }
        let made = make(
            app,
            &window,
            &name,
            WebviewUrl::External(url),
            Some(origin.to_string()),
        );
        match made {
            Ok(webview) => {
                let _ = webview.hide();
                let (app, id) = (app.clone(), id.to_string());
                tauri::async_runtime::spawn(async move {
                    let _ = tokio::time::timeout(LOAD_LIMIT, drawn(&webview)).await;
                    // A page that never drew its rail is shown as it is, so that it is seen.
                    app.state::<Pages>().lock().mark_drawn(&id);
                    reveal(&app, &id, false);
                });
            }
            Err(e) => {
                app.state::<Pages>().lock().forget(id);
                return Err(e.to_string());
            }
        }
    }
    let idle = app.state::<Pages>().lock().expired(Instant::now());
    for id in idle {
        if let Some(webview) = app.get_webview(&label(&id)) {
            if let (Some(origin), Ok(url)) = (rail.origin_of(&id), webview.url())
                && url.as_str().starts_with(&origin)
            {
                let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
                rail.note_page_of(&id, &format!("{}{query}", url.path()));
            }
            let _ = webview.close();
        }
    }
    Ok(())
}

/// Shows the connect page, which is read again so that it lists the servers as they are now.
pub fn show_connect(app: &AppHandle) -> Result<(), String> {
    app.state::<Pages>().lock().show_none(Instant::now());
    let webview = app.get_webview(CONNECT).ok_or("the connect page is gone")?;
    let _ = webview.eval("location.reload();");
    let pages = app.state::<Pages>();
    let _one_at_a_time = pages.revealing.lock().unwrap_or_else(|e| e.into_inner());
    let _ = webview.show();
    hide_all_but(app, CONNECT);
    pages.lock().set_front(None);
    let _ = webview.set_focus();
    Ok(())
}

/// Lets go of the webview of the server `id`, which was removed. When it was the one in front, the connect page comes
/// forward until the next server has drawn, so that the window is not blank.
pub fn forget(app: &AppHandle, id: &str) {
    let pages = app.state::<Pages>();
    let was_front = {
        let mut views = pages.lock();
        let was = views.front() == Some(id);
        views.forget(id);
        was
    };
    if was_front && let Some(connect) = app.get_webview(CONNECT) {
        let _ = connect.show();
    }
    if let Some(webview) = app.get_webview(&label(id)) {
        let _ = webview.close();
    }
}
