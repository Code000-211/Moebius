// A minimal Pi extension: writes one line to standard error for every tool call and every tool result of a run, and
// changes nothing. Standard output belongs to Pi's RPC channel, so an extension must never write to it. Moebius keeps
// a run's standard error in `<data_dir>/runs/<run id>/stderr.log`.
//
// Load it by naming it in an agent's `extensions` (see examples/README.md). For an extension that does more, see
// crates/moebius-core/bridge/moebius-bridge.ts.

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
