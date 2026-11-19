/**
 * The shapes a chat shows: a tool call with its input, the check's decision and its execution, the questions the
 * agent asks, and the entries of the conversation. Types only; `tools.ts` reads them from events and `Conversation.ts`
 * folds events into them.
 */
/** What a tool call works on, shown as one line — a file by its name, with `inputPath` for the whole path. */
export type ToolInput =
  | { type: "unknown" }
  | { type: "read"; path: string }
  | { type: "edit"; path: string; changes: Array<{ oldText: string; newText: string }> }
  | { type: "write"; path: string; content: string }
  | { type: "command"; command: string }
  | { type: "todos"; todos: Array<{ content: string; status: string }> }
  | { type: "questions"; questions: AskedQuestion[] }
  | { type: "other"; json: string };

/** One question the agent put to the person with the question tool: a few options to choose from, and always "Other". */
export interface AskedQuestion {
  /** A word or two that names the question; empty when the agent gave none. */
  header: string;
  question: string;
  /** Whether more than one option may be chosen. */
  multiple: boolean;
  options: AskedOption[];
}

/** One option of a question. The agent marks the one it recommends by ending its label in "(Recommended)". */
export interface AskedOption {
  /** The label without the mark. */
  label: string;
  description: string;
  recommended: boolean;
}

/** The answer of the tool-call check (or of the MCP gateway). */
export interface Check {
  allowed: boolean;
  reason: string;
}

export interface McpPart {
  server: string;
  tool: string;
  check: Check;
}

export type Execution = { state: "running" } | { state: "finished"; failed: boolean };

export interface ToolCall {
  id: string;
  tool: string;
  input: ToolInput;
  check: Check | null;
  mcp: McpPart | null;
  /** Null while only the check has been seen. */
  execution: Execution | null;
  /** What the tool printed: so far while it runs, all of it when it ended. */
  output: string;
}

export type Said = "said" | "steered";

/** Where an error comes from: the model's provider (refused key, quota, an HTTP error), or Moebius, which ended the run. */
export type ErrorSource = "model" | "run";

/**
 * One thing the chat shows. `at` is when it arrived and `seq` the event that started it: a page that reads older
 * messages puts them above the ones drawn, so nothing may be named by its place in the list, or every turn would
 * change its name whenever something was added above it.
 */
export type Entry = { at: number; seq: number } & (
  | { kind: "text"; text: string }
  | { kind: "thinking"; text: string }
  | { kind: "tool"; call: ToolCall }
  | { kind: "person"; said: Said; message: string }
  | { kind: "error"; source: ErrorSource; message: string }
  | { kind: "mcpError"; server: string; message: string }
);
