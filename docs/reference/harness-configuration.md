# Harness configuration

The harness configuration is a git repository that holds the agents, their skills, the server's rules, the MCP servers
and the settings of each project (a repository the server works on).
This page is for whoever sets it up or edits it, by hand or through the desktop API: it lists every file and key. The code
is in `crates/moebius-core/src/harness/repo/`.

## Using it

Name the repository in `moebius.toml`:

```toml
[harness]
url = "https://github.com/acme/harness-configuration"
mcp_env = ["DOCS_TOKEN"]
```

- `url` is anything `git clone` takes. With `[github]` set, an `https://github.com/...` URL is cloned, fetched and pushed
  with the GitHub App's installation token or the access token, like the repositories Moebius works on; the App or the
  token needs write access to the repository for edits. Any other URL, such as a local path, uses plain git.
- `mcp_env` (array of names, default none) lists the environment variables of the server that `mcp.toml` may take values
  from (see [`mcp.toml`](#mcptoml)). Each is a name of letters, digits and `_` that does not start with a digit, else
  the file is refused. Every value taken through `${NAME}` is treated as a secret: of 12 characters or more, it is
  replaced by `[redacted:secret]` in all stored and streamed text while the server runs, so name only secrets here.
- The server clones it to `<data_dir>/harness` when it starts. A clone that cannot be made stops the server; a clone that is
  there already is used as it is, so the server starts without the network.
- Every 30 seconds the server fetches `origin` and fast-forwards the clone's default branch (the branch that is checked
  out). When that changed anything, the agents are read again. Runs that are going keep the agent they started with.
- With `[harness]`, `[agents.*]` and `rules_file` in `moebius.toml` are errors that say to move them into the repository, so
  that there is one source of truth. Without `[harness]` everything is configured in `moebius.toml`, and the routes
  `GET`, `PUT` and `DELETE /v1/agents/{name}` answer `422`.
- Everything is read from the default branch. Other branches of the repository are not read.

## A repository on the server

`moebius init` and the web app's "Set up" (`POST /v1/harness/setup`, see the [desktop API](desktop-api.md) and
[configuration](configuration.md#setting-it-up-from-the-app)) make the repository `harness/` next to `moebius.toml` and
set `[harness] url` to its absolute path. It is a git repository on `main` with one commit, "Set up the harness
configuration repository", holding:

- `agents/<name>.toml` and `agents/<name>.md` for each `[agents.<name>]` the file had (none after `init`): the model,
  protocol, thinking level and token budget as they were, the instructions as the `.md`, and no MCP servers;
- `skills/<name>/`: a copy of each skill directory an agent named by path (a path to its `SKILL.md` names the directory),
  without a `.git` in it, named by the directory and listed by that name in the agent's `skills`. The name must be a valid
  skill name and not that of a built-in skill, and two different directories with the same name are refused: rename one
  first;
- `extensions/<file name>`: a copy of each extension file an agent named, listed by that path in its `extensions`; two
  different files with the same name are refused;
- `rules.md`: a copy of the file of `rules_file` (by default `rules.md` next to `moebius.toml`), empty when there was none;
- `mcp.toml` with no servers, and a `README.md` that says what the repository is.

The agents are read back as the server reads them before the commit; one that would not work stops the setup. A setup
that fails leaves the directory as it was. The repository is set with `receive.denyCurrentBranch=updateInstead`, so that
the server's clone can push the app's edits to the branch that is checked out and its files follow; a push is refused
while the working tree has changes that are not committed. Edit it by hand like any repository: commit on `main`, and
the server fetches the commit within 30 seconds.

## Layout

```
agents/<name>.toml     the settings of an agent
agents/<name>.md       the instructions of an agent (optional)
skills/<name>/SKILL.md a skill (a directory with this file)
rules.md               the server's rules
mcp.toml               the MCP servers
extensions/...         any files an agent's extensions name
packages/<name>/       a vendored Pi package with its production dependencies (see Packages)
packages.lock          what each vendored package was fetched as
repos/<repository>/... the settings of one project (see Project settings)
```

Nothing else is read. The Moebius agent stays built in: `agents/Moebius.toml` configures it as `[agents.Moebius]` does, and
`agents/Moebius.md` is added after its built-in instructions (see [configuration](configuration.md)). So do the
`Grilling-partner` (see [Grilling](#grilling)), the `Coder` and the `Reviewer` (see [The Coder and the
Reviewer](#the-coder-and-the-reviewer)).

### The Coder and the Reviewer

The agents the default Kanban starts exist without any file: Moebius brings the `Coder` and the `Reviewer` with their
instructions and their [engineering skills](#engineering-skills), and `agents/Coder.toml`, `agents/Coder.md` (or
`[agents.Coder]`) and the same for the Reviewer add what a person sets on top, as for the Moebius agent: the built-in
instructions come first and the agent's own after them, the built-in skills are the engineering skills alone (no
`moebius` skill), and the settings are the person's own. They are listed by `GET /v1/agents` as built in whether or not
they have files, and a new agent can be started as a copy of either (see [`agent save --template`](cli.md)).

The model is the one thing Moebius cannot choose. A configured agent has its own. The Reviewer without one takes the
Coder's model, protocol, harness and `runner`, so it reviews where the Coder coded (the other built-in agents take the
model, protocol and harness, and run on the server). The Coder without one has no other to take it from, so it is listed with an
empty model, the Agents page says it needs one, and a card that starts it fails closed with a `board_error` that says
`the Coder agent needs a model`; a chat with it runs on the model the chat is given. Nothing is guessed from the models
the router can serve.

The `Reviewer` is the review agent of the default Kanban: it exists always, like the Coder (`moebius.has_agent("Reviewer")`
is true on every server), so a card goes to review out of the box, on the Coder's model unless the Reviewer has its own.
A person who wants no review changes the column's script (`review.luau` of the project's settings, see the
[Kanban guide](../guide/kanban.md)), for example to `moebius.offer_change()` at once.
The instructions complement what each run's prompt says (the issue, the checks, the `FINDING` format) and do not repeat
it. The parts that fit only a card (the issue as the spec, the summary, the `FINDING` format) start with "When you work
on a card", so in a chat with either agent they do not hold it to a card's rules; it follows what the person asks, with
the same care (tests, checks, the worktree). The Reviewer's two review runs each limit it to one axis in their prompt,
and the instructions tell it that the prompt's scope and format win. The skills are named "when you have it", since
`engineering_skills` can remove them. The Coder's:

> You are the Coder. You turn one issue into a change that is ready to be reviewed and merged.
>
> When you work on a card, the issue is the spec. Do what it asks and nothing else: no cleanup of code you pass by, no feature it does not name, no refactoring it does not need. If something near it looks wrong, say so in your summary and leave it. When the issue is unclear, take the reading that is the smallest and the easiest to undo, and say in the summary which reading you took. In a chat, do what the person asks, with the same care as below: tests, checks, and staying in your worktree.
>
> Before you change anything, read the repository's CLAUDE.md, AGENTS.md and CONTRIBUTING.md, whichever of them exist, and follow them: its layout, its style, its tests, its commit messages. What they say wins over your habits.
>
> Work test first, with the tdd skill when you have it: write the test that shows what is asked, see it fail for the right reason, make it pass, and then tidy. For a bug, use the diagnosing-bugs skill when you have it to find the cause before you fix it, and let the test you write be the one that fails without the fix. Follow the skill's rules about where tests go and what a good test is. Nobody answers a question in a run, so where a skill tells you to ask, read the moebius-autonomous skill when you have it: decide, and write the decision and its reason in your summary.
>
> Before you finish, run the checks the repository has, those its documentation names (format, lint, type check, the tests), and fix what they report. Do not weaken a test, a check or a lint rule to make it pass. If a check fails for a reason that has nothing to do with your change, say so in the summary instead of fixing it.
>
> Commit in small steps, each a change that stands on its own, with a message whose subject says what the change does in the imperative and whose body says why, as the repository's own history does. Do not commit generated files, secrets or build output.
>
> Stay inside your worktree. Do not read or write files outside it, except to read the full output of a command that a result says was saved to a file; do not touch other branches, other worktrees or the repository's own clone, and do not push unless the person you are chatting with asks you to. Do not install anything on the machine that the repository does not already ask for.
>
> When you work on a card, end with the summary the run's prompt asks for: what you changed and why, how you tested it, what you decided without being able to ask, and anything you saw but left alone. In a chat, say the same in your last message.

The Reviewer's:

> You are the Reviewer. You judge a change that someone else made for an issue, and you decide whether it can be merged. You read; you do not change the change.
>
> When you work on a card, review the change against two things: the issue, and the repository's standards. If the run's prompt limits the review to one of them, report only on that one: the scope and the format of the prompt win over this text. Against the issue, ask whether it does what was asked and nothing more, whether it is correct, and whether every behaviour it adds has a test that would fail without it. Against the standards, read the repository's CLAUDE.md, AGENTS.md and CONTRIBUTING.md, whichever of them exist, and check the change follows them. Read the whole change and the code around it before you judge a line; a hunk alone can mislead you.
>
> Use the code-review skill for how to look at a change. Where a run's prompt tells you how to report, that wins over the report headings of the skill. Nobody answers a question in a run, so read the moebius-autonomous skill for what to do where the skill tells you to ask: decide, and say what you decided.
>
> You change no source file, make no commit and touch no branch. You may run the repository's tests and checks to see whether the change holds up; the build output they leave is fine. In a chat, answer what the person asks about a change, with the same care and the same limits.
>
> When you work on a card, report each problem in exactly the format the run's prompt asks for: a line `FINDING blocking: <what is wrong>` or `FINDING judgement: <what is doubtful>`, then a line `EVIDENCE: <the lines of the diff, or the line of the issue, that show it>`. Blocking is only for what is wrong: a bug, a missing or failing test, something the issue asks for that is not done, a rule of the repository that the change breaks. Judgement is for what is doubtful and a person should look at: a choice that may be right, a risk, a smell. Style that the repository's tooling enforces is neither. Never invent a finding to look thorough, never repeat a finding, and never report one you cannot back with evidence; a change with no problems gets no findings and an approval.
>
> When you work on a card, end the way the run's prompt says, with its last line and nothing after it.

## `agents/<name>.toml`

`<name>` is made of letters, digits, `-` and `_` and is not empty. Unknown keys are errors.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `model` | string | required | The model's name, sent to the router and given to Pi-Bolt as `--model`. Not blank. |
| `protocol` | string | `openai` | `openai` or `anthropic`: which router endpoint the harness uses. |
| `runner` | string | none | The name of a `[[runners]]` entry of `moebius.toml` (see [configuration](configuration.md#runners)) that the agent's runs go to over SSH; without it they run on the server. The name is checked when a run starts: a run of an agent whose runner is gone is not made, and says so. |
| `harness` | string | `pi-bolt` | `pi-bolt`, or `claude-code` to run the agent on Claude Code (see [configuration](configuration.md#claude-code)), which needs `protocol = "anthropic"` and no `extensions`, else the agent is an error. Written only when it is not `pi-bolt`. |
| `thinking` | string | none | Given to Pi-Bolt as `--thinking`. Pi-Bolt checks the value. |
| `token_budget` | integer | none | The most tokens a card may use with this agent, as `agents.<name>.token_budget` in [configuration](configuration.md). At least 1. |
| `skills` | array of strings | `[]` | Names of directories under `skills/` that hold a `SKILL.md`, or `pkg:<package>[/<path>]` for the skills of a vendored package (see [Packages](#packages)). Each is given to Pi-Bolt as `--skill`. A name that is not there is an error. |
| `extensions` | array of strings | `[]` | Paths of files in the repository, relative to its root and without `..`, or `pkg:<package>[/<path>]` for the extensions of a vendored package (see [Packages](#packages)). Each is given to Pi-Bolt as `--extension`, before the bridge. A path that is not a file, and a `pkg:` name that does not resolve, is an error. |
| `tool_output` | table | the defaults | `max_lines`, `max_bytes` and `compress`, as `agents.<name>.tool_output` in [configuration](configuration.md); a key left out has its default. Pi-Bolt runs only. The former key `hide_after_turns` is still read and ignored. |
| `engineering_skills` | boolean or array of strings | `true` | The engineering skills the agent is given (see [Engineering skills](#engineering-skills)): `true` the defaults of its name (the `Coder`'s, the `Reviewer`'s and the `Grilling-partner`'s, none for another name), `false` none, a list of names exactly those. Only the names of the engineering skills and `moebius-autonomous` are accepted (`tdd`, `diagnosing-bugs`, `code-review`, `grilling`, `to-tickets`, `wayfinder`, `domain-modeling`, `research`, `prototype`, `moebius-autonomous`); any other name, or any other value, is an error of the agent. Written only when it is not `true`. |
| `mcp` | array of strings | `[]` | Names of servers in `mcp.toml`. A run of the agent is given exactly these servers. A name that is not there, a server that cannot be used (see [`mcp.toml`](#mcptoml)), or any name while `mcp.toml` cannot be read, is an error. |

The instructions are not a key: they are the text of `agents/<name>.md`, given to Pi-Bolt as `--append-system-prompt`.
A file that is empty or only blank is no instructions.

```toml
model = "my-local-model"
protocol = "openai"
thinking = "medium"
token_budget = 2000000
skills = ["house-style"]
extensions = ["extensions/log-tool-calls.ts"]
mcp = ["docs"]
```

## `skills/<name>/SKILL.md`

A skill as Pi-Bolt documents it: a directory with a `SKILL.md` and whatever else the skill uses. A directory without a
`SKILL.md` is not a skill. An agent that names a skill that is missing is left out with an error. A run is given exactly
its agent's skills, in the order of the `skills` key, and no skill Pi-Bolt would find on its own. The Moebius agent has
its built-in skills first, the `moebius` skill and the task skills `moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions`, `moebius-plugins` and `moebius-report`, then those of
`agents/Moebius.toml`. The other built-in agents have the `moebius` skill first, except the Coder and the Reviewer, which have none of the reference library. A directory of `skills/` with the name
of a built-in skill is ignored, except for the engineering skills below.

### Engineering skills

Moebius brings nine skills from [Matt Pocock's skills](https://github.com/mattpocock/skills) (MIT license), copied
unchanged at a pinned commit (`crates/moebius-core/skill/vendor/mattpocock/` in the source, with the license), and the
small skill `moebius-autonomous`. The skills are written for work with a person: the adapter says what to do where one
asks the user (decide, and give the decision and its reason in the run's summary), that the card's issue is the spec and
the tracker (the Grilling partner's `wayfinder` keeps its map on Moebius's own issues instead, see below), and that the
setup skill is never run. They are written to `<data_dir>/skills/<name>/` with the other
built-in skills when the server starts, and given to a run before the agent's own `skills`:

| Agent | Skills |
|---|---|
| `Coder` | `tdd`, `diagnosing-bugs`, `moebius-autonomous` |
| `Reviewer` | `code-review`, `moebius-autonomous` |
| `Grilling-partner` | `grilling`, `to-tickets`, `wayfinder`, `domain-modeling`, `research`, `prototype`, `moebius-autonomous` (in front of the `moebius` skill) |

That is what `engineering_skills = true` means, the default: the skills go with the agent's name. Any agent can be given
others, by name, with a list: `engineering_skills = ["tdd", "diagnosing-bugs", "moebius-autonomous"]` gives the agent
exactly those skills, in that order, and nothing else (`moebius-autonomous` only when the list has it, so name it
beside any skill that asks a question of the user). An agent with another name has none by default. This holds on both harnesses (Claude Code gets them as skills of its run's
configuration directory). `to-tickets` and `wayfinder` ask not to be offered to the model on its own (`disable-model-invocation: true`), so the
Grilling partner is told in its instructions where their `SKILL.md` is, and that a message that starts with `/` and the
name of one (`/wayfinder`) asks it to follow that skill.

`wayfinder` plans work too big for one session as a map on the issue tracker, and `domain-modeling`, `research` and
`prototype` are the skills its tickets call. Moebius has no tracker configuration for it: the map is an issue labelled
`wayfinder:map` with its tickets as sub-issues labelled `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling`
or `wayfinder:task`, blocking edges between them, a claim for a session at work, and a comment, closing and a line of
the map's body as the answer. The Grilling partner keeps it with `"$MOEBIUS" issue` (its run has `MOEBIUS_URL`,
`MOEBIUS_TOKEN` and `MOEBIUS_REPOSITORY`, and the desktop API accepts its token for the routes of the
[issue tracker](desktop-api.md#the-issue-tracker) but deleting an issue), as `moebius-autonomous` says. The default
`ready.luau` never starts the Coder on an issue with a `wayfinder:` label (see [column scripts](column-scripts.md)); the
Grilling partner's session closes it.

- **Turn them off** for an agent with `engineering_skills = false` in its `agents/<name>.toml` (or its
  `[agents.<name>]` table); the app has the setting on the agent's page, and `agent save` has `--no-engineering-skills`.
  It turns off the nine skills and the adapter together; the `moebius` skill stays on the Grilling partner.
- **Choose them** with a list of names, as above; the app has a toggle for each skill on the agent's page, and `agent
  save` has `--engineering-skill <name>`, given again for more. A copy of an agent (see [`agent save
  --template`](cli.md)) takes over everything, so it is written with the engineering skills the source has, as an
  explicit list: `Coder-fast`, copied from the Coder, has `tdd`, `diagnosing-bugs` and `moebius-autonomous` whatever its
  name. The `Reviewer` without a configuration of its own, which only borrows the Coder's model, harness and `runner`, keeps the skill defaults of its own name (`code-review`, `moebius-autonomous`) whatever the Coder's `engineering_skills` says, a list or `false`.
- **Replace one** with a skill of your own: a directory `skills/<name>/` of this repository with a `SKILL.md` and the
  name of the skill (`tdd`, `diagnosing-bugs`, `code-review`, `grilling`, `to-tickets`, `wayfinder`, `domain-modeling`, `research` or `prototype`) takes its place for every agent that has it (by default or by its list), with all its files. These
  names, unlike `moebius` and the `moebius-` skills, are not reserved. `moebius-autonomous` cannot be replaced.
- **Update them from upstream**: the Skills page shows each one as built in with the source and the commit it was copied
  at, and "Update from upstream" asks the Moebius agent to import that skill from the repository with the Skills import,
  which saves it as a skill of this repository with the same name (so it replaces the built-in copy, as above). The
  copies that ship with Moebius move only with a new release.

`GET /v1/skills` and `GET /v1/skills/{name}` give these skills `"source": {"url", "commit"}`.

## Packages

A Pi package is one unit of extensions, skills, prompt templates and themes, in Pi's own format. The catalog of pi.dev is
the npm packages with the keyword `pi-package`. Moebius vendors a package into the repository, so that a run fetches
nothing when it starts and every change is reviewed like any other configuration change. Pi's own `pi install` and
`-e npm:` are never used for runs: Pi-Bolt is started with `--no-extensions`, and an agent gets only the files it names.

```
packages/<name>/       the package at its version, with its production dependencies in node_modules/
packages.lock          one [[package]] table for each, sorted by name
```

`<name>` is the package's name; a scoped name `@scope/name` is the directory `packages/@scope/name/`. Only lowercase
letters, digits, `.`, `-` and `_` are allowed, and a part does not start with `.` or `_`. `packages.lock` is TOML,
written by Moebius:

| Key | Meaning |
|---|---|
| `name` | The directory below `packages/`. For a git source, the `name` of its `package.json` (the repository's name without one). |
| `source` | As `pi install` takes it, with the version or ref that was asked for: `npm:<name>[@<version>]` or `git:<host>/<owner>/<repo>[@<ref>]`. A git source may also be a full URL (`git:https://...`, `git:file:///path`, `git:git@host:owner/repo`). |
| `version` | What was fetched: the npm version; for git, the `version` of `package.json`, else the short commit. |
| `resolved` | The tarball URL of an npm package, the full commit of a git one. |
| `integrity` | npm only: the registry's `dist.integrity` (`sha512-...`), which the download was checked against. A version whose packument has no sha512 integrity is refused. |
| `sha256` | npm only: the SHA-256 of the tarball in hex. |

An npm version is nothing (the `latest` tag), a tag, an exact version, or a range written `^1.2.3` or `~1.2.3`
(releases only, not prereleases); anything else is refused. A git ref is a branch, a tag or a commit; a commit is
pinned, a branch or tag is followed by an update to its newest commit.

**What is fetched.** An npm package is its tarball, with a download whose SHA-512 is not the registry's `dist.integrity`
refused. A git package is the files of the commit (`git archive`, no `.git`). The dependencies are then installed with
`npm install --ignore-scripts --omit=dev --legacy-peer-deps --no-package-lock --no-bin-links`, so no install script of
the package or of a dependency runs on the server, and the packages Pi supplies itself are not installed. A package
whose `dependencies` or `optionalDependencies` are not registry versions, ranges or tags (a git repository, a path, a
link, a tarball URL, an alias) is refused with `422`, since npm builds those and runs their `prepare` script despite
`--ignore-scripts`; a git dependency further down fails, and the `.npmrc` and `.yarnrc` of the package are deleted
before the install. The scripts a
package would have run (`preinstall`, `install`, `postinstall`, `prepare`) are listed in its commit message and in
`install_scripts` of `GET /v1/extensions/packages`.

**Which resources load.** A package's resources are read from the `pi` field of its `package.json`: `pi.extensions`,
`pi.skills`, `pi.prompts` and `pi.themes` are arrays of paths relative to the package, which may use `*`, `**` and `?` and
`!pattern` exclusions. A kind the manifest does not mention is read from the convention directory `extensions/`,
`skills/`, `prompts/` or `themes/`; `[]` means none. An extension is a `.ts`, `.js` or `.mjs` file, or a folder with an
`index` of those; a skill is a folder with a `SKILL.md`; a prompt template is a `.md` file and a theme a `.json` file.
A path that leaves the package is an error. Moebius loads the extensions and skills. Prompt templates and themes do not
apply to RPC runs: they are listed and not loaded.

**Naming a package.** `pkg:<name>` in an agent's `extensions` is every extension of the package, in the order the
package lists them, and `pkg:<name>/<path>` one of them (the path as the package lists it, such as
`pkg:pi-web-access/extensions/search.ts`); in `skills` the names are the same for the skills, `<path>` being the skill's
folder. They are resolved from `packages.lock` to the files in `packages/` and given to Pi-Bolt as `--extension` and
`--skill` where the agent's other extensions and skills are, so before the bridge. A name whose package is
not in `packages.lock`, a package without a directory, a path the package does not list, and a whole-package name for a
package that has none of that kind, are errors of the agent, like a missing file.

**Changes.** Installing, updating and removing a package are made by `moebius ext` (see [the command line](cli.md))
and `POST /v1/extensions/packages` (see [the desktop API](desktop-api.md#get-v1extensionssearch)), each on a branch
`moebius/ext-<action>-<name>-<n>` of the clone, in one commit whose message gives the source, version, download and
integrity and what the package has. The branch passes the same check as a chat's (it changes only harness configuration
files, and every agent can still be read) and is offered as a change of the harness configuration repository, with a
`ready_to_merge` inbox item. A person merges it; the run token of the Moebius agent can propose a package and cannot
merge a change, or a proposal of the Retro, that changes `packages/` or `packages.lock` (`403`). A removal also takes the
package's names out of the agents that have them. The nightly check puts a `extension_update` item in the inbox for each
package with a newer version (see [the desktop API](desktop-api.md#get-v1extensionspackages)).

## `rules.md`

The server's rules, in the format of [rules](rules.md): one `## name` section per rule. It replaces `rules_file`, and has
the same precedence over a project's `repos/<repository>/rules.md`: the server's rules come first and a project cannot
weaken them. Like `rules_file` it is read afresh for every tool call, from the clone's working tree, which is the
default branch. Without the file there are no server rules.

## `mcp.toml`

The MCP servers agents can name. Each server is a top-level table named for it, such as `[docs]`. A name is made of
letters, digits, `-` and `_`. A server is one of two kinds, and a table that is neither or both is an error:

| Key | Type | Meaning |
|---|---|---|
| `command` | string | A stdio server: the executable, not a shell command line. Not blank. |
| `args` | array of strings | Its arguments. Only with `command`. Default none. |
| `env` | table of strings | Its environment: the server starts it with a cleared environment (only `PATH`, `HOME`, the locale, the proxy and certificate settings and the like are kept; the list is in the [bridge protocol](bridge-protocol.md#the-environment-of-the-harness)) and adds these. Only with `command`. Default none. |
| `url` | string | A remote server (streamable HTTP). |
| `headers` | table of strings | Sent with every request, such as `Authorization`. Only with `url`. Default none. |

Unknown keys are errors. Every string value may contain `${NAME}`, which is replaced by the value of the environment
variable `NAME` of the Moebius server, when the run starts. Only the names in `mcp_env` of `[harness]` in
`moebius.toml` may be used (see [configuration](configuration.md)); any other name is an error for the server, so that
a repository cannot read secrets such as a provider's key. A name that is listed but not set in the server's
environment fails a run of an agent that uses the server (`cannot write the agent directory: ...`). Never write a
secret into the repository: keep it in the server's environment and take it with `${NAME}`.

```toml
[docs]
command = "docs-server"
args = ["--root", "/srv/docs"]
env = { API_KEY = "${DOCS_TOKEN}" }

[search]
url = "https://search.example.com/mcp"
headers = { Authorization = "Bearer ${SEARCH_TOKEN}" }
```

A server that cannot be used leaves out the agents that name it, with the error, like a missing skill. The other
servers and agents are not affected.

### What a run is given

A run reaches its servers only through the MCP gateway, Moebius's one MCP server for every harness (see the
[bridge protocol](bridge-protocol.md#the-mcp-gateway)). A run of an agent that names servers has the file
`<run-dir>/agent/mcp.json` in Pi's format with one server, `moebius`: the gateway's URL (`<router>/mcp`) and the run's
token as `Authorization: Bearer <token>`. The servers themselves, with the values of `${NAME}` filled in, are given to
the gateway for the run's token, so the run sees neither the servers' commands nor their values. Pi-Bolt is started
with `--extension builtin:mcp` before the agent's own extensions (the bridge stays last), because Pi's MCP support is
an extension that `--no-extensions` turns off. The gateway has `"exposure": "direct"`, so its tools are declared to the
model like Pi's own tools.

At the run's first request the gateway starts every stdio server as a child process of the Moebius server, in the run's
worktree, and connects to every remote one. It lists their tools as `<server>__<tool>`, so Pi names them
`mcp__moebius__<server>__<tool>`. Every call is checked twice: by the bridge under Pi's name, like any tool call, and by
the gateway under the name `mcp__<server>__<tool>`, so that a harness without a bridge is checked too. A refused call
never reaches the server. A server that cannot be started or reached leaves out its tools and is reported as an
`mcp_error` run event; the other servers still serve. The servers of a run are stopped when the run ends. A run of an
agent without servers has no `mcp.json` and no MCP support, and no agent is given a server of another agent.

A run's `<run-dir>/agent/models.json` names the router as the `moebius` provider with the agent's model, marked as one
that reasons when the agent's reasoning level is not `off`. When the model's endpoint names the reasoning levels
differently, the model also has a `thinkingLevelMap`, the value to send for every level whose own name the endpoint does
not take (see [the router](router.md#reasoning-level-names)); a model whose endpoint takes the levels' own names, and a
model class, has none, and the harness sends the level's name.

`mcp.json` and `models.json` (which both hold the run's token) can be read only by the server's user (mode `0600`).
A stdio server runs with the Moebius server's environment and its own `env`, and works in the run's worktree, so it can
read and change what the agent works on: give a run only servers you trust.

## Project settings

A project is a repository the server works on, and its settings are the files of `repos/<repository name>/`, the
repository's name as `GET /v1/snapshot` shows it. They replaced the `.moebius/` folder that a repository used to keep on
its default branch: nothing is read from a repository's `.moebius/` any more. A repository whose default branch still
has one is told so once while the server runs, in the log and as the event `settings_moved`, and
`GET /v1/repositories/{id}/settings` lists its files as `leftover`; Moebius imports nothing, so move the files by hand
(`git mv .moebius/<file>` becomes a commit of `repos/<name>/<file>` here). Every file is optional, and a server without
`[harness]` has no project settings at all. They are read afresh when they are used, from the clone's default branch, and
an edit through the API is a commit pushed to `origin`, like any other edit of this repository. A file that is a symbolic
link in the clone is not read, and not written. A repository whose name is a device name of Windows (`con`, `prn`, `aux`,
`nul`, `com1` to `com9`, `lpt1` to `lpt9`, in any case, with or without an extension) has no settings: a directory of
that name cannot be made there, so saving a file is refused with `422` on every system.

| File | Meaning |
|---|---|
| `rules.md` | The project's own rules, in the format of [rules](rules.md). They are added after the server's rules, which a project cannot weaken. |
| `review.md` | Notes for the reviews of the project's changes, given to the Standards review run in its prompt (the files the repository itself keeps, such as `CONTRIBUTING.md`, are read by the reviewer from the worktree). |
| `checks` | The command that checks a change of a repository without a forge (see [column scripts](column-scripts.md#checks-and-conflicts)). |
| `test-first` | Its presence turns test first on (content ignored; see [column scripts](column-scripts.md#test-first)). |
| `localize` | Turns the hint of likely files for code runs on or off for the project, and says how many files (see [configuration](configuration.md#localize)). |
| `<file>.luau` | The script of a column in place of the built-in one: `backlog`, `ready` (for `Not started`), `code`, `review`, `merge` and `done` (see [column scripts](column-scripts.md#repository-overrides)). |
| `plugins/<plugin>/<key>` | The value of one setting that a plugin asks of every repository in its manifest ([`project_settings`](plugin-api.md#plugin-settings)), such as the Dockerfile of a sandbox: the file's text is the value, and an empty or missing file is no value. A person saves them with the repository's page or [`PUT /v1/repositories/{id}/plugin-settings/{plugin}`](desktop-api.md#put-v1repositoriesidplugin-settingsplugin), one commit for all of a plugin's keys. The Retro and packages do not propose them. Plugin and key are letters, digits, `-`, `_` and `.`. |

A change to these files that a package or the Retro proposes is a branch of this repository like any other, merged by a
person (see [Packages](#packages) and [Proposals for Moebius itself](#proposals-for-moebius-itself)). The Retro's proposals
may change only `rules.md` and the column scripts of a project, never `checks`, which the server runs as a command.

## A broken agent

A file that cannot be used leaves only its agent out; the other agents are read and used. The agent is reported:

- by the event `harness_error`, `{"agent": name, "message": string}`, each time the repository is read (see the
  [desktop API](desktop-api.md));
- in `GET /v1/agents` as an entry with an `error` and an empty `model`, so that a client can show it and not offer it;
- by a run or a chat that asks for it, which fails with `the agent <name> cannot be used: <error>`.

The errors: a name that is not valid; `agents/<name>.md` without `agents/<name>.toml`; a `.toml` file that is not valid
TOML, has an unknown key or a value of the wrong type; a missing `model`; a skill, extension or MCP server that is not
there or cannot be used (see [`mcp.toml`](#mcptoml)). An error in `agents/Moebius.toml` is reported the same way, and the Moebius agent then runs as if the file were
not there. A fetch that fails is reported as a `harness_error` without an `agent`, and the agents stay as they were.

## Edits through the API

These routes are in the [desktop API](desktop-api.md). No secret belongs in these files and none is kept in them:
model providers' keys are read from the environment by the router, so the files can be read and written through the API
as they are.

- `GET /v1/agents/{name}` answers `{"name", "settings", "instructions"}`, where `settings` has the keys of the table above
  (`protocol` always, `skills`, `extensions` and `mcp` always, `thinking` and `token_budget` when set, `tool_output` when it is not the defaults, `engineering_skills` when it is `false` or a list) and `instructions`
  is the text of the `.md` file, empty without one.
- `PUT /v1/agents/{name}` takes `{"settings": {...}, "instructions": "..."}` (`instructions` may be left out) and
  is checked as a file read is. It writes `agents/<name>.toml` (the file is written again from the settings, so comments
  in it are lost) and `agents/<name>.md` (removed when the instructions are blank), commits them on the default branch as
  `Moebius <moebius@localhost>` with the message `Add the agent <name>` or `Update the agent <name>`, pushes the branch
  to `origin`, and then records the decision (kind `edit_agent`, decider `person`, the settings and instructions in its
  context). An edit that changes nothing commits nothing and records nothing.
- `DELETE /v1/agents/{name}` removes both files the same way, with the message `Delete the agent <name>`.
- `PUT /v1/skills/{name}` takes `{"description": "...", "text": "..."}` and writes `skills/<name>/SKILL.md`: the
  frontmatter `name` and `description` (a double-quoted line), then the text. The skill's other files are kept. The
  message is `Add the skill <name>` or `Update the skill <name>`, the decision kind `edit_skill`. `DELETE` removes the
  directory (`Delete the skill <name>`). The built-in skills (`moebius`, the task skills and `moebius-autonomous`) cannot be changed or
  deleted; the engineering skills cannot while the repository has no skill of the same name, and then they are the
  repository's own.
- `PUT /v1/skills/{name}/files/{*path}` takes `{"text": "..."}` and writes one of the skill's other files, such as
  `scripts/fill.py`, with its directories (`Add <path> of the skill <name>` or `Update <path> of the skill <name>`,
  the decision kind `edit_skill`); `DELETE` removes it and the directories it leaves empty (`Delete <path> of the
  skill <name>`). `SKILL.md` is written only with the skill, and a path through a symbolic link is refused.
- `PUT /v1/mcp-servers/{name}` takes the keys of a server of [`mcp.toml`](#mcptoml) and rewrites its table in the file,
  checked as reading checks it (so a `${NAME}` outside `mcp_env` is refused). The table is rewritten in place, or added
  at the end; the other tables and the comments of the file are kept, but comments inside the rewritten table are lost.
  The message is `Add the MCP server <name>` or `Update the MCP server <name>`, the decision kind `edit_mcp_server`.
  `DELETE` removes the table (`Delete the MCP server <name>`).
- A skill or an MCP server that an agent names is not deleted (`409`, naming the agents).
- `PUT /v1/rules` takes `{"text": "..."}`, writes it to `rules.md` and commits and pushes it the same way, with the
  message `Update the rules` and the decision kind `edit_rules`.
- `PUT /v1/workflows/{name}` takes a workflow as the whole TOML file or as its parsed definition, checks it as reading
  checks it, and writes it to `workflows/<name>.toml`; the message is `Add the workflow <name>` or `Update the workflow
  <name>`, the decision kind `edit_workflow`. `DELETE` removes the file (`Delete the workflow <name>`), and is refused
  while a run of the workflow is queued or running.

If anything fails after the commit was made, including the push, the commit is undone (`git reset --hard` to the head
before it), so the clone never drifts from `origin`, and no decision is recorded. The request then fails with the
error. When someone else pushed to
`origin` meanwhile, the push is refused for that reason, and the next refresh brings their commits in.

## Changing it in a chat

A chat can work in the harness configuration repository, so that an extension, a skill, an MCP server, an agent or the
server's rules are built as files and merged like any change. `POST /v1/chats` with `harness: true` (refused with `422`
without `[harness]`, and together with `repository`) starts the chat in a worktree on the new branch
`moebius/chat-<run id>` of the clone's default branch, made as a chat's worktree in a repository is. The run has
`harness_repository` true. A chat that continues an ended chat of this kind (`continues`) works there too, unless it names a
`repository`.

When the chat ends or is stopped, what it left is committed on that branch like any chat's work. If the branch has commits
beyond the default branch, it is checked the way a branch of a Retro proposal is (see [The Retro](#the-retro)):

- it changes only harness configuration files: `agents/`, `skills/`, `extensions/`, `packages/`, `packages.lock`, `rules.md` and `mcp.toml`;
- every agent that can be read on the default branch can still be read with the branch's files (a skill an agent names
  must exist in `skills/`, an extension in `extensions/` or wherever its path points, an MCP server in `mcp.toml`).

A branch that passes is pushed to `origin` and offered as a change of the harness configuration repository: a `Change`
with `repository_id` null, `harness_repository` true, no card and no pull request, in the snapshot's `harness_changes`,
with a `ready_to_merge` inbox item and the event `change_offered`. A branch that fails is not offered: the chat's run gets
a `moebius_error` event saying why, and the branch stays in the clone for the person.

`POST /v1/changes/{id}/merge` merges the branch into the clone's default branch with a merge commit (`Merge
moebius/chat-<run id>`), pushes it to `origin` and reads the agents again, so the next run of an agent has the skills and
extensions that came with it; this is the merge of a trial's proposal. A branch that does not merge cleanly is a `409`
and changes nothing, as does a push that `origin` refuses. The decision is recorded as the merge of any change is (kind
`merge`, with the repository named `harness`).

## The Retro

Once a day (`retro_at`, see [configuration](configuration.md)) Moebius collects the day's errors without a model into a
report (see `POST /v1/retros` in the [desktop API](desktop-api.md)). When the day had errors and `[harness]` is
configured, the built-in agent `Retro-analyst` turns the report into proposals. Agent names allow no spaces, so it is
`Retro-analyst`, not "Retro analyst".

- The agent is built in like the Moebius agent: its instructions are built in, it has the `moebius` skill (which holds
  these reference pages), and its model comes from `agents/Retro-analyst.toml` or else from the Coder. `agents/Retro-analyst.md`
  is added after the built-in instructions; `agents/Retro-analyst.toml` may set its model, skills and the rest.
- Its run has the purpose `retro` and no card. It works in a fresh detached worktree of the clone at the default branch
  (`<data_dir>/worktrees/harness/retro-<run id>`), which is removed when the run ends. It is told the report as JSON. It
  never waits for a person.
- It reads the raw traces too, not only the report's summary of each error: before it starts, the events of each run the
  report names are written to `.retro/run-<id>.events.jsonl` of its worktree (one JSON object a line: `seq`, `kind`,
  `data`, `at`), and what a run of a card changed, from the commit it started at to its branch, to `.retro/run-<id>.diff`
  while the branch is there. Every secret is redacted; at most the report's first 20 runs are written, each file at most
  200 000 bytes (an events file then ends with a `moebius_cut` line saying how many events are left out). Git leaves
  `.retro/` out (the clone's `info/exclude`), and the analyst is told never to commit it.
- The analyst also writes lessons (see `GET /v1/lessons` in the [desktop API](desktop-api.md)): it ends its final answer
  with a fenced block ```` ```lessons ```` holding a JSON array of `{"card", "kind", "situation", "lesson", "advice"}`,
  one for a failed run, a review rejection or a card that merged at once (`merged` in the report). Each is stored for the
  repository of its `card`, so `card` is the number the report line names; an entry of a card that is gone, naming no
  card, with an empty part, or an unreadable block is left out with a warning. A lesson is retired when it is not
  retrieved for 60 days (checked every minute) or when a trial run it was given to is negative.
- Each proposal is a branch `proposal/<date>-<short-name>` made from the default branch, with one commit whose message is
  the proposal: the subject is its title, the body says what changes and which errors it addresses, and the last line is
  `Cards: 48, 50`, the `card` of each error of the report that it addresses (numbers that are not ids are ignored). It
  changes only harness configuration files (`agents/`, `skills/`, `extensions/`, `packages/`, `packages.lock`, `rules.md`, `mcp.toml`, and the files of a project's settings, `repos/<repository>/<file>`).
- When the run has succeeded, Moebius lists the `proposal/<date>-*` branches of the clone. A branch that changes
  nothing, changes another file, or breaks an agent that could be used on the default branch is left out, with a warning
  in the Retro. The others are pushed to `origin` and stored as proposals (`GET /v1/proposals`), with the agents whose files
  they change and the cards of the `Cards:` line. Nothing is merged without a person's decision, after the trials.

### Proposals for Moebius itself

Some errors are better avoided by Moebius's own configuration than by the harness configuration. The analyst may also
propose a change to `moebius.toml` or to a project's own rules and column scripts. It states one the same way, as a
branch `proposal/<date>-<short-name>` with one commit whose message is the proposal, but the commit adds only files under
`moebius/`, which is not harness configuration:

- **A change to `moebius.toml`** adds only `moebius/settings.json`: a JSON object `{"set": {"<key>": value}, "remove":
  ["<key>"]}` with dotted keys, as `PUT /v1/settings` takes it (see the [desktop API](desktop-api.md)). It may name only
  `run_limit`, `retro_at`, `trial_window.start`, `trial_window.end`, `board.round_limit`, `budgets.day_tokens`,
  `checks.timeout_seconds`, `teacher.model`, `teacher.protocol` and `teacher.timeout_seconds`; the analyst is told
  their values now. The file must still be a valid [configuration](configuration.md) with the change.
- **A change to a project's settings** adds only files `moebius/repositories/<repository>/<file>`, each
  the whole new text of that file, all for one repository. The files are `rules.md` and the column scripts
  (`backlog.luau`, `ready.luau`, `code.luau`, `review.luau`, `merge.luau`, `done.luau`); rules must be fit to save (see
  [rules](rules.md)) and a script must pass `moebius check-script`. The analyst is told the files that the projects
  of the report's cards have now.

When the run has succeeded, these branches are read first and then deleted from the clone; they are never pushed. A
branch that breaks these conditions, or changes nothing, is left out with a warning in the Retro; a branch that mixes
files under `moebius/` with harness configuration is left out as one that changes another file. Each other one is
stored as a proposal of the kind `settings` or `repository` with a diff of what it changes. Trials do not apply to
them: each is `untried` and in the inbox at once, as its diff with the Retro's reason (the body of its commit message).
Nothing is applied without a person:

- Merging a `settings` proposal saves the change to `moebius.toml` through the same code as `PUT /v1/settings`; the
  live keys take effect at once, and when a key needs a restart the server restarts.
- Merging a `repository` proposal commits its files, as `repos/<repository>/<file>`, on the branch
  `moebius/proposal-<id>` of the harness configuration repository, from its default branch, and offers it as a change of
  that repository. The change is in the inbox, ready to merge, and a person merges it as any change of this repository
  (the files are on its default branch, and in use, only then).
- Dismissing one changes nothing.

### The trials

A stored proposal is `open`. Inside `[trial_window]` (see [configuration](configuration.md)), or when a person asks
(`POST /v1/proposals/{id}/trial`), it is tried, one proposal at a time, on each card of its `Cards:` line, one after the
other. A proposal with no cards, a card that is gone, a card the report has no error of, and a card without a code run
cannot be tried: that trial is `skipped`, with the reason. A trial that cannot be made (the agent cannot be used on the
proposal's branch, the run cannot start) is `failed`.

- **The run.** A run with the purpose `trial` and the card's id, by the agent that made the card's error in the report
  (the first error of the card that names an agent; the Coder otherwise), given the prompt a `code` run of the card
  gets. It works in a fresh worktree of the card's repository at the repository's default branch, on a branch of its own,
  `moebius/trial-<proposal id>-card-<card id>`. The agent's definition (settings, instructions, skills, extensions and MCP
  servers) is read from the proposal's branch, in a throwaway worktree of the harness clone, not from the default branch.
  Like a `code` run it fails when it changes nothing. A trial run is no part of the card's own history: the card's runs,
  rounds and tokens do not count it, and the next Retro's report leaves its errors out.
- **The checks.** When the run succeeded and the repository has no forge, its own checks (the project's `checks`) run on
  what the run made.
- **The comparison** is with the card's first code run, the one its errors happened in. A trial is **positive** when all of
  these hold, else **negative**, with the reasons that do not:
  - the run succeeded;
  - none of the addressed errors happened in the run: the kinds of the card's errors in the report, looked for in the run
    the way the report looks for them (failed and refused tool calls, a failed or stopped run, steering, and failed
    checks);
  - the checks passed, or failed before as well (the card was once sent back because they failed), or there are none;
  - the run used no more than 1.5 times the tokens of the first code run.

  A trial is one round, and the card needed at least one, so rounds never make a trial negative.
- **Afterwards** the trial's branch and worktrees are removed; the trial is stored (`GET /v1/proposals`) with the
  measurements and the reason. Unless these trials made it negative, the proposal is then tried on the regression set
  (below). The proposal becomes `positive` when every trial of both was positive, `negative` when some was
  negative, and else `untried`. A `positive` or `untried` proposal gets an inbox item of the kind `proposal`; a `negative`
  one gets none, and stays on the Retro page.
- **Deciding.** `POST /v1/proposals/{id}/merge` merges the branch into the default branch of the clone with a merge commit,
  pushes it to `origin` and reads the agents again; `POST /v1/proposals/{id}/dismiss` leaves it. Both record the person's
  decision (kind `merge_proposal`) and resolve the inbox item. A merge that conflicts changes nothing.
- A server that starts finds the proposals the last server was trying and opens them again, without their trials, and ends
  the trial runs it left going as it ends other runs.

### The regression set

A proposal that fixes its own cards may still break what worked before, and changes to skills and instructions often
help nothing or harm, so each is measured on past work as well. The regression set of a repository is a fixed sample of
the cards merged there: the latest `regression.size` (default 5; see [configuration](configuration.md#regression)), chosen
among the latest 50 merged changes, the cards whose code runs name a reproduction test (`REPRODUCE:` in their summaries)
first, latest first among equals. It changes only as new cards merge. A card is in it only when its first code run
recorded the commit it started at; the proposal's own cards are left out.

- **The runs.** After its own trials, a proposal is tried on the regression set of each repository its cards belong to,
  one card after the other, as above: a `trial` run of the card's task by the agent of its last code run, with the
  definition from the proposal's branch, in a fresh worktree at the commit the card's first code run started at (the
  repository as it was when the card was worked on), on `moebius/trial-<proposal id>-card-<card id>`, then the checks.
- **The comparison** is with the card's code runs together, which passed: the card was merged. A regression trial is
  positive when the run succeeded, the checks passed (or there are none), and it used no more than 1.5 times their
  tokens; else negative. It is one round, never more than the card needed.
- **The bound.** A proposal adds at most `regression.size` runs for each repository of its cards. They run on the
  proposal's own thread, one at a time, like its other trials (so one proposal is tried at a time, and the schedule starts
  one only inside the trial window and when the board has a run to spare); the first regression card that fails ends the
  rest, which are stored as `skipped`. A proposal whose own trials are negative is not tried on it.
- Each regression trial is stored with `regression` true. A proposal that fixes its cards but makes a regression card
  fail is `negative` and not offered; the inbox item of one that is offered lists both results. `regression.gate =
  false` turns all of this off.

### The replays

Inside `[trial_window]`, when no proposal is being tried, a sample of refused tool calls is run again in a throwaway
copy of the worktree, so that a refusal can later be weighed by what the call would have done. `POST
/v1/judgements/{id}/replay` replays one at any hour.

- **Which calls.** `tool_call` judgements that refused the call, have no replay yet, are at most a week
  old, belong to a run whose repository still exists, and are **reversible**. At most 20 are replayed per night, the
  oldest first, one after the other on a thread of their own (the schedule goes on meanwhile, and never two
  replay threads run at once); no trial starts while a replay thread is going (it waits for the next minute), and a replay thread stops when a
  proposal is `trying`. Calls that cannot be replayed are never picked and keep their outcome. A refusal on a card that is
  done already has the card's outcome (such as `merged`); its replay replaces it, since for a refusal what the call would
  have done says more, and the card still tells how it ended.
- **Reversible** means everything the call does stays inside the run's worktree: `read`, `write`, `edit`, `ls`, `grep`
  and `find` on paths inside it; and `bash` whose commands, read as the guard's patterns read them (nested shells, `cd`,
  redirects, `tee`), name only paths inside it. Not reversible: another tool; a path outside the worktree or `~`
  (`/dev/null` and the standard streams excepted); `git push`, `fetch`, `pull`, `clone`, `remote`, `ls-remote`,
  `submodule`; a URL; `curl`, `wget`, `ssh`, `scp`, `rsync`, `sftp`, `ftp`, `nc` and the like; `sudo`, `su`, `kill`,
  `pkill`; package managers and `cargo`; `env`, `printenv`, `export` and any `$` (variable or substitution);
  interpreters such as `python` and `node`; a shell nested too deeply to follow. This reads the command line, it is not a
  sandbox, so it errs on the side of not reversible. A `bash` command is replayed only when it also names **no
  absolute path** (even one inside the worktree, which would act on the original and not on the copy).
- **The copy** is a directory `<data_dir>/replays/<judgement id>` holding the files of the run's worktree if it still
  exists (not `.git`, and no symbolic links), else a `git archive` of the card's change branch, else of the repository's
  default branch (its symbolic links removed after extracting). Git does not look above the copy for a repository
  (`GIT_CEILING_DIRECTORIES`), and the file tools refuse a path that leaves the copy. `read`, `write` and `edit` are applied by Moebius (an edit's text must occur exactly once); `ls` is
  listed; `grep`, `find` and `bash -c` run in the copy with an empty environment (`PATH` only, `HOME` the copy), no input
  and 60 seconds, after which the process group is killed. The copy is removed afterwards; the worktree is never touched.
- **What is stored** (`GET /v1/replays`): the exit code (or that it timed out), the output (secrets removed, last 4 KB)
  and the files created, changed and deleted, found by comparing a listing of the copy with sizes and SHA-256 hashes
  before and after. Directories are not listed, so an empty one is not noticed.
- **The outcome** of the judgement is one sentence: `replayed: <ended>; <files>`. `<ended>` is `exit <code>`, `timed
  out` or `ended by a signal`. `<files>` is `created N files`, `changed N files` and `deleted N files` (those that are
  not zero, in that order, joined by `, `; `1 file` for one) or `no file changed`. For example `replayed: exit 0;
  deleted 3 files, changed 1 file`. A replay that could not be made (the copy or the checkout failed) reads `replayed: not
  run; <why>` and is stored too, so it is not tried again. The event `replayed` carries the sentence.

## Autoresearch

A research (see `POST /v1/research` in the [desktop API](desktop-api.md)) is driven by the built-in agent
`Autoresearcher`. Like `Retro-analyst` it is built in: its instructions are built in, it has the `moebius` skill, its model
comes from `agents/Autoresearcher.toml` or else from the Coder, and `agents/Autoresearcher.md` is added after the built-in
instructions. It works with or without a harness configuration repository (without one, from `[agents.Autoresearcher]`).

- Each round is a run with the purpose `research` and no card, in a fresh detached worktree of the researched repository at
  its default branch (`<data_dir>/worktrees/<repository>/research-<run id>`), which is removed when the run ends. It is
  told the goal, the measure, the direction, the baseline and every experiment so far with its result. It never waits for
  a person. The runs of a research are no part of the Retro's report.
- Each lever is a branch `research/<research id>-<short-name>` made from the default branch, with one commit whose message
  says what the lever is. A branch without a commit beyond the default branch is ignored. A branch the research has an
  experiment for is not tried again, so a round that leaves no new branch ends the research.
- A research is limited by its brief's budget, a number of experiments or of tokens. With a budget in experiments the
  prompt of a round says how many experiments are left (the built-in instructions ask for no more levers than that), and
  no experiment starts once they are used, even in the middle of a round; with a budget in tokens the tokens of the
  research's runs are compared with it before each round.
- Each experiment runs the measure and, if the measurement succeeded, the repository's own checks (see [the
  trials](#the-trials)), each in a fresh worktree of the branch. The branches stay in the clone, kept or not.

## Grilling

A grilling (see `POST /v1/grillings/{id}/confirm` in the [desktop API](desktop-api.md)) is a chat with the built-in agent
`Grilling-partner`, which grills a person's idea until the work is clear. Like the Moebius agent it is built in: its
instructions are built in, it has the `moebius` skill and the engineering skills `grilling`, `to-tickets`, `wayfinder`, `domain-modeling`, `research` and `prototype` with `moebius-autonomous` (see [Engineering skills](#engineering-skills)), and its model comes from `agents/Grilling-partner.toml` (or
`[agents.Grilling-partner]`) or else from the Coder. `agents/Grilling-partner.md` is added after the built-in instructions;
`agents/Grilling-partner.toml` may set its model, skills and the rest. It is listed by `GET /v1/agents` after the Moebius
agent, and a chat with it needs a repository.

- The agent asks one question at a time and challenges assumptions. It works in the chat's worktree, a worktree of the
  repository, so it can read the code. It never waits for the person in any other way than by asking in the chat.
- It ends by writing exactly one of two files in the worktree. It never commits them and never pushes:
  - `.moebius/grilling/issues.json`, when the idea can be built: a JSON array of small issues, each doable in one card.

    ```json
    [
      {"title": "Add the export button", "body": "A button on the report page that downloads a CSV.", "blocked_by": []},
      {"title": "Write the CSV", "body": "The columns are the report's.", "blocked_by": [0]}
    ]
    ```

    `title` is a non-blank string, `body` a string (may be left out) and `blocked_by` an array of indexes of issues of the
    same array that must be done first (may be left out). An index out of range, the issue's own index or a circle makes the
    file unusable.
  - `.moebius/grilling/report.md`, when the idea needs research rather than issues: a research report in Markdown.
- A grilling does not offer its worktree as a change: nothing is committed on its branch and no `ready_to_merge` item is
  made, whatever the agent left outside `.moebius/grilling/`.
- If both files exist, `issues.json` decides. A grilling that leaves nothing usable is stored as `empty`, with the reason,
  and makes no inbox item.
