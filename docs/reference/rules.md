# Rules

A rule is something a run must or must not do. Moebius checks every tool call of every run against the rules before the
call runs, and a call that breaks one is refused with the rule's name, so that the model can find another way; the run
goes on. This page is for anyone who writes a `rules.md` file or wants to know exactly what the built-in rules refuse.
The code is in `crates/moebius-core/src/rules.rs` (the file format), `guard.rs` (the built-in rules and the teacher) and
`check.rs` (where they are asked).

## The `rules.md` format

A `rules.md` is Markdown with one section per rule.

- A line that starts with `## ` (two number signs and a space) starts a rule. The rest of the line, trimmed, is the
  rule's name.
- The rule's text is every following line up to the next line that starts with `## `, with leading and trailing blank
  lines removed. Deeper headings (`###`) and a single `#` heading are part of the text.
- Text before the first `## ` line is ignored. Use it for a title or an introduction.
- A section with an empty name is dropped. A rule may have an empty text.

```markdown
Rules for this repository.

## leave-ci-alone
Do not edit files under .github/ or any other continuous integration configuration.
```

Pick short names without spaces, such as `leave-ci-alone`. The teacher model must repeat a name exactly to refuse for
it, and the name is what the model sees in the refusal. Write the text as an instruction to a person: say what is not
allowed and, where it is not obvious, why. [`examples/rules.md`](../../examples/rules.md) is a working file.

## Where rules come from

Two files, read afresh for every tool call:

1. The server's file, named by `rules_file` in the configuration. Without that key there are no server rules.
2. The repository's `.moebius/rules.md` on its default branch, read with `git show <ref>:.moebius/rules.md`. `<ref>` is
   the default branch of the clone for a repository without a forge and `origin/<default branch>` for a GitHub
   repository. A file that is not committed to that branch is not read. A run whose card is gone has no repository
   rules.

The server's rules come first, then the repository's. A repository adds rules and cannot weaken the server's: a rule in
the repository's file whose name equals the name of a rule already listed (compared exactly, case included) is dropped,
and the first one stands. That also drops a repeated name within the repository's own file. A file that is missing or
cannot be read gives no rules.

The built-in rules below are not in any `rules.md`; they always apply.

## How a call is checked

