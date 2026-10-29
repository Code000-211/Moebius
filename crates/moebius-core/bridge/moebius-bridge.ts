// The Moebius bridge for Pi-Bolt (spec 3.2, ADR 0007): asks the Moebius server about every tool call before it runs and
// reports every tool result before the model sees it. The harness loads this file into each run with `--extension` and
// sets MOEBIUS_CHECK_URL (the router's base URL), MOEBIUS_RUN_TOKEN (the run's token) and MOEBIUS_TOOL_OUTPUT (the
// agent's `tool_output` as JSON) for the Pi process.
// It never waits for a person: a refusal is an answer, and the run goes on. It fails closed: when the server cannot be
// asked, the call is blocked and the result is withheld.
// It also keeps the model's context small: the server cuts each result to the agent's budget, and before each model
// request the outputs of tool calls older than the last `hide_after_turns` turns are replaced by a placeholder. The
// session and the run's events keep them whole.

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

// The agent's `tool_output`, with the defaults for what is not given.
function toolOutput(): { hide_after_turns: number; max_lines: number; max_bytes: number; compress: boolean } {
	const defaults = { hide_after_turns: 10, max_lines: 200, max_bytes: 16384, compress: true };
	try {
		return { ...defaults, ...JSON.parse(process.env.MOEBIUS_TOOL_OUTPUT ?? "{}") };
	} catch {
		return defaults;
	}
}

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

// The messages with the output of each tool call older than the last `turns` turns (assistant messages) replaced by
// a placeholder naming the tool, its input and how many lines were hidden. Errors are kept, and so is an output
// shorter than its placeholder.
function hideOldOutputs(messages: any[], turns: number): any[] {
	const total = messages.filter((m) => m.role === "assistant").length;
	const inputs = new Map<string, unknown>();
	let turn = 0;
	return messages.map((message) => {
		if (message.role === "assistant") {
			turn += 1;
			for (const block of Array.isArray(message.content) ? message.content : []) {
				if (block?.type === "toolCall") inputs.set(block.id, block.arguments);
			}
			return message;
		}
		if (message.role !== "toolResult" || message.isError || total - turn < turns) return message;
		const text = (message.content ?? [])
			.filter((block: any) => block.type === "text")
			.map((block: any) => block.text)
			.join("\n");
		let input = JSON.stringify(inputs.get(message.toolCallId) ?? {});
		if (input.length > 200) input = `${input.slice(0, 200)}…`;
		const lines = text === "" ? 0 : text.replace(/\n$/, "").split("\n").length;
		const placeholder = `[${message.toolName} ${input}: output hidden: ${lines} lines]`;
		const images = (message.content ?? []).some((block: any) => block.type !== "text");
		if (!images && placeholder.length >= text.length) return message;
		return { ...message, content: [{ type: "text", text: placeholder }] };
	});
}

export default function (pi: ExtensionAPI) {
	const settings = toolOutput();

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
				input: event.input,
				content: event.content,
				is_error: event.isError,
				tool_output: settings,
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

	// Only the request changes: the session keeps every output, and a failure here leaves the context as it is.
	pi.on("context", async (event) => {
		if (!(settings.hide_after_turns > 0)) return undefined;
		try {
			return { messages: hideOldOutputs(event.messages as any[], settings.hide_after_turns) };
		} catch {
			return undefined;
		}
	});
}
