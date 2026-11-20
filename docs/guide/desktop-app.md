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

   The app checks that it matches the server: an app of `0.14.x` connects to a server of `0.14.x` only. Its version is
   in the file's name.
2. **Make an access token** for yourself on the server: the **My tokens** page of the web app (under Setup in the
   sidebar), or ask an owner to make one for you on the People page. The server's own token (`.moebius/token`) works
   too, and acts as the first owner.
3. **Start the app and connect.** The window asks for the server's address (`127.0.0.1:7171`, or the address the
   server listens on; `https://` for a server behind TLS) and the token. Connect opens the web app, signed in as the
   person the token belongs to. The window has no frame of the system: drag the strip along its top to move it, double
   click it to maximize, and minimize, maximize and close with the buttons at its right end.
4. **Open the quick window from anywhere.** While the app runs, even behind other windows or minimized, press
   `Ctrl+Shift+Space`: a small window opens over everything. **Ask Moebius** sends what you type to the Moebius agent;
   **New issue** makes an issue in the project you pick (the first line is the title, the rest the description).
   Enter sends and the window goes; the chat, or the new card on its board, opens in the app's main window. Shift+Enter
   is a new line, Escape closes the window without sending, and the shortcut again closes it too. With no server
   connected the shortcut brings the connect page forward instead.
