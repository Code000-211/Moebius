// The Moebius question tool for Pi-Bolt: a chat's agent puts a few questions to the person with typed options instead of
// asking in prose. Moebius loads this file with `--extension` into the Pi of a chat whose agent operates Moebius. The
// tool waits for nobody: it answers at once, and the web app draws its arguments as a card under the agent's turn (the
// `ask_user_question` tool call is the question). The person's choices come back as their next message, so a run never
// holds a tool open for a person and a chat that was paused meanwhile loses nothing.
// The arguments are those of Claude Code's `AskUserQuestion`, which the web app also understands.

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const parameters = {
	type: "object",
	properties: {
		questions: {
			type: "array",
			minItems: 1,
			maxItems: 4,
			description: "One to four questions, asked together.",
			items: {
				type: "object",
				properties: {
					question: { type: "string", description: "The whole question, ending in a question mark." },
					header: { type: "string", maxLength: 16, description: "A word or two that names the question." },
					options: {
						type: "array",
						minItems: 2,
						maxItems: 4,
						description:
							"The choices. End the label of the one you recommend with \"(Recommended)\". Do not add an Other option: the person can always write their own answer.",
						items: {
							type: "object",
							properties: {
								label: { type: "string", maxLength: 60, description: "The choice in one to five words." },
								description: { type: "string", description: "What the choice means or costs." },
							},
							required: ["label", "description"],
						},
					},
					multiSelect: { type: "boolean", description: "Whether more than one option may be chosen. Default false." },
				},
				required: ["question", "header", "options"],
			},
		},
	},
	required: ["questions"],
};

export default function (pi: ExtensionAPI) {
	pi.registerTool({
		name: "ask_user_question",
		label: "Ask the person",
		description:
			"Ask the person one to four questions with a few options each, when a choice is theirs and guessing would waste their time. " +
			"The questions appear in the chat; their answer arrives as their next message.",
		promptSnippet: "ask_user_question: put questions with options to the person",
		promptGuidelines: [
			"Use ask_user_question for a decision that is the person's, in place of a question written in your answer. Put your recommended option first, end its label with \"(Recommended)\" and say why in its description.",
			"After calling ask_user_question, write nothing more and end your turn: the person's answer is their next message.",
		],
		parameters: parameters as never,
		async execute() {
			return {
				content: [
					{
						type: "text",
						text: "The questions are shown to the person. End your turn now without answering them yourself; their answer arrives as their next message.",
					},
				],
				details: {},
			};
		},
	});
}
