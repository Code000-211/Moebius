# A launcher plugin wraps how a harness process starts

Amended 2026-11-05: the agent directory is read-write, not read-only as the first launcher's paragraph below says,
because Pi keeps its credential store there; the request's `read_write` names it and the sessions and temporary
directories.

Amended 2026-11-05: several plugins may be launchers, each for the agents it names (`launch_agents`), so the Moebius
agent, which the sandboxes leave out because it must reach the desktop API, can have a launcher of its own. A run goes
to the first launcher in the file that takes its agent; the last launcher must take every agent, so no run starts
unwrapped while a launcher is configured (the review of the change found the gap, and the configuration is refused
without it). The fail-closed rule below holds for the launcher a run is given.

Runs are the part of Moebius that does what no one wrote down: an agent runs as the server's user, in the server's file
system, with a model's idea of what to type. The guard sees what a tool call says, not what it does. A sandbox the
kernel enforces makes the boundary real, but which sandbox is right differs by machine and by need (nono on a laptop, a
container for a toolchain of its own, nothing at all on a throwaway VM), and ADR 0009 keeps such choices out of the core.
So the core gets one generic hook, a **launcher**, and a sandbox is a plugin that implements it.

## The decision

A plugin of the new kind `launcher` (`[[plugins]]` with `launches = true`; see the amendment above for several) is asked before the core starts
a harness process. It is sent the run (id, purpose, agent, chat or not, card), the program, arguments and complete
environment, the working directory, the paths the process must read and those it must read and write, and the addresses
it must reach (router, MCP gateway, tool-call check). It answers `unchanged`, or a wrapped launch: the program,
arguments and complete environment to start instead, the addresses as seen from inside, a stop command, a cleanup
command and files to remove. The core starts the wrapped program and speaks the harness's protocol over its standard
input and output as before. It runs the stop command whenever it stops, kills or restarts the process (and still kills
the wrapper's own process group), the cleanup command when the process has ended, and asks the launcher at its start for
what earlier runs left. The reference is `docs/reference/plugin-api.md#launchers`.

- **Fail closed.** A launcher that errors, does not answer within the host's time limit, answers nonsense, is not
  running or is not a launcher fails the run before the harness starts. The core never falls back to an unwrapped start
  for a run its launcher is asked about, and a server whose launcher cannot name its leftovers does not start.
- **Nothing changes without one.** Without a launcher the harness starts exactly as before; the code path is not
  entered.
- **Stop and cleanup are commands, not calls.** The core runs them itself, with a time limit, so a launcher that is a
  process need not be alive or responsive to end a run, and a sandbox's tree is ended even when the plugin has crashed.
- **What the launcher cannot cover is refused, not skipped.** A run on an SSH runner is passed with its runner, so the
  launcher can answer `unchanged` (an answer that wraps it fails the run). Pi-Bolt's terminal interface for a chat is a
  process the person's terminal owns, which no stop or cleanup could reach, so it is unavailable while a launcher is
  configured. Claude Code is not asked yet, and so does not start while a launcher is configured, rather than start
  outside it.

## First user: a sandbox with nono

The first launcher is a plugin in the separate repository `Code000-211/moebius-plugins` that runs each Pi-Bolt process
in a [nono](https://github.com/nolabs-ai/nono) sandbox. nono is one unprivileged binary (Landlock on Linux, Seatbelt on
macOS) with no daemon, starts instantly, uses the server's own toolchains and filters the network per host and port.

- For a launch the plugin writes an owner-only profile: the worktree and the run's temporary directory read-write; the
  agent directory, extensions and toolchain paths read-only; not the data directory, `secrets.toml`, the token file or
  `moebius.toml`. It returns `nono run --profile <file> -- <program> <args>` with the environment in the profile, so the
  run token is never in argv, and removes the profile in its cleanup command and in its leftovers.
- **Network.** By default only the router and the gateway, both on the server's one loopback listener, so the rule is a
  single host and port in the profile and not network plumbing. A mode `internet` adds general outbound access, and a
  mode can be set per agent. This is why the request carries the three addresses, and why the launcher may name others
  as seen from inside, should a sandbox reach the server at another address.
- Agents that must operate the server (the Moebius agent) can be excluded, and a run on a runner is answered
  `unchanged`.

## Considered Options

- **A sandbox in the core.** A choice of nono over containers, or the reverse, for everyone; it grows the core with a
  dependency most installations do not want (ADR 0009).
- **A wrapper program in `moebius.toml`** (`pi_bolt.program = "nono run ... pi-bolt"`). It cannot know the run: no
  per-run profile, no cleanup, no per-agent exceptions, and the run token and paths are only known at launch.
- **A runner kind (`[[runners]]`).** Runners move the harness to another machine and copy files there; a sandbox on the
  same machine is a different thing and would need to be taught the same worktree layout.
- **Docker as the first launcher.** It isolates toolchains as well, but needs an image per toolchain and a daemon. It may
  follow as a second plugin of the same kind.

## Consequences

- A sandbox is one more `[[plugins]]` entry, with its own settings under that entry; the core stays generic.
- A launcher sees the run's token and the environment of every run it wraps, so it is trusted as much as the server.
  Its answers are not stored, and its standard error goes to its log with secrets redacted.
- Claude Code can use the same hook later; nothing in the messages is specific to Pi-Bolt.
