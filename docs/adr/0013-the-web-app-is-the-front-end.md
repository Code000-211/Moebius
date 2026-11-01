# The web app is the front end; the iced app stays until it has been used for a release

Supersedes [0011](0011-iced-for-the-desktop-app.md) as the choice of front end. The server serves a web app (`web/`,
React) on the address of its API, behind the same token (a browser signs in once with a code: `moebius web`). From now
on the web app is the front end of Moebius: a new page is built there first, and the iced app gets nothing new unless a
page that exists in both would otherwise break.

## Considered Options

- Keep iced as the only front end: a second front end was started because a browser reaches a server from anywhere with
  nothing installed, and because React, a browser's text rendering, accessibility and editing widgets need no code of
  ours.
- Wrap the web app in Electron or Tauri to get a window: Electron adds a browser to every release (a download of about
  100 MB for what is a window around a page), and Tauri uses WebKitGTK on Linux, which renders and behaves differently
  from the Chromium of the other systems. Electron can come later, if a window needs what a browser's app mode cannot
  give (a tray icon, an address that opens `moebius://`); nothing in this decision stands in its way.
- Remove the iced app now: the web app has not yet been used day to day, so what the iced app does that it does not
  would be lost unseen.

## Consequences

- `moebius open` is the window: it asks the running server for a one-time login code and starts Chromium, Chrome, Edge
  or Brave in app mode (`--app=<address>`) with a profile folder of its own, so that the window keeps its own cookie. It
  falls back to the default browser. `moebius connect <host>` does for the web app what the iced app does over SSH: the
  same `ssh -L` tunnel, then the window on the forwarded port. See [the command line](../reference/cli.md#open---browser-path).
- Releases need nothing new: the web app is built into the server (`release.yml`), so a server serves it wherever it runs.
- The iced app (`crates/moebius-desktop`) stays and is kept working, by the tests it has, until the web app has been used
  day to day for a release. Then a later issue removes it. Removing it means:
  - the crate `crates/moebius-desktop`, and its entries in the workspace, CI, `release.yml` and `dist/package.sh`
    (`moebius-desktop` archives, the icon and `moebius-desktop.desktop`);
  - its picture tests (`tests/snapshots/`) and render tests, of which what is not covered by the web app's tests is
    ported to them first;
  - the SSH setup the app does (installing and starting `moebius` on a host), which `moebius connect` does not do: it
    moves to the command line or is dropped;
  - the documentation that describes it: `docs/design.md` (the theme's source of truth moves to
    `web/src/styles/`), the screenshots in `README.md`, the desktop sections of the guides and `docs/architecture.md`,
    and ADR 0011 stays as a record.
- The desktop API is the same for both and stays the contract; the command line does not depend on either front end.
