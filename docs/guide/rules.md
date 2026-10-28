# Forbid something to the runs

## When you want this

You want every run, or the runs of one repository, to stay away from something: a directory, a kind of command, a file
format. A rule is a short text with a name. Moebius checks every tool call of every run against the rules before the
call runs. A call that breaks one is refused with the rule's name, the model sees the refusal as the tool's result and
finds another way, and the run goes on. A run never waits for a person.

The exact format and every built-in pattern are in [`docs/reference/rules.md`](../reference/rules.md). This page is how
to use them.

## What is refused without any rule from you

Four built-in rules always apply, with fixed patterns that look at the tools `bash`, `write` and `edit`:

| Name | Refuses |
|---|---|
| `no-destructive-commands` | `rm` with a recursive flag outside the worktree, `dd` to a device, `mkfs`, a fork bomb. |
| `no-force-push` | `git push` with `--force`, `-f` or a `+` refspec. |
| `stay-in-the-worktree` | Writing a file outside the run's worktree (`write`, `edit`, a redirect or `tee`), except `/tmp/` and `/dev/null`. Reading is allowed anywhere. |
| `no-secrets-in-commands` | A command that contains a key or token. |

The patterns are a first line. They do not see `sed -i`, `cp`, `mv`, scripts the agent writes and then runs, or
variables. Rules in prose are for what the patterns cannot see.

## The steps

