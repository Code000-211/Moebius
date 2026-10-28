# Running Moebius

## When you want this

You want a Moebius server that stays up: installed from a release instead of built from source, started once, kept
running by the operating system, reachable from the desktop app on your own machine or on another one, and upgraded and
backed up without losing a card. Releases are built for the server (`moebius`) on Linux x86_64 and macOS arm64, and for
the desktop app (`moebius-desktop`) on Linux x86_64, macOS arm64 (as `Moebius.app`) and Windows x86_64. The server is not
built for Windows. Every key named here is in the [configuration reference](../reference/configuration.md).

## The steps

1. Install from a release. Download the archive for your machine and `SHA256SUMS` from the
   [releases page](https://github.com/Code000-211/Moebius/releases), check the archive against the list, and unpack it.
   The server archive holds the program and the two service files described below:

   ```sh
   VERSION=0.1.0
   TARGET=x86_64-unknown-linux-gnu   # or aarch64-apple-darwin
   ARCHIVE=moebius-$VERSION-$TARGET.tar.gz
   grep " $ARCHIVE\$" SHA256SUMS | shasum -a 256 -c -
   tar -xzf "$ARCHIVE"
   mkdir -p ~/.local/bin
   cp "moebius-$VERSION-$TARGET/moebius" ~/.local/bin/
   moebius --version
   ```

   `shasum -a 256` is on macOS and on most Linux systems; `sha256sum -c -` does the same where it is installed. The
   check must print `OK`. Put `~/.local/bin` on your `PATH` if `moebius --version` is not found. The desktop archive
   holds `Moebius.app` on macOS (copy it to `/Applications`; it is not signed, so the first time, open it with a right
   click and Open), `moebius-desktop.exe` on Windows, and the program, `moebius-desktop.png` and
   `moebius-desktop.desktop` on Linux. The server needs git and [Pi-Bolt](https://github.com/opensec-git/Pi-Bolt)
   installed as `pi-bolt`.

2. First start. `moebius init` writes a commented `moebius.toml` with every default; add an upstream, the Pi-Bolt
   program and the Coder agent as the [README](../../README.md#quick-start) shows. `moebius serve` creates `.moebius/`
   next to the file, with the token in `.moebius/token`:

   ```sh
   mkdir -p ~/.config/moebius
   cd ~/.config/moebius
   moebius init
   moebius serve
   ```

   Open the desktop app and give it the address (`http://127.0.0.1:7171`) and the content of `.moebius/token`. On the
   machine that runs the server the `moebius` commands find both by themselves when you run them in the directory of
   `moebius.toml`, or with `--config`.

3. Run it as a service, so that it starts with you and starts again after a failure. Stop the `moebius serve` of the
   step before first (Ctrl+C).

   On Linux, `moebius.service` from the archive is a systemd user unit. It starts
   `moebius serve --config %h/.config/moebius/moebius.toml` and restarts it when it fails:

   ```sh
   mkdir -p ~/.config/systemd/user
   cp "moebius-$VERSION-$TARGET/moebius.service" ~/.config/systemd/user/
   systemctl --user daemon-reload
   systemctl --user enable --now moebius
   systemctl --user status moebius
   journalctl --user -u moebius -f
   ```

   Run `loginctl enable-linger "$USER"` once to keep it running while you are logged out.

   On macOS, `dev.moebius.server.plist` is a launchd agent with `KeepAlive`; it writes its log to
   `~/Library/Logs/moebius.log`. launchd does not expand `~`, so replace `/Users/you` in the file with your home
   directory, and move `moebius.toml` to `~/Library/Application Support/moebius/` (or change the paths in the file),
   because that is where the agent looks for it:

   ```sh
   mkdir -p ~/Library/LaunchAgents ~/Library/Logs "$HOME/Library/Application Support/moebius"
   sed "s|/Users/you|$HOME|g" "moebius-$VERSION-$TARGET/dev.moebius.server.plist" > ~/Library/LaunchAgents/dev.moebius.server.plist
   sudo cp "moebius-$VERSION-$TARGET/moebius" /usr/local/bin/moebius
   launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/dev.moebius.server.plist
   tail -f ~/Library/Logs/moebius.log
   ```

   Relative paths in `moebius.toml`, such as the default `data_dir = ".moebius"`, are resolved against the directory of
   the file, not against where the service started, so both units work from any directory. A service does not read your
   shell profile: name the `pi-bolt` program in `pi_bolt.program`, or keep it in a directory on the `PATH` the unit sets.

4. Reach it from another machine. The server listens on `127.0.0.1:7171`, where only the machine itself can reach it.
   To use the desktop app from another machine of a private network, set `listen` to an address of this machine on that
   network, such as `192.168.1.20:7171` (an `ip:port`, not a host name), and restart the server. The address is plain
   HTTP and anyone who has the token has full control, so use a private network, a VPN or an SSH tunnel
   (`ssh -L 7171:127.0.0.1:7171 server`), and never expose the port to the internet. The model router
   (`router.listen`) must stay on a loopback address, and the app does not need it. In the app, enter the address of the
   server and the token, which you copy from `.moebius/token` on the server.

5. Upgrade. Install the new release over the old one and restart the service. The store migrates itself when the server
   starts and the change cannot be undone, so make the backup of the next step first if you may want to go back to the old
   release:

   ```sh
   cp "moebius-$NEW-$TARGET/moebius" ~/.local/bin/moebius
   systemctl --user restart moebius
   ```

   On macOS, copy the program to `/usr/local/bin/moebius` and run
   `launchctl kickstart -k "gui/$(id -u)/dev.moebius.server"`.

6. Back up. Everything Moebius keeps is in the data directory (`.moebius/` next to `moebius.toml`, unless `data_dir`
   says otherwise): the store, the blobs, the clones, the token and the keys of `secrets.toml`. Stop the server so that
   the store is not copied halfway, copy the directory and `moebius.toml`, and start the server again. Keep the copy
   private, because it holds the keys:

   ```sh
   systemctl --user stop moebius
   tar -czf "moebius-backup-$(date +%F).tar.gz" -C ~/.config/moebius .
   systemctl --user start moebius
   ```

   To restore, stop the server, unpack the archive into the empty directory and start it again.

## How to try it

Check that the service is up and answers with the token, from the machine that runs it and, once `listen` is changed,
from the other one:

```sh
curl -s -H "Authorization: Bearer $(cat ~/.config/moebius/.moebius/token)" http://127.0.0.1:7171/v1/snapshot
moebius board
```

Kill the server (`systemctl --user kill -s KILL moebius`) and see it start again after a few seconds in
`systemctl --user status moebius`. To see the checksum catch a bad download, change one byte of a copy of the archive and
run the `shasum` line of step 1 on it: it must print `FAILED`.

## What can go wrong

- `shasum` prints `FAILED` or `no properly formatted checksum lines found`: the archive is damaged or is not the one
  `SHA256SUMS` lists. Download it again; do not unpack it.
- `cannot listen on 127.0.0.1:7171: Address already in use`: another server, or the `moebius serve` of step 2, still runs.
  Stop it, or give this one another `listen`.
- The service starts and stops at once: read `journalctl --user -u moebius` or `~/Library/Logs/moebius.log`. A
  configuration error is printed there, with the key at fault. The unit starts the server with `--config`, so a
  `moebius.toml` in another place is not read, and a missing file is not an error: the server then runs with the
  defaults, which have no upstream and no agent.
- Runs fail because `pi-bolt` is not found: the service's `PATH` is not yours. Set `pi_bolt.program` to the full path.
- The app cannot connect from another machine: the server still listens on `127.0.0.1`, or a firewall blocks the port.
  `listen` takes an address of the machine, and a change of it needs a restart.
- The app says the token is wrong after a restore or a move: the token is the content of `.moebius/token` of the server
  it talks to; copy it again.
- An older release started on a store that a newer one has migrated: it is not supported and may fail in odd ways.
  Install the newer release again, or restore the backup made before the upgrade.
