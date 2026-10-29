# Training export

Every decision Moebius or a person makes is recorded as a judgement with its full context (ADR 0010). The training
export writes them out for fine-tuning one decision model. Judgements leave the server only this way, and only when a
person starts it: `moebius export` (see [configuration](configuration.md#the-commands)) or `POST /v1/exports` (see the
[desktop API](desktop-api.md)). The code is in `crates/moebius-core/src/improve/export.rs`.

## Files

An export is a directory with one file per kind of judgement, `<kind>.jsonl`, such as `tool_call.jsonl`. A kind with no
judgements has no file. An export of a kind that has none is refused. Kinds are written in alphabetical order. A file is
written under the name `<kind>.jsonl.tmp` and renamed to `<kind>.jsonl` when it is complete, so a file with the final
name is a whole one. The export made through the API also holds `export.json`, its record (`id`, `at`, `kinds`), which is
written last.

A file is JSON Lines: one JSON object per line, one line per judgement, oldest first (by `id`). The judgements are
read in pages of 1000 and never all held in memory. A judgement recorded while the export runs may or may not be in it.

## A line

```json
{"id": 41, "kind": "merge", "context": {"change": "7", "diff": "..."}, "options": ["merge"], "choice": "merge",
 "reason": "looks good", "decider": "person", "run": null, "card": 12, "outcome": "merged", "at": 1790000000,
 "split": "train"}
```

| Field | Type | Meaning |
|---|---|---|
| `id` | number | The judgement's id, unique across kinds. |
| `kind` | string | The kind of decision, such as `tool_call`, `merge`, `approve_issue`, `retry`, `export`, `lint_report`, `deep_check`. |
| `context` | object | The full context the decision was made on: an object from names to strings, read from the context's blobs (the objects of several blobs are merged). What a kind's context holds is up to its decision point. |
| `options` | array of strings | What could be chosen. |
| `choice` | string | What was chosen. |
| `reason` | string | Why. A refusal by a failing decider starts with `refused: `. |
| `decider` | string | Who decided: `person`, or a plugin as `<plugin>` or `<plugin>/<part>` (such as `guard/patterns`), or `none` when no decider answers the kind. |
| `run` | number or null | The run the decision was made in. |
| `card` | number or null | The card it was made for. |
| `outcome` | string or null | What came of it, null until known: a card that is done gives each of its judgements without an outcome the card's outcome (`merged`), except those of `localize`, whose outcome is the files the change touched (below); a replayed refusal has `replayed: <how it ended>; <what happened to the files>` (see [the replays](harness-configuration.md#the-replays)); a `lint_report` has `fixed` or `ignored` and a `deep_check` `found something new`, `nothing new` or `not run` (see [the lens decisions](decision-model.md#the-lens-decisions)). |
| `at` | number | When it was recorded, in seconds since 1970. |
| `split` | string | `train` or `held_out`, see below. |

The kind `localize` (a hint of the files a code run is likely to touch, see [the decision model](decision-model.md#localize))
has these rows: `context` holds `title`, `body`, `repository`, `card`, `files` (how many to name) and `candidates` (the
files whose text best matches the issue, one per line); `options` is empty; `choice` is a JSON list of `{"path", "reason"}`
objects, best first, at most ten, `[]` when nothing was hinted; and `outcome` is, once the card's change is known, a JSON
object `{"touched": [paths, alphabetical], "hinted": number, "recall": number or null, "precision": number or null}`, the
files the change touched with the share of them among the first ten hinted and the share of the hinted files among them
(`{"unknown": true}` when the change could not be read). The label of the row is `touched`.

Secrets are removed from a context when it is recorded. Every string of a line is checked for them again when the line
is written, so a secret in an old blob does not leave the server; they appear as `[redacted:<kind>]`.

## The split

`split` is `held_out` for a tenth of the judgements, which training leaves out and shadowing measures a decision model
on (see [the decision model](decision-model.md)). The rule involves no randomness, so a judgement is on the same side in every export and on every server: take the
`id` as an unsigned 64-bit number, multiply it by `0x9E3779B97F4A7C15`, wrapping around at 2^64, shift the product right
by 32 bits and divide by 10. The judgement is `held_out` when the remainder is 0, and `train` otherwise. In Rust,
`export::held_out(id)`.

## Counts

Each file has counts, which `moebius export` prints and `POST /v1/exports` returns, and `GET /v1/judgements/kinds`
gives for what is stored now: `count` judgements, of which `with_outcome` have an outcome, `by_person` were decided by a
person (decider `person`; the scarcest labels) and `held_out` are in the held-out tenth.
