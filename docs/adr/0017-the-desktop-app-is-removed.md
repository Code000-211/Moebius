# The desktop app is removed; `moebius connect` sets a server up over SSH

Supersedes [0013](0013-the-web-app-is-the-front-end.md), which kept the iced app until the web app had been used for a
release. It has been used for six. The desktop app (`crates/moebius-desktop`) is removed, and the one thing only it did,
setting a server up on another machine over SSH, moves to the command line.

## Considered Options

- Keep the desktop app without new features: it must still compile and pass its tests for every change to the API, and
  its pictures and render tests are a cost on every change to the interface that no one reads; `moebius open` already
  shows the web app in a window of its own, and a browser reaches a server from anywhere with nothing installed.
- Drop the SSH setup with the app: a server on a machine one can only log in to would then need a manual install, an
  `init` and a service unit, which is what the setup saved people from.
- Move the setup into `moebius connect <host>`, which already holds the tunnel and opens the window.

## Decision

- The web app is the only front end. `docs/design.md` and its mockups go with the app; `docs/design-web.md` is the
  design standard ([0016](0016-the-web-app-has-its-own-design-standard.md)). Nothing moves, because the web app's tokens
  are where they are to stay.
- `moebius connect <host>` sets the server up when the login works but no server answers on the host. It finds the
  system and `moebius` (on the `PATH`, else `~/.moebius/bin`), installs the release of its own version after asking when
  it is missing or older than the program, runs `moebius init` in `~/.moebius/server` when there is no configuration,
  starts the server, waits for its token, and then opens the tunnel and the window as before. The steps are those the
  app took, run as `ssh` calls from the terminal over one shared connection (where `ssh` can share one), with the
  system's `ssh` asking for a password once on that terminal. A server that already runs on the host (it answers, or
  `moebius.service` or `moebius-connect.service` is active) is never set up over.
- On a host with systemd the server is a systemd user service, `moebius-connect.service` (not `moebius.service`, the
  name the guide gives to a unit installed by hand), which `connect` installs and enables, and `loginctl
  enable-linger` is tried so that it survives a logout; the question says so. Without systemd the server is started
  detached. The directory and the log are private (`umask 077`), because the log holds a login link.
- The program is checked against the `SHA256SUMS` of its release before it is put in place, and the setup fails
  closed without a tool for the hash. That guards against a damaged download or a bad mirror; the list comes from the
  same place as the program, so it does not guard against a compromised release.
- Nothing on the host changes before a yes. In a terminal `connect` asks `[y/N]` once, naming what will be installed
  (including the systemd user service); `--yes` answers for scripts;
  without a terminal and without `--yes` it refuses and changes nothing. `--upload` sends the program from this machine
  to a host without internet.
- The command line keeps its own settings in `<config dir>/moebius/cli.toml` (`address`, `token`, `ssh_program`), and
  reads `desktop.toml` from earlier releases when `cli.toml` is missing. `connected.toml` stays beside it.
- The tests of the app that started a real server and used the API through its client are checked against the server's
  own tests; what they covered and the server's tests did not was moved into `crates/moebius-core/tests`.

## Consequences

- The Windows archive holds `moebius.exe`, `LICENSE` and `NOTICE`; there is no desktop archive, icon or desktop entry.
  CI no longer tests the app on Windows.
- Forwarding a port of the server to this machine, which the app did for the ports a run or a terminal opens, is no
  longer done for the person: the Ports page of the web app shows the `ssh -L` command that does it. The route
  (`GET /v1/ports/{port}/tunnel`) stays for a client that carries connections itself.
- `PUT /v1/client`, where the app reported where it ran, stays in the API; the web app does not use it, so the Moebius
  agent's "Where Moebius runs" says that no app has reported.
- The HTTP API is still called the desktop API in the documentation, the routes' reference and the tests. The name is a
  habit now, not a description; renaming it touches every page that links to `desktop-api.md`.
- ADR [0003](0003-headless-server-and-native-desktop-app.md) and [0011](0011-iced-for-the-desktop-app.md) stay as
  records.
