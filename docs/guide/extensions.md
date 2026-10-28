# Add a Pi extension to an agent

## When you want this

You want something to happen inside the Pi process of a run that a rule, a column script or a plugin cannot do: log
every tool call, add a tool for the model, add a command, change what the agent sees. An extension is a TypeScript
module that Pi loads. Extensions belong to a harness, here Pi-Bolt, and are not portable to another one. Everything else
about an agent carries over; the extensions do not.

Pi-Bolt documents extensions in `docs/extensions.md` in its distribution. This page covers the Moebius side: how an
extension gets into a run, how it sits next to the bridge, and one that works.

## The bridge comes last

Moebius loads one extension into every run after all of yours, the bridge
(`crates/moebius-core/bridge/moebius-bridge.ts`). It asks the server about every tool call and every tool result. It is
what makes the rules work. For each run the server writes it to `<data_dir>/runs/<id>/moebius-bridge.ts` and starts
Pi-Bolt with `--extension` for each path in the agent's `extensions`, in the order you list them, and then with
`--extension` for the bridge. It also passes `--no-extensions` and `--no-skills`, so Pi loads no extension or skill it
finds on its own: only the ones in the agent's configuration, the bridge and, for skills, Moebius's own. The [bridge protocol](../reference/bridge-protocol.md) describes what the bridge does and the rules for
a bridge of any harness.

What this guarantees, and what it asks of yours:

- Pi runs the handlers of one event in load order (Pi-Bolt's documentation). Your `tool_call` handlers run first, so
  the bridge checks the input the call will run with, even if your handler changed it, and a tool your extension adds
  goes through the same hook.
- Your `tool_result` handlers run first too, so the bridge removes the secrets from the content the result will
  carry, even those your handler added.
- A call or a result that the bridge refuses or withholds stays so; no extension of the agent has a say after it.
- An extension runs inside the Pi process with the same permissions, and sees prompts, tool calls, files and
  credentials. Load only extensions you trust.

## Build one in a chat

With a harness configuration repository, an extension can be built by an agent instead of by hand: a chat with
`harness: true` writes `extensions/<name>.ts` and names it in the `extensions` of `agents/<agent>.toml`, and what it
leaves is offered as a change that a person merges (see [Build an extension or skill in a
chat](agents.md#build-an-extension-or-skill-in-a-chat)). The steps below are for an agent in `moebius.toml`.

## The steps

1. Write the extension as a TypeScript file. It exports a default function that takes the Pi API; `import type` brings
   in the type only, so the file has nothing to install and needs no compile step (Pi runs TypeScript directly).
2. Name its path in the agent's `extensions` in `moebius.toml`. A relative path is relative to the directory of
   `moebius.toml`. Moebius does not check that the path exists.
3. Restart the server; the configuration is read once, at start.
4. Start a run with that agent. Extensions are loaded for each run.

```toml
[agents.Coder]
model = "my-local-model"
extensions = ["extensions/log-tool-calls.ts"]
```

## An example

[`examples/pi-extension/log-tool-calls.ts`](../../examples/pi-extension/log-tool-calls.ts) logs every tool call and every
tool result of a run to standard error and changes nothing. Its two handlers are registered with `pi.on(event, handler)`:
`tool_call` has the fields `toolName`, `input` and `toolCallId`, `tool_result` has `toolName`, `toolCallId`, `content`
and `isError`. Returning nothing lets the call run and keeps the result as it is.

<!-- excerpt: examples/pi-extension/log-tool-calls.ts -->
```ts
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function (pi: ExtensionAPI) {
	pi.on("tool_call", async (event) => {
		console.error(`tool_call ${event.toolName} ${event.toolCallId} ${JSON.stringify(event.input)}`);
		// Returning nothing lets the call run.
		return undefined;
	});

	pi.on("tool_result", async (event) => {
		console.error(`tool_result ${event.toolName} ${event.toolCallId} error=${event.isError}`);
		// Returning nothing keeps the result as it is.
		return undefined;
	});
}
```

To stop a call, a `tool_call` handler returns an object with `block: true` and a `reason`, which the model sees as the
tool's result. This is how the bridge does it:

<!-- excerpt: crates/moebius-core/bridge/moebius-bridge.ts -->
```ts
if (answer.decision === "allow") return undefined;
if (answer.decision === "refuse") {
	return { block: true, reason: `Moebius refused this call: ${answer.reason}` };
}
```

A rule is the better tool for forbidding a call (see [Rules](rules.md)), because it is recorded as a judgement and
applies to every harness. Pi has many more events (input, the agent's start and end, the model request, sessions) and
can register tools and commands; see Pi-Bolt's documentation for them. Standard output belongs to Pi's RPC channel with
Moebius, so an extension must never write to it. Use `console.error`.

## How to try it

Start a chat with the agent and ask it to do something that uses a tool (see [Agents](agents.md)):

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"agent": "Coder", "message": "List the files in this directory."}' $API/v1/chats
```

The answer is the run; the log is the run's standard error, `<data_dir>/runs/<id>/stderr.log`, one line per call and per
result, such as `tool_call ls <id> {}`.

## What can go wrong and how you see it

| What happens | How you see it |
|---|---|
| The extension does not load (wrong path, a syntax error, an exception while loading). | Moebius only passes the path. Look at `<data_dir>/runs/<id>/stderr.log` for what Pi says, and at the run's events. |
| Nothing is logged. | The change in `moebius.toml` has not been loaded: restart the server. Or the run is of another agent. |
| Run events of kind `unparsed` appear. | An extension wrote to standard output, which Moebius reads as JSON lines. Use `console.error`. |
| Every tool call is slow. | A handler awaits something slow, and the call waits for it. |
| A call is refused, and you do not know by whom. | The reason starts `Moebius refused this call:` when the bridge blocked it, with the rule's name. A call blocked by your own handler carries your reason. The rule refusals are `tool_check` events of the run. |

The example is not run in CI, which has no TypeScript toolchain. The guide's test checks only that the code above is a
copy of the file.
