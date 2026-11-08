// The Moebius bridge for Pi-Bolt (spec 3.2, ADR 0007). It does not touch the agent's process: it blocks no tool call,
// changes no result and hides nothing, so what the model reads is what the tools returned and its prompt cache holds.
// The harness loads this file into each run with `--extension` and sets MOEBIUS_CHECK_URL (the router's base URL) and
// MOEBIUS_RUN_TOKEN (the run's token) for the Pi process.
// It offers the tool `lessons(query)`, which asks the server for the lessons of earlier cards of the run's repository
// that best match the query (`/moebius/lessons`), as many as the prompt's "Lessons from earlier cards" or more.

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
	pi.registerTool({
		name: "lessons",
		label: "Lessons",
		description:
			"Look up lessons Moebius learned from earlier cards of this repository (what went wrong or worked, and what to do). " +
			"Give the words of the task or the problem; the best matches come back.",
		parameters: {
			type: "object",
			properties: { query: { type: "string", description: "What the task or problem is about" } },
			required: ["query"],
		} as any,
		async execute(_toolCallId: string, params: { query: string }) {
			let text: string;
			try {
				const answer = await ask("lessons", { query: String(params.query ?? "") });
				text = typeof answer.text === "string" ? answer.text : "No lessons match.";
			} catch (error) {
				text = `The lessons could not be looked up (${error}).`;
			}
			return { content: [{ type: "text" as const, text }], details: {} };
		},
	});
}