The guard (`guard/…` in a judgement's decider) judges a `tool_call` question in two steps.

1. The built-in rules are tested in the order below, with fixed patterns. The first one the call breaks refuses it. The
   judgement's decider is `guard/patterns`. The teacher is not asked.
2. The rules from the files above (the prose rules) are judged by the teacher model.
   - No prose rules: the call is allowed, with the reason `no rule is broken` and the decider `guard/patterns`.
   - Prose rules and a teacher (`[teacher]` in the configuration): the teacher is asked for every call that passed the
     built-in rules. The decider is `guard/teacher`.
   - Prose rules and no teacher: the call is refused with the reason `no teacher is configured to check the rules in
     prose`, decider `guard/teacher`. This is fail closed: if rules exist that cannot be checked, nothing runs.

Whatever the guard cannot answer is a refusal too. The model sees a refusal as `Moebius refused this call: <reason>`.

### The built-in rules

The patterns look only at the tool `bash` (its `command` input) and the tools `write` and `edit` (their `path` input).
All other tools pass them. The run's worktree is the directory a run works in; a path is "inside" when, with `.` and
`..` resolved without touching the disk, it is the worktree or below it. A run without a worktree has no place that is
its own, so every path is outside.

For `bash`, the command is cut into simple commands at `;`, `|`, `&`, new lines, parentheses and backticks. In each,
quotes around words are removed, and leading `sudo`, `env`, `command` and words like `NAME=value` are skipped. The first
remaining word is the command.

| Name | Refuses | Allowed near misses |
|---|---|---|
| `no-destructive-commands` | `bash`: `rm` with a recursive flag (`-r`, `-R`, `-rf`, `-fr`, `-r -f`, `--recursive`) and a target outside the worktree, where `~`, `$HOME` and `${HOME}` at the start of a target count as outside, and so does `..` and a sibling directory such as `/srv/wt/10` next to `/srv/wt/1`; `dd` with an `of=/dev/...` argument; any command whose name starts with `mkfs`; a fork bomb such as `:(){ :\|:& };:`. Reason: `commands that delete recursively outside the worktree, format a disk, overwrite a device or exhaust the machine are never run`. | `rm -rf ./target`, `rm -rf build/`, `rm -rf /srv/wt/1/target` (inside), `rm file.txt` and `rm -f /tmp/x` (not recursive), `dd if=/dev/zero of=./disk.img`, `echo mkfs`, `ls /`. |
| `no-force-push` | `bash`: `git push` (after any `-C <dir>` or `-c <name=value>` options) with an argument that starts with `--force` (so `--force-with-lease` too), a short-flag group containing `f` (`-f`, `-uf`), or a refspec starting with `+` (`+main`). Reason: `git push --force rewrites history on the remote`. | `git push`, `git push origin HEAD`, `git push -u origin feature`, `git commit -m "push -f"`, `echo git push --force`, `git log --format=%h origin/+main`. |
| `stay-in-the-worktree` | `write` and `edit`: a `path` outside the worktree, a `path` that starts with `~`, `$HOME` or `${HOME}`, or no `path` string at all. Reason: `files are written only inside the run's worktree`. | Relative paths inside (`src/main.rs`, `./a/../b.txt`), absolute paths inside the worktree, and any `read` of any path. |
| `no-secrets-in-commands` | `bash`: a command that contains a secret, that is, text the redaction would replace: a PEM private key block, `sk-ant-` followed by 20 or more of `A-Za-z0-9_-`, `sk-` (or `sk-proj-`) followed by 20 or more of those, `ghp_`, `gho_`, `ghu_`, `ghs_` or `ghr_` followed by 30 or more letters and digits, `github_pat_` followed by 40 or more of `A-Za-z0-9_`, `AKIA` followed by 16 capitals and digits. Reason: `a command must not contain a secret; read it from the environment instead`. | `curl -H "x-api-key: $ANTHROPIC_API_KEY" ...` (a variable, not the key). A `write` whose content holds a key is not refused; the content of results is redacted instead. |

A refusal's reason is `<name>: <reason>`, for example `no-force-push: git push --force rewrites history on the remote`.

What the patterns do not see: file writes made by a shell (`echo x > /etc/passwd`, `sed -i`), `cd` into another
directory before a relative `rm`, and commands in scripts the model writes first and runs later. The prose rules and the
teacher are the answer to those.

## Prose rules and the teacher

The guard parses the `rules` text it is given into rules, and sends the teacher model a request through the router: a
fixed system prompt and one user message holding the rules (as `## name` sections), then `Tool`, `Input` (the call's
input as JSON), `Worktree`, `Card title` and `Card body`. The request is made with a token issued for the run being
checked, so the teacher's tokens are counted in the run's usage and against the card's budget. The teacher must answer
with JSON only:

```json
{"decision": "allow" | "refuse", "rule": "<name of the rule broken, or empty>", "reason": "<one sentence>"}
```

A reply in a ```` ```json ```` fence is read too. Then:

| Reply | Result |
|---|---|
| `allow` | The call is allowed. The reason is the teacher's, or `the teacher found no rule broken` when it gave none. |
| `refuse` naming one of the rules given | Refused with the reason `<rule>: <reason>`. |
| `refuse` naming anything else or no rule | Refused with `teacher: the answer refuses for "<rule>", which is not one of the rules`. |
| Any other `decision` | Refused with `teacher: "<decision>" is neither allow nor refuse`. |
| Not JSON | Refused with `teacher: the answer is not the JSON asked for (...)`. |
| No answer within `teacher.timeout_seconds`, an HTTP error from the router, a reply without text | Refused with `teacher: <why>`. |
| The question names no run | Refused with `teacher: the question names no run to count its tokens against`. |

The teacher is configured in the `[teacher]` table, see [configuration](configuration.md). The check endpoints a
harness calls are described in the [bridge protocol](bridge-protocol.md).