5. **Change the shortcut** with **Change** at the foot of the quick window, then press the keys you want (one key with
   Ctrl, Alt or Super held; Shift alone is not enough, as the shortcut would take over typing). It is kept in
   `quick.json` in the app's configuration directory (`~/.config/io.github.code000-211.moebius/` on Linux,
   `%APPDATA%\io.github.code000-211.moebius\` on Windows) and registered again at every start.

### A server on this machine

On Linux, and on Windows inside WSL 2, the connect page also shows **A server on this machine**, and nothing is
installed until you press its button. **Run a server on this machine** does what `moebius connect` does on a host:

1. downloads `moebius` of the app's own release (`moebius-x86_64-unknown-linux-gnu` from the release page), checks it
   against the release's `SHA256SUMS` and puts it in `~/.moebius/bin/moebius` (a `moebius` of the same release line that
   is installed already, in `~/.moebius/bin` or on the `PATH`, is used instead);
2. runs `moebius init` in `~/.moebius/server`, which makes the configuration and the harness configuration repository;
3. starts `moebius serve` there in the background (its output is `~/.moebius/server/serve.log`) and waits for its token;
4. connects to it with that token (the server's own token acts as the first owner); the token never goes to the page.

Leaving the form below as it is declines: the page only looks at the machine, so nothing is downloaded or written. The
server keeps running when the app is closed (on Windows it runs while the app is open, because WSL stops a distribution
that has nothing running). The next start of the app finds the same server again: the page offers **Connect to it**, or
**Start it and connect** when it has stopped (after a reboot, say), without a new `init`. Releases are built for Linux
x86_64 only, so another system gets no offer. On Windows, WSL 2 has to be installed (`wsl --install`).

## Several servers

The rail at the far left of the window holds a square for every server the app knows: the first letter of its address,
the count of its Inbox in a badge, and a grey dot when it does not answer. The open one is raised. The app asks each server for these few figures
with one small request (`GET /v1/status`) every ten seconds, and takes the open server's from the tray, which follows
it by its events, so a busy server is not read as a whole to draw a badge. Click a square to
open that server. The app keeps the page of every server you opened, so switching back to one shows it at once, as you
left it (the scroll, the filters, the open card), and the rail stays where it is; a server opened for the first time
loads out of sight while the page you are on stays, and shows when it is ready. To keep memory bounded the app lets
go of the pages of servers you have not looked at for half an hour, and of all but the three you left last; such a
server opens again on the page you left it on. **+** connects another one:

- **Address**: the server's address and an access token, as for the first one.
- **Over SSH**: a host you can log in to with keys or the agent (the app asks no question: `ssh` runs in batch mode). The
  app opens a tunnel to the server on that host and reads its token, as `moebius connect <host>` does. A host with no
  server there is not changed until you say **Yes, set it up**, which runs `moebius connect --yes` for you; that needs
  `moebius` on this machine.

A right click on a square offers **Remove server**, which forgets it and deletes its token. That is also how you
disconnect: signing out in the web app does nothing inside the app, since the app signs every request itself. The app
opens the server that was open when you closed it, and shows the connect page, with the list of the servers it knows,
when none answers.

Tokens are kept in the system's keychain (Secret Service on Linux, which GNOME Keyring or KWallet provide; Credential
Manager on Windows), never in a file. The list of servers, without tokens, is `servers.json` in the app's configuration
directory (`~/.config/io.github.code000-211.moebius` on Linux).

The app needs a keychain that answers. Every call to it has a limit of 8 seconds, so a keychain that is installed and not
running (GNOME Keyring or KWallet that is not started, or locked without a prompt: common on minimal desktops, remote
sessions and tiling window managers) ends the connection with an error that names the remedy, and the button says
**Saving the token in the keychain…** while it waits. Install or unlock a keyring, or press **Connect for this session
only**: the token then stays in memory, is written to no file, and is asked for again at the next start (the server is not
in the list of remembered ones). When the app starts and cannot read the token of a remembered server because the
keychain does not answer, it shows the connect page with that message instead of waiting.

### The app runs once

Starting the app while it already runs does not make a second one: the second start brings the first one's window
forward (also when it was minimized, and showing its connect page when no server is connected) and ends. So there is one
window, one tray icon and one global shortcut. The first app keeps a lock, `instance.lock`, in the app's configuration
directory; it is released when the app ends, however it ended.

### Notifications

What reaches the Mentions tab of your Inbox (a card given to you, a mention, a change to a card you
follow, which includes a change that is ready to merge) is also a notification of your system, once. Choosing it brings
the window forward and opens the card or item, and reads the notice. The count of unread notices is on the app's icon
(the launcher's badge on Linux where it shows one, the red number over the taskbar icon on Windows) and in the
window's title, `(3) Moebius`, for the server that is open; reading a notice, here or in a browser, lowers it. What was unread when the app started
is counted and not announced.

### The tray icon

Once the app is running, its icon sits in the tray (the notification area of Windows, the
status area of a Linux desktop that shows tray icons, which GNOME needs an extension for). Its menu shows the
server's state: **the agents working**, **the Inbox**, and **today's tokens** against the day's budget. These
figures follow the server's events. Under them: **New issue** and **Ask Moebius** bring the window up and open the
form of a new issue or a new chat, **Open Moebius** brings the window up, also when it was minimized, and **Pause all
agents** holds every run of the server and starts none until you choose **Resume all agents**, which the same item
says while they are paused. Hovering the icon shows the same figures. Closing the window quits the app and its
icon, so that no invisible app is left behind on a desktop that shows no tray; to keep the shortcut, the notifications
and the tray going, minimize the window instead.

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
- Press `Ctrl+Shift+Space` with the app in the back, type a title under **New issue** and press Enter: the window goes
  and the card is on the project's board. Change the shortcut, close the app and start it again: the new one opens the
  window.
- Revoke the token on the My tokens page and start the app again: the server's square has a grey dot, and opening it
  answers that the server does not accept the token. Remove it and connect again with a new token.
- Connect a second server: its square appears in the rail; switch between them, and each comes back at once, as you left it (scroll down a page, switch away and back: it is where it was).

- Have someone give you a card (or mention you in a comment): a notification appears, the count rises by one, and
  choosing it opens the card.

- Choose **Pause all agents** in the tray: the item says **Resume all agents**, the first line says "(paused)", and
  `moebius pause` is not needed; the agents stand still until you resume them. From a terminal, `moebius pause` and
  `moebius resume` do the same (see [the desktop API](../reference/desktop-api.md#post-v1runspause-and-post-v1runsresume)).

## What can go wrong

- **The window is slow, blank or uses a whole core on Linux**: WebKitGTK's accelerated compositing has no working GPU
  path on virtual machines, remote desktops, Xvfb and some NVIDIA and Wayland setups. The app turns compositing off
  at start when `/dev/dri` has no render node. To choose yourself, start it with `MOEBIUS_RENDERING=software` (compositing
  off) or `MOEBIUS_RENDERING=gpu` (WebKitGTK's own choice); setting `WEBKIT_DISABLE_COMPOSITING_MODE` or
  `WEBKIT_DISABLE_DMABUF_RENDERER` yourself also stops the app from choosing. Windows is not affected.
- **No tray icon on Linux**: a desktop without a status area (GNOME without its AppIndicator extension) shows none.
  The app works without it, only its Pause all agents item is out of reach: use `moebius pause` then.

- **"the server runs Moebius 0.13.0, and this app is Moebius 0.14.0, which works with servers of 0.14 only"**: the
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
- **"cannot download ..." or "does not match the SHA256SUMS"** when running a server on this machine: the release of the
  app's version is not published (an app built from a branch has none), or the download is damaged. Nothing was installed;
  install `moebius` yourself in `~/.moebius/bin` or on the `PATH` and press the button again.
- **"the server did not answer within 90 seconds"**: read `~/.moebius/server/serve.log`; a port 7171 that is taken by
  another program is the usual reason (change `listen` in `~/.moebius/server/moebius.toml`).
- **"the system's keychain does not answer"**: nothing offers a Secret Service on this Linux session (install
  `gnome-keyring` or KWallet and sign in to it), the keychain is locked, or it does not answer within 8 seconds. The app
  keeps no token in a file, so it does not remember the server; **Connect for this session only** keeps the token in
  memory until the app closes.
- **"no server is set up on <host>"** over SSH: the login worked and there is no token on the host; say yes to set
  one up, or run `moebius connect <host>` in a terminal.
- **The shortcut does nothing, or the quick window says it could not be taken**: another program holds that key
  combination. Change it to another. On Linux the shortcut is a grab on the X11 server: under a Wayland session it
  works only where the compositor lets X11 clients grab keys, so choose a shortcut in the system's own keyboard
  settings that starts `moebius-desktop` if it does not.
- **No notification appears on Linux**: the desktop needs a notification service (`org.freedesktop.Notifications`,
  which GNOME, KDE and most others have). Without one the count still shows in the title.
