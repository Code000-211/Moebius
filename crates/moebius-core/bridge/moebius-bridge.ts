// The Moebius bridge for Pi-Bolt (spec 3.2, ADR 0007): asks the Moebius server about every tool call before it runs and
// reports every tool result before the model sees it. The harness loads this file into each run with `--extension` and
// sets MOEBIUS_CHECK_URL (the router's base URL) and MOEBIUS_RUN_TOKEN (the run's token) for the Pi process.
// It never waits for a person: a refusal is an answer, and the run goes on. It fails closed: when the server cannot be
// asked, the call is blocked and the result is withheld.

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

// POSTs `body` to a Moebius check endpoint and returns the parsed answer; throws when there is none.
async function ask(path: string, body: unknown): Promise<any> {
	const url = process.env.MOEBIUS_CHECK_URL;
	const token = process.env.MOEBIUS_RUN_TOKEN;
	if (!url || !token) throw new Error("MOEBIUS_CHECK_URL or MOEBIUS_RUN_TOKEN is not set");
	const response = await fetch(`${url}/moebius/${path}`, {
		method: "POST",
		headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
		body: JSON.stringify(body),
	});
	if (!response.ok) throw new Error(`HTTP ${response.status}`);
	return await response.json();
}

export default function (pi: ExtensionAPI) {
	pi.on("tool_call", async (event) => {
		try {
			const answer = await ask("check", {
				tool: event.toolName,
				input: event.input,
				tool_call_id: event.toolCallId,
			});
			if (answer.decision === "allow") return undefined;
			if (answer.decision === "refuse") {
				return { block: true, reason: `Moebius refused this call: ${answer.reason}` };
			}
			throw new Error(`unknown decision ${JSON.stringify(answer.decision)}`);
		} catch (error) {
			return { block: true, reason: `Moebius refused this call: the check could not be reached (${error})` };
		}
	});

	// Returning only `content` also drops `structuredContent`, which may hold what was removed from the text.
	pi.on("tool_result", async (event) => {
		try {
			const answer = await ask("result", {
				tool_call_id: event.toolCallId,
				tool: event.toolName,
				content: event.content,
				is_error: event.isError,
			});
			if (!Array.isArray(answer.content)) throw new Error("no content in the answer");
			return { content: answer.content };
		} catch {
			const withheld = "Moebius withheld this result because it could not be checked for secrets.";
			return {
				content: event.content.map((block) => (block.type === "text" ? { ...block, text: withheld } : block)),
			};
		}
	});
}
