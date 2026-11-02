# Packages come from registries, and plugins live in a second repository

Until now a plugin was a program a person built or downloaded and named by path under `[[plugins]]`, and a workflow or
a set of rules was copied by hand. There was no way to see what exists, which version is installed, or to update it. ADR
0009 keeps choices such as which sandbox is right out of the core, so the core needs the means to install them, not the
things themselves.

## The decision

- **A package manager in the core** (`moebius pkg`, `/v1/packages`) finds, installs, updates and removes four kinds of
  package: a `plugin` (a program speaking the process plugin protocol), and `workflow`, `rules` and `skills` packages
  (files). The reference is `docs/reference/packages.md`.
- **The plugins are not built in this repository.** A second repository, `Code000-211/moebius-plugins`, holds them, each
  in its own crate, in any language, and is also the package index Moebius installs from: its CI tests each package,
  builds plugins per target, publishes a GitHub release `<plugin>-v<version>` and regenerates `index.toml`. Plugins in
  Rust depend on `moebius-plugin-api` by git tag of this repository, so this repository stays the source of the API and
  the plugins' releases are independent of Moebius's.
- **A registry is a git repository with an `index.toml`.** It is read with `git clone` and `git fetch`, which brings
  authentication (private repositories, a company's own) and history for free and needs no server of ours. The default
  registry is `moebius-plugins`; `[[registries]]` adds others with the same layout.
- **The index names every download with its sha256.** A download whose hash differs is refused before anything is
  written, and a package whose `moebius` range does not include the running version is refused with a sentence. The
  index is read as a whole or not at all: an entry that cannot be trusted is a reason not to trust the rest. Signing
  (minisign, with a key pinned per registry) is designed in, since the index is one file whose signature can be checked
  before it is read, and is a follow-up.
- **Nothing of a package runs while it installs.** A plugin's program is downloaded, checked and written; its
  `[[plugins]]` entry goes in through the live settings (the same path as an edit in the app, so it is validated and
  recorded); only then does the plugin host start it. A plugin that fails to start fails closed, as any plugin does.
- **Plugins take effect at once, files are proposed.** The host now takes a plugin in and out while the server runs
  (`Host::set_process`, `Host::remove`), so a decider needs no restart and an update swaps the program (the new one must
  run before it replaces the old). A launcher is read at the server's start, so it asks for a restart. Workflows and
  rules change a repository, and skills the harness configuration repository; they are proposed as a change that a
  person merges, as the Retro's proposals and a chat's work are, never written to a branch someone is working on.
- **A plugin can only be installed by a person.** A plugin is code that runs as the server's user. The Moebius agent
  may search and propose files, which a person merges, and may not install, update or remove a plugin.
- **Updates are noticed, not made.** The schedule looks once a night for newer compatible versions of what is installed
  and puts one inbox item for each; a person updates.

## What this costs

- A plugin's program is one file per target. An archive, or a program with data files, needs a follow-up in the format.
- The lock records what was proposed for the files of a workflow, rules or skills package, not what a person later
  merged or changed; removing what was not merged is refused rather than guessed.
- Registries are read when the server starts, so a registry added in `moebius.toml` is used after a restart.