1. Write the rules as a `rules.md`: one `## name` heading per rule, then the text.
2. Put the file where it applies:
   - Every run of the server: any path, named by the top-level key `rules_file` in `moebius.toml`. A relative path is
     relative to the directory of `moebius.toml`. Without the key it is `rules.md` next to `moebius.toml`; while that
     file does not exist there are no server rules. With a harness
     configuration repository (`[harness]`, see [Agents](agents.md#in-a-harness-configuration-repository)) the rules are
     its `rules.md` instead, and `rules_file` is an error.
   - The runs of one repository: `.moebius/rules.md`, committed to its default branch. Where to commit is the same as for
     [column scripts](column-scripts.md): push to GitHub, or commit to the source repository (or, once Moebius has merged a change there, to the clone) for a repository without a forge.
3. If any prose rule applies, configure a `[teacher]`. Without one, every tool call is refused (see below).
4. Both files are read again for every tool call, so there is nothing to restart.

A repository can add rules but cannot weaken the server's: a repository rule with the same name as a server rule is
dropped, and the server's text stands. When a name is repeated within one file, the first rule of that name stands.

### Change the server's rules in the desktop app

The Rules page of the desktop app shows the built-in patterns, the rules in
prose and the rules of each repository, with how many calls each rule refused in the last week. The text of the server's
rules is an editor there: with a harness configuration repository, Save commits it to the repository as `Update the
rules` and pushes it; without one, Save replaces the `rules_file` (creating `rules.md` next to `moebius.toml` the first
time). The next tool call is checked against it.
Save is disabled until the text changes, and a refusal of the server is shown in red. "Let the Moebius agent write them"
asks for what should be allowed or forbidden and starts a chat in which the Moebius agent reads the rules, changes them
in prose without weakening any and saves them with `"$MOEBIUS" rules save <file>`.
The same is `GET` and `PUT /v1/rules`, see the [desktop API](../reference/desktop-api.md).

## An example

A rules file for one repository. Text before the first `## ` line is ignored; the name is what the model sees when it is
refused, so keep it short and without spaces.

<!-- rules -->
```markdown
Rules for the billing service.

## keep-migrations
Do not edit or delete a file under migrations/ that already exists. Adding a new migration is allowed.

## no-network-installs
Do not run commands that download and execute code, such as piping curl into a shell.

## generated-code
Files under src/generated/ are produced by a tool. Do not edit them by hand.
```

A rules file for the whole server, which `no-secrets-in-commands` does not cover: it looks at commands only, not at what
a `write` puts in a file.

<!-- rules -->
```markdown
## no-secrets-in-files
Do not write API keys, tokens or passwords into any file, not even as an example. Read them from the environment.
```

The setting that turns the prose rules on is the teacher. This configuration names a server rules file and a
teacher, which is asked through the router, so the router needs an upstream for the teacher's protocol:

```toml
rules_file = "rules.md"

[teacher]
model = "my-flash-model"
protocol = "openai"
timeout_seconds = 20

[[router.upstreams]]
protocol = "openai"
base_url = "http://127.0.0.1:8080"
```

`rules_file` is a top-level key, so it must come before the first `[table]` of the file.

## How prose rules are checked

For each tool call that passed the built-in patterns, the guard asks the teacher model through the router. The question
holds the rules, the tool's name, its input as JSON, the worktree, and the card's title and body. The teacher answers
`allow`, or `refuse` with the name of the rule and a sentence. So write rules about what a tool call shows: a command, a
path, content. A rule about an outcome (`the change must be small`) cannot be judged from one call.

The teacher's tokens count in the run's usage and against the card's token budget.

| Situation | Result |
|---|---|
| No prose rules apply | Every call that passed the patterns is allowed (`no rule is broken`). The teacher is not asked. |
| Prose rules apply and there is a `[teacher]` | The teacher decides each call. A refusal reads `<rule>: <reason>`. |
| Prose rules apply and there is no `[teacher]` | Every call is refused: `no teacher is configured to check the rules in prose`. This fails closed: rules that cannot be checked stop everything, including reads. |
| The teacher is too slow, fails, answers something that is not the JSON asked for, or names a rule that was not given | The call is refused with a reason that starts with `teacher:`. |

## How to try it and see a refusal

Start a chat with an agent (see [Agents](agents.md)) and ask it to do the forbidden thing, for example to run
`git push --force`. Or put a card through the workflow whose issue asks for it. A chat has no card, so only the built-in
and the server's rules apply to it; a repository's rules apply to the runs of its cards.

Every checked call is stored in the run's events as a `tool_check`. Read them and keep the refusals:

```sh
curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/runs/$RUN/events?limit=5000" \
  | jq '.[] | select(.kind == "tool_check" and .data.decision == "refuse") | .data'
```

```json
{"tool":"bash","tool_call_id":"call-1","decision":"refuse","reason":"no-force-push: git push --force rewrites history on the remote"}
```

The same events arrive on `GET /v1/events` as `run_event` with the kind `tool_check`. The judgement behind each call also
appears in the card's history, `GET /v1/cards/{id}/history`, as an entry `decided` with the kind `tool_call`, the
choice, the reason and the decider: `guard/patterns` for the built-in rules, for a rules file that cannot be read and for
calls with no prose rule to check, `guard/teacher` for the teacher's answers and for the refusal when there is no
teacher. Every call is a judgement, allowed ones too, so the history of a card
that has run is long.

## What can go wrong and how you see it

| What happens | How you see it |
|---|---|
| The agent keeps trying the same call. | Many `tool_check` refusals with the same reason. The model gets the reason as the tool's result; make the rule's text say what to do instead. |
| Everything is refused with `the rules cannot be read, so no call is allowed: ...`. | The server's rules file exists but cannot be read (permissions, a directory, text that is not UTF-8). The decider is `guard/patterns`. A file that does not exist is not an error: it gives no rules. |
| Everything is refused with `no teacher is configured ...`. | A prose rule applies and there is no `[teacher]`. Add one, or remove the rule. |
| A repository's rule has no effect. | The file is not committed to the default branch, or a rule with that name is in the server's file. A repository file that cannot be read gives no rules and no error. |
| A rule is ignored by the teacher. | The text is too vague to judge from one call. Name the paths or commands. The rule's name must also be one the teacher can repeat exactly. |
| Refusals say `teacher: ...`. | The teacher failed or timed out (`teacher.timeout_seconds`, from 1 to 29), or the upstream for its protocol is missing from `[[router.upstreams]]`. |
| A harmful call got through. | The patterns do not see everything (`sed -i`, `cp`, `mv`, variables, scripts). Add a prose rule for it. |
