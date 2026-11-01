/**
 * The entries of a conversation grouped for the page (the desktop app's CH3-D layout): each message of the person
 * heads a turn, and what the agent did and answered until the next one is its body. The agent's texts, errors and
 * unreachable MCP servers stand on their own; the thinking and tool calls that follow one another are one block of
 * steps, in which consecutive reads are aggregated into one line.
 */
import type { Entry, ErrorSource, ToolCall } from "./conversation";
import { refused } from "./conversation";

export type PersonEntry = Extract<Entry, { kind: "person" }>;

/** One line of a block of steps. `key` is stable while the conversation grows, for the folded state. */
export type StepView =
  | { kind: "thinking"; key: string; text: string }
  | { kind: "tool"; key: string; call: ToolCall }
  | { kind: "reads"; key: string; calls: ToolCall[] };

export type Block =
  | { kind: "text"; key: string; text: string }
  | { kind: "error"; key: string; source: ErrorSource; message: string }
  | { kind: "mcpError"; key: string; server: string; message: string }
  | { kind: "steps"; key: string; steps: StepView[]; working: boolean };

export interface Turn {
  key: string;
  head: PersonEntry | null;
  blocks: Block[];
}

const isRead = (call: ToolCall) => call.input.type === "read" && !refused(call);

/** The lines of a block of steps: thinking and tool calls, with runs of two or more reads made one. */
export function stepViews(entries: Array<{ index: number; entry: Entry }>): StepView[] {
  const views: StepView[] = [];
  let reads: Array<{ index: number; call: ToolCall }> = [];
  const flush = () => {
    if (reads.length === 1) {
      const [only] = reads;
      if (only) views.push({ kind: "tool", key: `step-${only.index}`, call: only.call });
    } else if (reads.length > 1) {
      views.push({
        kind: "reads",
        key: `step-${reads[0]?.index}`,
        calls: reads.map((read) => read.call),
      });
    }
    reads = [];
  };
  for (const { index, entry } of entries) {
    if (entry.kind === "tool" && isRead(entry.call)) {
      reads.push({ index, call: entry.call });
      continue;
    }
    flush();
    if (entry.kind === "thinking") {
      views.push({ kind: "thinking", key: `step-${index}`, text: entry.text });
    } else if (entry.kind === "tool") {
      views.push({ kind: "tool", key: `step-${index}`, call: entry.call });
    }
  }
  flush();
  return views;
}

/** Groups `entries` into turns. `working` is whether the agent works now: the last block of steps then shows it. */
export function groupTurns(entries: Entry[], working: boolean): Turn[] {
  const turns: Turn[] = [];
  let current: Turn | null = null;
  const turn = (index: number): Turn => {
    if (!current) {
      current = { key: `turn-${index}`, head: null, blocks: [] };
      turns.push(current);
    }
    return current;
  };
  let index = 0;
  while (index < entries.length) {
    const entry = entries[index] as Entry;
    switch (entry.kind) {
      case "person":
        current = { key: `turn-${index}`, head: entry, blocks: [] };
        turns.push(current);
        break;
      case "text":
        turn(index).blocks.push({ kind: "text", key: `text-${index}`, text: entry.text });
        break;
      case "error":
        turn(index).blocks.push({
          kind: "error",
          key: `error-${index}`,
          source: entry.source,
          message: entry.message,
        });
        break;
      case "mcpError":
        turn(index).blocks.push({
          kind: "mcpError",
          key: `mcp-${index}`,
          server: entry.server,
          message: entry.message,
        });
        break;
      default: {
        const start = index;
        const run: Array<{ index: number; entry: Entry }> = [];
        while (index < entries.length) {
          const next = entries[index] as Entry;
          if (next.kind !== "thinking" && next.kind !== "tool") break;
          run.push({ index, entry: next });
          index += 1;
        }
        turn(start).blocks.push({
          kind: "steps",
          key: `steps-${start}`,
          steps: stepViews(run),
          working: working && index === entries.length,
        });
        continue;
      }
    }
    index += 1;
  }
  return turns;
}
