# Running Moebius

## When you want this

You want a Moebius server that stays up: installed from a release instead of built from source, started once, kept
running by the operating system, reachable from the desktop app on your own machine or on another one, and upgraded and
backed up without losing a card. Releases are built for Linux x86_64, the server (`moebius`) and the desktop app
(`moebius-desktop`) in an archive each, and for Windows x86_64, both in `moebius-windows-x86_64.zip`; on another
system, build from source. The server is documented and tested on Linux and on WSL 2; on Windows, run the app there and
the server in WSL (see [On Windows](#on-windows)). Every key named here is in the
[configuration reference](../reference/configuration.md).

## The steps

0a. **Forward ports.** When an agent or a terminal on the server starts a dev server, the Ports page (Setup, and the
   "Ports" button of the live view, the chat and the Terminal page) lists it and forwards it to the same port number on
   `127.0.0.1` here, or to a free one; "Open in browser" opens it, and a banner offers that when the port opens in the run,
   chat or terminal you are looking at. "Forward a port…" forwards one by hand, "Change local port" and "Stop forwarding"
   do what they say. It works the same with an address and over SSH, since each connection is carried over the API
   (`GET /v1/ports/{port}/tunnel`) and the server listens on nothing new. The forwarded ports are remembered for each
   server in `desktop.toml` and forwarded again when the app connects (turn this off on the Ports page, or with
   `restore_forwards = false`). Detection needs Linux on the server.
0. **Connect over SSH** (the way that needs nothing but a login). In the desktop app's connect screen, press "Connect
   over SSH" and pick a host of your `~/.ssh/config`, or "Add new SSH host" and type `user@host` or a whole `ssh …`
   command, which is written to the config as a `Host` entry. The app uses the system's `ssh`, so keys, `ssh-agent`,
   `ProxyJump` and known hosts work as in a terminal; use key-based login. A password or passphrase that `ssh` asks for is
   shown in the app and never saved. The app then finds `moebius` on the host's `PATH` or in `~/.moebius/bin` (and offers
   to install the release, downloaded there or, when the host has no internet, downloaded here and uploaded), runs
   `moebius init` in `~/.moebius/server` if there is no configuration, starts `moebius serve` (a systemd user service
   where there is one, else detached), reads the token and opens `ssh -N -L` to the server. The steps and the log are on
   the screen. The server listens on `127.0.0.1` only and nothing else on the host needs to be reachable; if the tunnel
   drops, the sidebar says "Reconnecting…" and it is made again. Hosts connected to before are in the sidebar header's
   menu. On the command line, `moebius connect <host>` or `--ssh <host>` does the same tunnel (see
   [the command line](../reference/cli.md#connect-host)). The rest of the steps below are for installing by hand and for a
   server that listens on a private network.

1. Install from a release. Download the archive for your machine and `SHA256SUMS` from the
   [releases page](https://github.com/Code000-211/Moebius/releases), check the archive against the list, and unpack it.
   The server archive holds the program and the service file described below:

   ```sh
   VERSION=0.1.0
   TARGET=x86_64-unknown-linux-gnu
   ARCHIVE=moebius-$VERSION-$TARGET.tar.gz
   grep " $ARCHIVE\$" SHA256SUMS | sha256sum -c -
   tar -xzf "$ARCHIVE"
   mkdir -p ~/.local/bin
   cp "moebius-$VERSION-$TARGET/moebius" ~/.local/bin/
   moebius --version
   ```

   The check must print `OK`. Put `~/.local/bin` on your `PATH` if `moebius --version` is not found. The desktop archive
   holds the program, `moebius-desktop.png` and `moebius-desktop.desktop`. The server needs git and
   [Pi-Bolt](https://github.com/opensec-git/Pi-Bolt) installed as `pi-bolt`.

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

   An upstream can log in to a provider with OAuth instead of a key: write its client id and URLs in a
   `[router.upstreams.oauth]` table (see [configuration](../reference/configuration.md#routerupstreamsoauth)), then press
   "Log in" at the upstream on the Models page. Using a subscription through a third-party tool may break the
   provider's terms; the page asks you to confirm that you have read the warning before the login starts, and
   `moebius doctor` repeats it.

   **Log in from the app.** The quickest way is "Log in to a provider" on the Models page: choose Anthropic (a Claude
   Pro/Max subscription) or OpenAI (ChatGPT), or "Other" for a provider you give the client id and addresses of. The app
   adds the upstream to `moebius.toml` (live, no restart), shows a warning (Moebius uses the client id of Claude Code or
   of the Codex CLI, as Pi does, and the provider may block the account) and, once you confirm it, opens the provider's
   page in your browser. When the browser ends on this machine's `localhost` address the login finishes by itself. When
   it cannot (the server runs elsewhere, over SSH, or the provider shows a code), copy the code or the address from the
   address bar into "Paste the code or the address you ended on" and press "Finish login". With `preset = "anthropic"`
   or `"openai"` in the `oauth` table the same works from the file. Both work fully; a ChatGPT login serves the
   models of the subscription (`gpt-6-sol`, ...) through the router like any other upstream.

3. Run it as a service, so that it starts with you and starts again after a failure. Stop the `moebius serve` of the
   step before first (Ctrl+C).

   `moebius.service` from the archive is a systemd user unit. It starts
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

   Relative paths in `moebius.toml`, such as the default `data_dir = ".moebius"`, are resolved against the directory of
   the file, not against where the service started, so the unit works from any directory. A service does not read your
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

7. Work on GitLab. For repositories on gitlab.com, or on a GitLab you host, create an access token with the `api` scope
   (a personal one, or a project or group access token for one project or group). Give it to the server in a file only
   you can read, or as a secret (`moebius secrets set gitlab` reads it from standard input; then write
   `token_secret = "gitlab"` instead of `token_file`), so that it is never in `moebius.toml`:

   ```sh
   printf '%s' "$GITLAB_TOKEN" > ~/.config/moebius/gitlab-token
   chmod 600 ~/.config/moebius/gitlab-token
   cat >> ~/.config/moebius/moebius.toml <<'TOML'
   [gitlab]
   url = "https://gitlab.com"
   token_file = "gitlab-token"
   TOML
   systemctl --user restart moebius
   moebius repo add widgets https://gitlab.com/acme/platform/widgets.git --gitlab acme/platform/widgets --trusted leon
   ```

   In the app, the "+" of the sidebar does the same with the forge GitLab. The server polls the project's issues every
   `gitlab.poll_seconds`: open issues become cards, issues by the trusted usernames move on without your approval, and
   "blocked by #n" in an issue becomes a dependency. Changes are pushed with the token and opened as merge requests; the
   latest pipeline of a merge request's head commit is its checks, an approval is its review, and a merge on GitLab, or
   in the app, finishes its card. See [configuration](../reference/configuration.md#gitlab).

8. Work on Codeberg, or another Forgejo or Gitea. In the forge's settings (Settings, Applications), generate an access
   token with the scopes `write:issue` and `write:repository`. Give it to the server in a file only you can read, or as
   a secret (`moebius secrets set codeberg`, then `token_secret = "codeberg"`). Without `url`, the server reaches
   Codeberg; for a Forgejo or Gitea of your own, set `url` to its web address:

   ```sh
   printf '%s' "$CODEBERG_TOKEN" > ~/.config/moebius/forgejo-token
   chmod 600 ~/.config/moebius/forgejo-token
   cat >> ~/.config/moebius/moebius.toml <<'TOML'
   [forgejo]
   url = "https://codeberg.org"
   token_file = "forgejo-token"
   TOML
   systemctl --user restart moebius
   moebius repo add gadgets https://codeberg.org/acme/gadgets.git --forgejo acme/gadgets --trusted leon
   ```

   In the app, the "+" of the sidebar does the same with the forge Forgejo. The server polls the repository's issues
   every `forgejo.poll_seconds`: open issues become cards, issues by the trusted logins move on without your approval,
   and "blocked by #n" in an issue becomes a dependency. Changes are pushed with the token and opened as pull requests;
   the statuses of a pull request's head commit (those of Forgejo Actions, or any CI that reports them) are its checks,
   its latest review is its review, and a merge on the forge, or in the app, finishes its card. One server reaches one
   Forgejo. See [configuration](../reference/configuration.md#forgejo).

9. Run harnesses on another machine. A GPU box, a bigger build machine or a machine with tools the server lacks can run
   the harness of an agent's runs over SSH, while the server keeps the board, the router, the checks and the store. On
   that machine install `git`, `sshd` and the harness (`pi-bolt`, and `claude` for agents on Claude Code), and let the
   server's user log in with a key and no passphrase; log in once by hand so that the host key is in the server's
   `known_hosts`, because Moebius never answers an SSH question. Then name the machine in `moebius.toml` and send an
   agent there:

   ```toml
   [[runners]]
   name = "gpu-box"
   host = "gpu.example.com"
   user = "moebius"
   key_file = "gpu-box.key"
   working_dir = "/srv/moebius"
   max_runs = 2
   bin_dir = "/usr/local/bin"

   [agents.Coder]
   model = "qwen3.8-flash-next"
   runner = "gpu-box"
   ```

   Restart the server and check `moebius runners`. A run of the Coder now pushes its branch over SSH to a clone under
   `/srv/moebius/repos/`, starts the harness in a checkout under `/srv/moebius/runs/<id>/work`, and fetches what the
   agent committed back into the card's worktree when it ends, so the rest of the card (the checks, the review, the
   change) goes on at the server as always. The machine needs no access to the server's network: the harness reaches the
   router, the tool-call check and the MCP gateway through an SSH reverse tunnel that the server holds open for the
   run, so every tool call is still checked by the server. At most `max_runs` runs go on the machine; one more is
   not started: `moebius.start_run` raises `the runner gpu-box is busy`, the card waits with that as its `script error`,
   and its script is tried again on the next tick, so the card starts once a run on the machine has ended. A column script can choose the machine for one run:
   `moebius.start_run("Coder", "code", card.note, "gpu-box")`. Chats, the Retro and the Autoresearcher always run on the
   server. The live view shows a run's runner. See [configuration](../reference/configuration.md#runners).

## On Windows

The desktop app and the `moebius` commands that operate a server run on Windows. Download
`moebius-windows-x86_64.zip` and `SHA256SUMS`, check the archive and unpack it, in PowerShell:

```powershell
(Get-FileHash moebius-windows-x86_64.zip -Algorithm SHA256).Hash.ToLower()
Select-String moebius-windows-x86_64.zip SHA256SUMS
Expand-Archive moebius-windows-x86_64.zip -DestinationPath $env:LOCALAPPDATA\Programs
& "$env:LOCALAPPDATA\Programs\moebius-windows-x86_64\moebius-desktop.exe"
```

The two hashes must be the same. The app keeps its address and token in `%APPDATA%\moebius\desktop.toml`, where the
`moebius` commands find them too.

Run the server on Linux or in WSL 2, as the steps above show: install a Linux distribution with `wsl --install`, and in
it install git, Pi-Bolt and the Linux release. WSL forwards `127.0.0.1:7171` to Windows, so the app reaches a server
started in WSL at `http://127.0.0.1:7171` with the token from `.moebius/token` (`wsl cat ~/.config/moebius/.moebius/token`
prints it). To keep it running, enable systemd in WSL (`[boot] systemd=true` in `/etc/wsl.conf`) and use the unit of
step 3.

`moebius.exe serve` also builds and starts on Windows itself, but that is not tested, and these parts need Linux or WSL:

- A repository's own checks (`.moebius/checks`), replays of refused calls and research measures run `sh` and `bash`.
  Without them they fail and say so.
- Files that hold secrets (`secrets.toml`, the token, a run's settings) keep the permissions of their folder instead of
  being readable by their owner only; under your profile that is you and the administrators.
- A program that is stopped is ended with `taskkill /T`, which misses a process whose parent has already ended.
- Pi-Bolt and Claude Code are found on the `PATH` only as `.exe` files: name another one by its full path, with its
  `.cmd` or `.exe`, in `pi_bolt.program` or `claude_code.program`. The terminal pages open PowerShell, and a Claude Code run
  gets copies of its skills instead of links.

## How to try it

Check that the service is up and answers with the token, from the machine that runs it and, once `listen` is changed,
from the other one:

```sh
curl -s -H "Authorization: Bearer $(cat ~/.config/moebius/.moebius/token)" http://127.0.0.1:7171/v1/snapshot
moebius board
```

Kill the server (`systemctl --user kill -s KILL moebius`) and see it start again after a few seconds in
`systemctl --user status moebius`. To see the checksum catch a bad download, change one byte of a copy of the archive and
run the `sha256sum` line of step 1 on it: it must print `FAILED`.

## What can go wrong

- `sha256sum` prints `FAILED` or `no properly formatted checksum lines found`: the archive is damaged or is not the one
  `SHA256SUMS` lists. Download it again; do not unpack it.
- `cannot listen on 127.0.0.1:7171: Address already in use`: another server, or the `moebius serve` of step 2, still runs.
  Stop it, or give this one another `listen`.
- The service starts and stops at once: read `journalctl --user -u moebius`. A
  configuration error is printed there, with the key at fault. The unit starts the server with `--config`, so a
  `moebius.toml` in another place is not read, and a missing file is not an error: the server then runs with the
  defaults, which have no upstream and no agent.
- GitLab repositories report `forge_error` with `401` or `403`: the token expired, lacks the `api` scope, or (a project
  or group token) does not reach the project. With `404`, the project's path is not the one GitLab shows under the
  project's name, groups first.
- Forgejo repositories report `forge_error` with `401` or `403`: the token was revoked or lacks a scope. With `404`, the
  repository is not `owner/repo` as its address on the forge shows it, or the token's user cannot see it.
- Runs fail because `pi-bolt` is not found: the service's `PATH` is not yours. Set `pi_bolt.program` to the full path.
- A run on a runner fails at once with `gpu.example.com failed: ...` or `the tunnel to gpu.example.com did not come up`:
  log in from the server's user with the same command Moebius uses (`ssh -o BatchMode=yes -i <key_file> <user>@<host>
  true`). The usual causes are a key that asks for a passphrase, a host key that is not in `known_hosts`, a
  `working_dir` the user cannot create, and an `sshd` that forbids remote forwarding (`AllowTcpForwarding no`).
- A run on a runner fails with `command not found`: the harness is not on the `PATH` of a non-interactive login. Set
  `bin_dir` to its directory.
- The app cannot connect from another machine: the server still listens on `127.0.0.1`, or a firewall blocks the port.
  `listen` takes an address of the machine, and a change of it needs a restart.
- In a Wayland session on Linux the desktop app opens its window through XWayland, because through Wayland itself it
  cannot reach the clipboard. Without XWayland (no `DISPLAY`) it opens on Wayland, and nothing can be copied or pasted
  in it.
- The app says the token is wrong after a restore or a move: the token is the content of `.moebius/token` of the server
  it talks to; copy it again.
- On Windows, a change's checks say `` `sh` was not found: this needs Linux or WSL ``: the server runs on Windows itself.
  Run it in WSL, as [On Windows](#on-windows) shows.
- An older release started on a store that a newer one has migrated: it is not supported and may fail in odd ways.
  Install the newer release again, or restore the backup made before the upgrade.
