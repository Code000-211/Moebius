# A desktop app again, built with Tauri around the web app

[0017](0017-the-desktop-app-is-removed.md) removed the iced desktop app and left the web app as the only front end,
shown by `moebius open` in a browser's app window. Moebius is now meant for teams that are not its authors
([0018](0018-more-than-one-person.md)), and a tool people keep open all day is expected to be an app of its own: in the
dock or the taskbar, with notifications, a menu bar icon and a shortcut that works when it is in the back. This record
brings a desktop app back without a second front end: it is the web app in a native window.

## Considered Options

- **Keep the browser window** of `moebius open`, with an icon and a desktop entry: no notifications, no tray, no global
  shortcut, and no way to hold several servers.
- **Electron**: Chromium on every system, so the look of [0020](0020-a-quiet-standard-with-depth.md) renders the same
  everywhere, at about 100MB and with a Node runtime beside a Rust server.
- **Tauri 2**: Rust like the rest of Moebius and about 10MB, using the system's web engine (WebKitGTK on Linux, WebView2
  on Windows).
- For the interface: **loaded from the server**, so it always matches the server's version, or **bundled in the app**,
  so it starts at once and without a network.

## Decision

- **A Tauri 2 app, `crates/moebius-desktop`, built for Linux first**, where the server runs and its people work, as an
  AppImage and a `.deb`; a Windows build follows from the same code, since CI already builds Moebius there. There is
  **no macOS build** for now. Every release builds them, unsigned and without automatic updates.
- **The web app is bundled**: `web/dist` of the same release is in the app. The app asks a server for its version when
  it connects and **refuses a server of another minor version**, naming the release that matches; it never runs the
  interface against an API it was not built for.
- **The same look everywhere**: the app draws what the web app draws, with no path of its own for one system; the
  fallbacks of 0020 (an opaque surface where blur is missing) are the only difference a system's engine may cause.
- **Several servers**, each an organisation, in a **rail at the far left** on the textured ground: a rounded square per
  server with the count of its Inbox, a grey dot for one that cannot be reached, "+" to connect one
  ([round 10](../design-drafts/round-10.html), 10A). A server is connected by its address, over SSH as
  `moebius connect` does (setting it up after a yes), or as a **local server** the app starts on this machine where the
  server runs (Linux, and Windows inside WSL 2). Access tokens are kept in the system's keychain (Secret Service on
  Linux, Credential Manager on Windows), never in a file.
- **Native notifications** for what the Inbox gets (a change ready to merge, a mention, an assignment), and the count on
  the app's icon.
- **A menu bar or tray icon** with the server's state: agents working, the Inbox, today's tokens against the budget;
  New issue, Ask Moebius, Open Moebius, and **Pause all agents**, which holds every run of the server until it is
  resumed ([round 10](../design-drafts/round-10.html), 10C).
- **A global shortcut**, `Ctrl+Shift+Space` by default and changeable, opens a small window over everything: one line
  to ask the Moebius agent or make an issue in the chosen project; the answer opens in the app (10D).

## Consequences

- The release builds the AppImage, the `.deb` and the Windows installer, and CI builds the app on Linux and Windows. An
  unsigned build warns on its first start on Windows; the guide says how to open it.
- Pausing all agents is a new route of the desktop API (`POST /v1/runs/pause`, `POST /v1/runs/resume`), held in the
  store, and the board engine starts no run while it holds; its reference and `"$MOEBIUS"` command come with it.
- `moebius open` and `moebius connect` stay for people without the app.
- A macOS build is left out until someone needs it; Tauri builds one from the same code, so adding it later is a
  matter of CI, signing and a test on a Mac, not of design.
- The name "desktop API" fits again.

## Follow-up: one webview per server

The first version showed a server by navigating the window's one webview to that server's proxy, which loads the whole
web app again (1.4 to 4 s with a blank window on a machine under load, against 0.4 to 1.1 s for a browser's full load
of the same page) and loses the page's state. The window now holds a webview for the connect page and one for every
server that was opened, all of the window's size (Tauri's multiwebview support, the `unstable` feature), and shows
exactly one. Switching to a server that was opened is showing its webview, in about a tenth of a second and as it was
left; a server opened for the first time loads out of sight while the page shown stays, and is shown once it has drawn
its rail, so the window is never blank and the rail never goes away. The web app marks the square asked for at once.
Memory stays bounded: at most three hidden webviews are kept, each for at most half an hour, and a dropped one is
opened again on the page it was left on. Each webview keeps its own server's origin (its handlers compare addresses
with it), the tray, the notifications and the quick window follow the server that is open, and the main window is a
window with several webviews rather than a webview window.
