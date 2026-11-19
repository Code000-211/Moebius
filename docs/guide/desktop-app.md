# The desktop app

## When you want this

You keep Moebius open all day and want it as an app of its own, in the taskbar or the dock, rather than as a browser
tab or the window of `moebius open`. The desktop app is the web app of the same release in a window of its own
([ADR 0021](../adr/0021-a-desktop-app-in-tauri.md)): it looks and works the same, and it connects to a server with an
access token. It is built for Linux x86_64 (an AppImage and a `.deb`) and for Windows x86_64 (an installer); there is no
macOS build. `moebius open` and `moebius connect` stay for everyone without it.

## The steps

1. **Download the app of the server's release** from the release page on GitHub
   (`https://github.com/Code000-211/Moebius/releases/tag/v<version>`):
   - `moebius-desktop-<version>-x86_64.AppImage`: any Linux x86_64 with glibc 2.35 or later (Ubuntu 22.04 and
     newer); it carries WebKitGTK and GTK with it. Make it executable
     (`chmod +x moebius-desktop-<version>-x86_64.AppImage`) and start it.
   - `moebius-desktop-<version>-amd64.deb`: Debian and Ubuntu (`sudo apt install ./moebius-desktop-<version>-amd64.deb`),
     which brings WebKitGTK with it; start Moebius from the applications menu, or `moebius-desktop` in a terminal.
   - `moebius-desktop-<version>-x86_64-setup.exe`: Windows 10 and 11, which have the WebView2 runtime the app draws with
     (the installer fetches it where it is missing). The app is not signed, so on its first start Windows SmartScreen
     says it "protected your PC": choose **More info**, then **Run anyway**.

   The app checks that it matches the server: an app of `0.13.x` connects to a server of `0.13.x` only. Its version is
   in the file's name.
2. **Make an access token** for yourself on the server: the **My tokens** page of the web app (under Setup in the
   sidebar), or ask an owner to make one for you on the People page. The server's own token (`.moebius/token`) works
   too, and acts as the first owner.
3. **Start the app and connect.** The window asks for the server's address (`127.0.0.1:7171`, or the address the
   server listens on; `https://` for a server behind TLS) and the token. Connect opens the web app, signed in as the
   person the token belongs to. The window has no frame of the system: drag the strip along its top to move it, double
   click it to maximize, and minimize, maximize and close with the buttons at its right end.

A server on another machine is reached as the web app reaches it: on its address if it listens on the network, or
through the tunnel of `moebius connect <host> --no-window`, then `127.0.0.1:7171` in the app (see
[Running Moebius](running-moebius.md)).

## An example

A server started on this machine with `moebius serve` in `~/moebius`, and a token made for the app with the command
line:

```sh
moebius api POST /v1/tokens --body '{"name": "desktop app"}'
```

The answer's `token` (it starts with `moebius-access-`) goes into the app with the address `127.0.0.1:7171`.

## How to try it

- The app shows the Inbox of the server, and the sidebar's connection says **Online**: the event stream reaches the
  server through the app.
- Revoke the token on the My tokens page and start the app again: Connect answers that the server does not accept the
  token.

## What can go wrong

- **"the server runs Moebius 0.12.4, and this app is Moebius 0.13.0, which works with servers of 0.13 only"**: the
  app and the server are of different releases. The message names the release whose app matches the server; install
  that one, or upgrade the server ([Running Moebius](running-moebius.md)). A server older than the first release with
  a desktop app has no app of its own: upgrade it.
- **"the server does not accept this access token"**: the token is wrong, revoked, or belongs to a person who is
  disabled. Make a new one.
- **"cannot reach ..."**: nothing answers at that address from this machine. Check the address and that the server
  runs; for a server on another machine, check that it listens on the network or that the tunnel is open.
- **"... answered 404 where a Moebius server answers its version"**: something else answers at that address, such as
  another program on the port.
- **The AppImage does not start on Linux** with an error about FUSE (`dlopen(): error loading libfuse.so.2`): the system
  cannot mount AppImages. Install FUSE 2 (`libfuse2` on Ubuntu 22.04, `libfuse2t64` on 24.04), or start it with
  `--appimage-extract-and-run`, or use the `.deb`.
- The token is kept in the app's memory only, for now: the app asks for it on every start.
