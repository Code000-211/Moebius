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
- **Tauri 2**: Rust like the rest of Moebius and about 10MB, using the system's web engine (WebKit on macOS and Linux,
  WebView2 on Windows).
- For the interface: **loaded from the server**, so it always matches the server's version, or **bundled in the app**,
  so it starts at once and without a network.

## Decision

- **A Tauri 2 app, `crates/moebius-desktop`**, for macOS, Windows and Linux, built in every release, unsigned and without
  automatic updates for now.
- **The web app is bundled**: `web/dist` of the same release is in the app. The app asks a server for its version when
  it connects and **refuses a server of another minor version**, naming the release that matches; it never runs the
  interface against an API it was not built for.
- **The same look everywhere**: the app draws what the web app draws, with no path of its own for one system; the
  fallbacks of 0020 (an opaque surface where blur is missing) are the only difference a system's engine may cause.
- **Several servers**, each an organisation, in a **rail at the far left** on the textured ground: a rounded square per
  server with the count of its Inbox, a grey dot for one that cannot be reached, "+" to connect one
  ([round 10](../design-drafts/round-10.html), 10A). A server is connected by its address, over SSH as
  `moebius connect` does (setting it up after a yes), or as a **local server** the app starts on this machine where the
  server runs (Linux, and Windows inside WSL 2). Access tokens are kept in the system's keychain (Keychain, Credential
  Manager, Secret Service), never in a file.
- **Native notifications** for what the Inbox gets (a change ready to merge, a mention, an assignment), and the count on
  the app's icon.
- **A menu bar or tray icon** with the server's state: agents working, the Inbox, today's tokens against the budget;
  New issue, Ask Moebius, Open Moebius, and **Pause all agents**, which holds every run of the server until it is
  resumed ([round 10](../design-drafts/round-10.html), 10C).
- **A global shortcut**, `Ctrl+Shift+Space` by default and changeable, opens a small window over everything: one line
  to ask the Moebius agent or make an issue in the chosen project; the answer opens in the app (10D).

## Consequences

- The release builds three more archives, and CI builds the app on Linux, Windows and macOS. Unsigned builds warn on
  their first start on macOS and Windows; the guide says how to open them.
- Pausing all agents is a new route of the desktop API (`POST /v1/runs/pause`, `POST /v1/runs/resume`), held in the
  store, and the board engine starts no run while it holds; its reference and `"$MOEBIUS"` command come with it.
- `moebius open` and `moebius connect` stay for people without the app.
- On macOS the app is a client only until the server runs there.
- The name "desktop API" fits again.
