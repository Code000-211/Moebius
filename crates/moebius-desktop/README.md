# moebius-desktop

The desktop app ([ADR 0021](../../docs/adr/0021-a-desktop-app-in-tauri.md)): the web app of the same release in a
window of its own, built with Tauri 2 for Linux (an AppImage and a `.deb`) and Windows (an installer), with no macOS
build. How to install and use it is in [the guide](../../docs/guide/desktop-app.md); how a window comes to show the web
app is in [`docs/architecture.md`](../../docs/architecture.md#the-desktop-app).

## Layout

| Path | What it holds |
|---|---|
| `src/main.rs` | The `moebius-desktop` program. |
| `src/lib.rs` | The parts of the app that are tested without a window. |
| `src/links.rs` | Which addresses the window shows (its own pages) and which open in the system's browser. |
| `src/local/` | The server of this machine the app sets up and starts: `mod.rs` (state, `start`), `host.rs` (the shell: `sh`, or `wsl.exe` on Windows), `release.rs` (downloading `moebius` and checking it against `SHA256SUMS`). |
| `src/server.rs` | `Server` (an address and an access token) and `check`, which asks `GET /v1/health` and refuses a server of another minor version, naming the release that matches. |
| `src/proxy/` | The app's own address on the loopback: `assets.rs` serves the web app embedded from `web/dist`, `forward.rs` forwards `/v1/` to the server with the token (HTTP and WebSockets), `mod.rs` lets only the window use it. |
| `src/tray.rs` | What the tray shows of a server and asks of it: the figures, their lines, `follow` (reads them again after every burst of the server's events) and the routes that pause and resume all agents. |
| `src/tray_icon.rs` | The Tauri side of the tray, in the program: the icon, its menu and what its items do. |
| `src/rail/` | The rail of servers: `mod.rs` (the remembered servers, a proxy and a page of each, adding, opening, removing, `poll`), `records.rs` (`servers.json`, no token), `secrets.rs` (the system's keychain), `ssh.rs` (a server over an SSH tunnel) and `status.rs` (whether a server answers and its Inbox's count). |
| `src/servers.rs` | The rail's commands on the Tauri side: `connect`, `connect_ssh`, `servers`, `switch_server` and `remove_server`, and `local_state` and `local_connect` for the server of this machine. |
| `src/window.rs` | The Tauri side, a module of the program so that the library's tests do not link Tauri: the window without a system frame, the bridge `window.moebiusDesktop` every page gets, and links that leave the app opened in the browser. |
| `src/notices.rs` | `watch`: the notices of the person on a server, from `/v1/events` and `GET /v1/notices`, as `Effect`s (notify once, the count of unread). |
| `src/badge.rs` | The count as a title and as the overlay image Windows draws over the taskbar icon. |
| `src/notifications.rs` | The Tauri side of notices, a module of the program: native notifications that open their notice, the icon's count. |
| `tests/notices.rs` | The notices on a fake server: one event, one notification; reading lowers the count; a refused token gives nothing. |
| `connect/` | The connect page, bundled by Tauri: a server's address and a token, drawn with the web app's tokens. |
| `capabilities/` | What each page may call: `connect.json` for the connect page and `web-app.json` for the web app on the loopback: the window's controls and the rail's commands, nothing else. |
| `tauri.conf.json`, `build.rs`, `icons/` | Tauri's configuration, the bundles and their icons. `build.rs` declares the rail's commands as the app's own and builds the app again when `web/dist` changes. |
| `tests/rail.rs` | The rail against fake servers (and a fake `ssh`): two servers switched between with each keeping its page, a grey dot for one that does not answer, a token in the keychain and in no file, and a removed server that takes its token with it. |

| `tests/local.rs` | The local server against a fake release and a fake `moebius`: a first start makes and connects to a server, a restart finds it again, a stopped one is started without a new `init`, a bad checksum or a missing release installs nothing. |
| `tests/tray.rs` | The tray's figures against a real server on port 0: they follow its events, pausing from the tray pauses the server, and a server that cannot be reached shows none. |
| `tests/connect.rs` | The app against a real server on port 0 and against fake servers: signing in with an access token, the web app and the event stream through the proxy, the refusal of another minor version, and who may use the proxy. |

## Building

On Linux, Tauri needs WebKitGTK 4.1 (on Debian and Ubuntu: `sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev`). On
Windows it needs what the rest of the workspace needs: Rust with the MSVC build tools, and NASM for `aws-lc-sys` or
`AWS_LC_SYS_PREBUILT_NASM=1` to use its prebuilt objects, as CI does; the WebView2 runtime is part of Windows 10 and 11.
The app embeds `web/dist`, so build the web app first:

```sh
pnpm -C web install && pnpm -C web build
cargo run -p moebius-desktop          # the app, from the build
cargo test -p moebius-desktop
```

The bundles are made by the Tauri command line from this directory, as the release does:

```sh
pnpm dlx @tauri-apps/cli@2.12.4 build --bundles appimage,deb   # Linux
pnpm dlx @tauri-apps/cli@2.12.4 build --bundles nsis           # Windows
```

They land in `target/release/bundle/`.

## For what comes next

The later parts of ADR 0021 build on these names:

- **The bridge** is `window.moebiusDesktop` (`BRIDGE` in `src/window.rs`; it also carries the rail's calls), read in the web app only through
  `web/src/desktop/bridge.ts` (`DesktopBridge`, `desktopBridge()`). A new thing the web app asks of the app is a member of
  both; what the web app draws only in the app goes into `web/src/desktop/`.
- **What touches Tauri** stays in the program (`src/window.rs`, or a module beside it declared in `src/main.rs`); the
  library holds what is tested without a window. On Windows a test executable that links Tauri cannot start, since it
  lacks the program's manifest.
- **Commands** of the app are `#[tauri::command]` functions registered in `window::run`, declared in `build.rs`
  (`AppManifest::commands`) and allowed per page in `capabilities/`: the web app on the loopback gets a command only
  when `web-app.json` names it.
- **A server** is `server::Server`, checked with `server::check` before anything opens, and shown through a
  `proxy::Proxy` (`proxy::start`, then `Proxy::entry_url` in the window). Several servers (the rail) keep one `Proxy`
  each (each has its own port, cookie and origin, `Proxy::origin`), and dropping one stops everything it relays; the
  keychain holds their `Server::token`, which the app never writes to a file.
- The main window's label is `window::MAIN`.
