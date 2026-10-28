# The decision model

Moebius trains one decision model on its judgements (ADR 0010, [the training export](training-export.md)). This page
says how a model that is served behind an OpenAI-compatible endpoint is put to work: it first decides alongside the
existing decider without anyone noticing (shadowing), and a person can then let it take a kind of decision over (a
takeover). Nothing here happens without a `[decision_model]` table in the [configuration](configuration.md#decision_model).
The code is in `crates/moebius-core/src/decision_model.rs`.

## Asking the model

One non-streaming request is sent to `<url>/chat/completions` with `Authorization: Bearer <key>` when `api_key_env` is
set:

```json
{"model": "moebius-decider", "stream": false, "temperature": 0,
 "messages": [{"role": "system", "content": "<the fixed prompt>"},
              {"role": "user", "content": "{\"kind\":\"merge\",\"options\":[\"merge\",\"leave\"],\"context\":{\"diff\":\"...\"}}"}]}
```

The system prompt says the model is Moebius's decision model and must answer with JSON only. The user message is a JSON
object with the `kind`, the `options` and the `context` (an object from names to strings, secrets removed). Each field
of the context is cut to its first 4000 characters, followed by ` [cut]`; the full context stays in the store, so the cut
can change without touching a judgement.

The answer is `choices[0].message.content`, and it must be exactly this JSON, bare:

```json
{"choice": "merge", "reason": "one sentence"}
```

`choice` must be one of the options; `reason` is optional; no other field is allowed. A JSON answer inside a code fence is
not accepted, nor is any other text around it, since the model is trained to answer bare. A timeout (`timeout_seconds`),
a connection error, an HTTP status other than 2xx, a reply without text, an answer that is not that JSON and a choice that
is not among the options are all failures.

## Shadowing

Whenever a judgement is recorded, by `Decisions::decide` or for a person's decision, and its kind is shadowed
(`decision_model.shadow`, all kinds when not given), and it is held out from training (`split` is `held_out`, see
[the split](training-export.md#the-split)), the model is asked the same question in the background. Judgements the model
itself made (`decider` is `decision-model` or `<plugin>/decision-model`) are not asked about. Of `tool_call`, only the
judgements the guard's teacher made (`decider` is `guard/teacher`) are asked about, because a takeover replaces only the
teacher: the calls its built-in patterns decide would make the agreement look better than it is.

- It never delays a decision or a run: the request goes to a queue of 64 that one worker thread empties, and a request
  that finds the queue full is dropped and counted (per kind, since the server started; it is not stored).
- Each answer is stored as a shadow: the judgement, the kind, the model's choice (null when it failed), its reason, whether
  it failed and when. The reason is stored with secrets removed.
- The agreement of a kind is the share of its shadows whose choice equals the recorded choice. A failed shadow counts as
  a disagreement. Only held-out judgements are measured, because the model may have trained on the others.

## Takeover proposals

After each shadow, a kind is proposed for a takeover when all of these hold:

1. A decider plugin answers the kind: it is decided through `Decisions::decide`, not only recorded when a person decides.
   Kinds that only a person decides, such as `merge` or `approve_issue`, are shadowed and measured but never proposed.
   Taking such a kind over needs the column script to ask through `moebius.decide` first.
2. It has at least `min_judgements` shadows and an agreement of at least `agreement`.
3. It has no open or merged takeover. After a takeover was dismissed or released, the kind is proposed again only once
   `min_judgements` more shadows have been measured since, and its agreement over all its shadows is again high enough.

A proposal is a takeover with the status `open` (kind, the agreement and the number of shadows it was measured on), an inbox
item of the kind `takeover` titled "The decision model can take over <kind>" and the event `takeover_proposed`.

A person decides in the app, or through the [desktop API](desktop-api.md#post-v1takeoversidmerge). Each decision is recorded
as a judgement of kind `takeover` (decider `person`; choices `merge`, `dismiss` and `release`) with the reason.

- **Merge** (`open` to `merged`): the kind is routed to the decision model from now on, and after a restart (the merged
  takeovers are read from the store at start).
- **Dismiss** (`open` to `dismissed`): nothing changes.
- **Release** (`merged` to `released`): the kind goes back to the decider it had.

## What routing means

- For a kind decided through `Decisions::decide`, the model is registered as the decider plugin `decision-model` and
  answers instead of the kind's plugin, under the plugin host's time limit. Anything that goes wrong (a failure in the
  list above, a timeout of the host) makes the decision fail closed, as for any decider: the question's fail-closed option
  is taken and the judgement's reason starts with `refused: `. The judgements it makes have the decider `decision-model`.
- For `tool_call` the guard keeps its built-in patterns, which always apply first and refuse what they refuse. Only the
  rules in prose, which the teacher judges, are answered by the model instead of the teacher, with the options `allow`
  and `refuse` and the call, the rules and the card as the context. The judgement's decider is `guard/decision-model`. A
  model that fails refuses the call, with a reason that starts with `decision model: `.
- A merged kind is no longer shadowed, because the model would be measured against itself. After a release shadowing
  starts again.

## Without a decision model

Without the table nothing is shadowed, no takeover is proposed, `GET /v1/decision-model` says `configured: false`, and
the kinds are decided as before.
