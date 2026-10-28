---
name: moebius-extensions
description: Write a Pi extension (TypeScript that runs inside the Pi process of a Moebius run) and give it to an agent. Use it when the person wants something a rule, a column script or a plugin cannot do - a new tool or command for the model, logging every tool call, changing what the agent sees - or names a Pi extension.
---

# Pi extensions

Goal: one small TypeScript file that does what was asked, loaded only by the agents that should have it, with the
Moebius bridge still last.

## Where it goes

- In a chat in the harness configuration repository (the usual way): write `extensions/<name>.ts` and add its path to
  `extensions` in `agents/<agent>.toml`. The person merges the change; the agent's next run loads it. To name it from
  outside such a chat: `"$MOEBIUS" agent show <agent> --json`, then `"$MOEBIUS" agent save <agent> ... --extension
  extensions/<name>.ts` with every setting it had (see the `moebius-configure` skill).
- A server without a harness configuration repository: the path goes in `agents.<agent>.extensions` of `moebius.toml`
  (`"$MOEBIUS" settings set`), relative to the file's directory, and needs a restart.

Moebius starts Pi with `--no-extensions`, then `--extension` for each path of the agent in order, then the bridge. The
bridge checks every tool call and result against the rules after your handlers ran, so it sees what they changed;
nothing an extension does gets past it. Never try to load an extension after it or to turn it off.

## The extension API

A file exports a default function that takes the Pi API. `import type` needs nothing installed; Pi runs TypeScript as it
is. The parts used most:

- `pi.on("tool_call", async (event) => ...)`: `event.toolName`, `event.input`, `event.toolCallId`. Return nothing to let
  the call run, or `{ block: true, reason: "..." }` to stop it (a rule is better for forbidding).
- `pi.on("tool_result", async (event) => ...)`: `toolName`, `toolCallId`, `content`, `isError`.
- `pi.registerTool({ name, label, description, parameters, execute })`: a tool the model can call; `parameters` is a
  TypeBox schema (`Type` from `@earendil-works/pi-ai`), `execute(id, params, signal, onUpdate, ctx)` returns
  `{ content: [{ type: "text", text }], details }`.
- `pi.registerCommand(name, { description, handler })`: a `/` command.
- Other events cover input, the agent's start and end, the model request and sessions.

Standard output is Pi's channel to Moebius: never write to it. Use `console.error`, which lands in the run's
`stderr.log`.

## Example

`extensions/log-tool-calls.ts`, which logs every call and result and changes nothing:

```ts
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function (pi: ExtensionAPI) {
	pi.on("tool_call", async (event) => {
		console.error(`tool_call ${event.toolName} ${event.toolCallId} ${JSON.stringify(event.input)}`);
		return undefined;
	});
	pi.on("tool_result", async (event) => {
		console.error(`tool_result ${event.toolName} ${event.toolCallId} error=${event.isError}`);
		return undefined;
	});
}
```

## Try it

After the change is merged, start a chat with the agent that uses a tool and read its events and log:

```sh
"$MOEBIUS" chat start "List the files in this directory." --agent Coder
"$MOEBIUS" api GET /v1/runs/<id>/events
```

The log is `<data_dir>/runs/<id>/stderr.log` (`data_dir` is in `"$MOEBIUS" settings`). Say what you could not try
before the merge.

## What can go wrong

- Nothing happens: the change is not merged, the agent does not name the file, or (with `moebius.toml`) the server was
  not restarted.
- Events of kind `unparsed`: the extension wrote to standard output.
- Every call is slow: a handler awaits something slow.
- A refusal that starts `Moebius refused this call:` came from the bridge and a rule, not from your handler.

## Details

In the `moebius` skill: `guide/extensions.md`, `reference/bridge-protocol.md` and
`reference/harness-configuration.md`. Pi-Bolt's own `docs/extensions.md` has every event and method.
