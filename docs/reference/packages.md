# Packages

The package manager finds, installs, updates and removes what extends Moebius itself, with versions pinned and checked.
A package is one of:

| Kind | What it is | Where it goes |
|---|---|---|
| `plugin` | A program speaking the [process plugin protocol](plugin-api.md#plugins-in-their-own-process): a decider, or a [launcher](plugin-api.md#launchers). | `<data_dir>/packages/<name>/<version>/`, with a `[[plugins]]` entry written for it. |
| `workflow` | Kanban column scripts (`.luau`). | Proposed as a change to the [harness configuration repository](harness-configuration.md), under `repos/<repository name>/`. |
| `rules` | A fragment of a `rules.md`. | Its rules are proposed to be added to a project's `repos/<repository name>/rules.md` of the harness configuration repository. |
| `skills` | Skills. | Proposed as a change to the [harness configuration repository](harness-configuration.md), under `skills/`. |

Packages come from **registries**: git repositories with an `index.toml` at their root. Moebius's own, `moebius-plugins`
(`https://github.com/Code000-211/moebius-plugins`), is always there; more are added with
[`[[registries]]`](configuration.md#registries). Why it is built this way is in
[ADR 0015](../adr/0015-packages-come-from-registries.md); how to publish a plugin is in
[the plugins guide](../guide/plugins.md#publish-a-plugin-and-install-it).

The code is in `crates/moebius-core/src/server/packages/`.

## The package manifest: `moebius-package.toml`

A package's directory in a registry repository (`plugins/<name>/`, `workflows/<name>/`, `rules/<name>/`,
`skills/<name>/`) holds a `moebius-package.toml`. The tool that writes `index.toml` reads it; the index entry is these
fields plus the downloads. Keys not listed are ignored.

| Key | Type | Meaning |
|---|---|---|
| `name` | string | Letters, digits, `-` and `_`. The name `[[plugins]]` and `moebius pkg` use. |
| `version` | string | One to three numbers separated by dots, such as `0.1.0`. No pre-release suffix. |
| `kind` | string | `plugin`, `workflow`, `rules` or `skills`. |
| `description` | string | One sentence. |
| `license`, `homepage` | string | Optional. |
| `moebius` | string | The Moebius versions it works with, a [range](#version-ranges), such as `">=0.2"`. |
| `targets` | array of strings | Plugins: the target triples it is built for, such as `x86_64-unknown-linux-gnu`. |
| `[plugin]` | table | Plugins only, below. |
| `[[files]]` | array of tables | Kanban (`workflow`), rules and skills packages: `source` (the file in the package's directory) and `to` (where it goes), below. |

`[plugin]`:

| Key | Type | Meaning |
|---|---|---|
| `kind` | string | `decider` or `launcher`. |
| `program` | string | The program's file name: no `/`, `\` or `:`, not starting with `.` or `-`. It is installed to `<data_dir>/packages/<name>/<version>/<program>`. |
| `launches` | boolean | Whether the `[[plugins]]` entry gets `launches = true`. Always so for a launcher. |
| `decides` | array of strings | The kinds of decision a decider answers (the entry's `decides`). A decider needs at least one. |
| `settings_as` | string | `args`: the settings are the program's arguments (`flag`). Informative. |
| `[plugin.requires]` | table | Programs it needs besides Moebius, with versions. Informative. |
| `[[plugin.settings]]` | array of tables | The settings it takes, for a form: `name`, `flag`, `type`, `choices`, `repeatable`, `default`, `description`. Informative. |

`[[files]]`: `to` is a plain relative path (no `..`, no empty parts, nothing in `.git`). For a `workflow` package (a Kanban) it ends in
`.luau` and lands at `repos/<repository name>/<to>` of the harness configuration repository. For `skills` it lands at `skills/<to>` of the harness
configuration repository, so `to = "tdd/SKILL.md"` is the skill `tdd`. A `rules` package has exactly one file, a
`rules.md` fragment; its `to` is not used.

The manifest of the nono sandbox plugin in `Code000-211/moebius-plugins` is read by this format as it is
(`crates/moebius-core/testdata/packages/nono-sandbox.toml` is a copy that a test parses).

## The index: `index.toml`

```toml
format = 1

[[package]]
name = "nono-sandbox"
version = "0.1.0"
kind = "plugin"
description = "Runs every Pi-Bolt process in its own nono sandbox."
license = "Apache-2.0"
homepage = "https://github.com/Code000-211/moebius-plugins/tree/main/plugins/nono-sandbox"
moebius = ">=0.2"
targets = ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"]

[package.plugin]
kind = "launcher"
program = "nono-sandbox"
launches = true
settings_as = "args"

[[package.downloads]]
target = "x86_64-unknown-linux-gnu"
url = "https://github.com/Code000-211/moebius-plugins/releases/download/nono-sandbox-v0.1.0/nono-sandbox-x86_64-unknown-linux-gnu"
sha256 = "9f2c...64 hexadecimal digits"

[[package]]
name = "careful"
version = "1.0.0"
kind = "rules"
description = "Rules for careful repositories."
moebius = "*"

[[package.files]]
to = "rules.md"
url = "https://raw.githubusercontent.com/acme/packages/main/rules/careful/rules.md"
sha256 = "..."
```

`format` is `1`; an index with another format is refused. `[[package]]` is a package manifest (every field above, the
`[plugin]` table with its settings included) with two additions:

- `[[package.downloads]]` (plugins; at least one): `target` (a target triple), `url` and `sha256`. The URL is the
  program itself, one file, which is saved under the manifest's `program` name and made executable. Moebius picks the
  download whose `target` is its own: `<arch>-unknown-linux-gnu`, `<arch>-apple-darwin` or `<arch>-pc-windows-msvc`,
  with `<arch>` as Rust names it (`x86_64`, `aarch64`). A plugin without one for the target is refused.
- `url` and `sha256` in each `[[package.files]]` (the others): the file's text, UTF-8.

Every URL is `http` or `https`; every `sha256` is 64 hexadecimal digits. The same name and version twice, or any entry
that is not valid, refuses the whole index (one entry that cannot be trusted is a reason not to trust the others). The
registry's generator writes one `[[package]]` for every version it has released; Moebius picks among them.

The index can be signed (a minisign signature `index.toml.minisig` and a key pinned per registry in `[[registries]]`).
That is designed in (a signature is checked before the index is read, and a key that is set makes an unsigned index an
error) and is not switched on yet; `[[registries]]` has no key for it until it is.

### Version ranges

A version is one to three numbers separated by dots, the missing ones being 0 (`0.2` is `0.2.0`). A range is
comparators separated by commas or spaces: `>=`, `>`, `<=`, `<` and `=` (or none) followed by a version, or `*` for
every version. A version is in the range when it satisfies every comparator: `">=0.2, <1"`.

## Installing

`moebius pkg install <name>[@<version>]` (`POST /v1/packages`) picks the package in the first registry that has it (or
the one named), at the version asked for or the newest whose `moebius` range includes the running Moebius. Refused,
before anything changes:

- the package is installed already (`409`);
- a registry that is named is not configured, no registry has the package (`404`), or the version does not exist;
- the `moebius` range of the version asked for does not include the running version, or none of the versions' does:
  `<name> <version> works with Moebius <range>, and this is Moebius <running>` (`422`);
- a plugin has no download for this target;
- a download does not have the sha256 the index names: `the download of <url> has the sha256 <actual>, not
  <expected>; nothing was installed` (`422`). The check is made on the bytes before a file is written;
- a download fails (`502`).

Nothing of a package runs while it installs: a program is downloaded, checked, written and only then, with its entry
in place, started.

**A plugin** is written to `<data_dir>/packages/<name>/<version>/<program>`. The `[[plugins]]` entry, `name`, `path`
(the program), `args = []`, and `decides` and `launches` from the manifest, is added to `moebius.toml` through the
live settings, so comments stay and the file must still parse. The plugin host starts the plugin at once, and a
decider answers its kinds from then on; the answer carries the `plugin` status (`state`, `error`). A plugin that does
not start is installed all the same and fails closed as any plugin does (its calls are refused and it is started again
after a pause). A **launcher** is read at the server's start, so the answer says `restart_needed`. Settings of a plugin
that are flags go in the entry's `args` (the Settings page, or `moebius settings set`), and are kept by updates.

**A Kanban (`workflow`), rules or skills package** is proposed: the files are committed on the branch `moebius/package-<name>`
from the default branch of the harness configuration repository, and offered as a change with a `ready_to_merge` item
in the inbox. A Kanban (`workflow`) or rules package names the project it is for (`--repository`; the only repository when there
is one): its files go to `repos/<repository name>/` there. Nothing is installed until a person merges it. The change has
no repository (`repository_id` null). A rules package adds its rules, the `## name` sections of the fragment (text
before the first is not added), at the end of the project's `rules.md`; a rule named like one that is there is refused,
and the result must pass the rules check. Every kind needs a harness configuration repository, else it is refused.

The Moebius agent may propose a Kanban (`workflow`), rules or skills package. Installing a plugin is for a person only.

A plugin package whose name is the name of a plugin that is part of Moebius (such as `guard`) is refused, so that a
registry cannot take a built-in decider out of service. Install, update and remove, the nightly check and the reading of
a registry's clone each run one at a time; a second request waits for the first.

## Updating and removing

`moebius pkg update [<name>]` (`POST /v1/packages/{name}/update`) installs the newest version of the package's registry
that works with this Moebius, if it is newer than the installed one; otherwise nothing changes (`action` is
`unchanged`).

- A plugin's new version is downloaded and checked, started on its own, and only if it runs does it replace the old
  one: the `path`, `decides` and `launches` of its entry are set from the new version (so that a restart routes the
  same kinds as the running server), the host swaps the process (the old one is stopped) and the old version's
  folder is deleted. One that does not start leaves the old version running and is refused. Its arguments are kept.
  A launcher is swapped the same way and needs no restart.
- Files are proposed anew: the new version's files, and the deletion of those the old version had and the new one has
  not (rules: the old version's rules are replaced by the new version's).

`moebius pkg remove <name>` (`DELETE /v1/packages/{name}`): a plugin's `[[plugins]]` entry is removed through the
live settings, the plugin stopped, its decisions unrouted, the lock entry removed, and then
`<data_dir>/packages/<name>/` deleted; if the folder cannot be deleted (on Windows a program that has not ended yet
can keep it) the removal is done all the same and the answer names the folder as `leftover`. A launcher needs a
restart (`restart_needed`). For the others, the deletion of the files the lock records is proposed as a change (what
has not been merged yet cannot be deleted: it is refused with `the files are in <repository> already` when the
default branch lacks them). Either way the lock entry goes.

Updating and removing are for a person only.

## The lock: `<data_dir>/packages.lock`

```toml
[[package]]
name = "gate"
version = "1.1.0"
kind = "plugin"
registry = "moebius-plugins"
url = "https://example.com/gate-1.1.0"
sha256 = "ab12..."
program = "/srv/.moebius/packages/gate/1.1.0/gate"
```

| Key | Meaning |
|---|---|
| `name`, `version`, `kind`, `registry` | What is installed and where from. |
| `url`, `sha256` | The program (plugins), or the first file (others), as downloaded. |
| `program` | A plugin: where its program is. |
| `repository` | A Kanban (`workflow`) or rules package: the repository it was proposed to, by name. Not for skills. |
| `change` | The change that proposed the files. The lock records what was proposed, not what was merged. |
| `[[package.files]]` | The proposed files: `to`, `url`, `sha256`. |
| `rules` | A rules package: the names of the rules it added. |

It is written in one step after every install, update and removal.

## The nightly check

Once a day, from 03:00 on the server's local clock, and only when something is installed, the schedule reads each
registry that an installed package came from and looks for versions that are newer than the installed one and work
with this Moebius. For each package it adds one inbox item of the kind `package_update` (no card;
`data` is `{"name", "installed", "version"}`) and publishes `package_update_available`, once for each version: an open
item for the same package and version is not added again, and an open item for an older version is resolved when a
newer one is offered, so a package has at most one. Updating or removing the package resolves its items. A
registry that cannot be read is reported as `package_check_error`. The day of the last check is kept in
`<data_dir>/packages.checked`, so a check that fails is not tried again until tomorrow. `GET /v1/packages` shows the
newest waiting version as `update_available`.

## Command line

See [the command line](cli.md): `moebius pkg search|info|install|update|remove|list`.

## API and events

The routes are in [the desktop API](desktop-api.md#packages): `GET /v1/packages/search`, `GET /v1/packages`, `POST
/v1/packages`, `GET /v1/packages/{name}`, `POST /v1/packages/{name}/update` and `DELETE /v1/packages/{name}`. The
events are `package_installed`, `package_removed`, `package_update_available` and `package_check_error`.
