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
  /** Thinking says how long it took once the model moved on: `seconds`, or null while nothing after it has arrived. */
  | ({ kind: "thinking"; key: string; text: string; seconds: number | null } & DurationState)
  | { kind: "tool"; key: string; call: ToolCall }
  | { kind: "reads"; key: string; calls: ToolCall[] };

/** Whether the thinking on a line is still arriving. */
export interface DurationState {
  /** Whether the model is still thinking this one. */
  ongoing: boolean;
}

export type Block =
  | { kind: "text"; key: string; text: string }
  | { kind: "error"; key: string; source: ErrorSource; message: string }
  | { kind: "mcpError"; key: string; server: string; message: string }
  | { kind: "steps"; key: string; steps: StepView[]; working: boolean };

export interface Turn {
  key: string;
  /**
   * The `seq` of the first entry of the turn. When a page that opened mid-answer is joined by the page above it, the
   * turn at the top takes in the beginning of its answer and is named for an earlier event, but this number still
   * places it, which is what the list anchors to.
   */
  from: number;
  head: PersonEntry | null;
  blocks: Block[];
}

const isRead = (call: ToolCall) => call.input.type === "read" && !refused(call);

/**
 * The lines of a block of steps: thinking and tool calls, with runs of two or more reads made one.
 *
 * `nextAt` is the time of the entry that follows the block, or null when the block runs to the end of the
 * conversation; `working` is whether the agent works now. Together they say whether the last thinking line is still
 * arriving, and for the ones that have stopped they give the length to show: an entry keeps the time its first piece
 * arrived, so a thought runs from its own `at` to the `at` of whatever came after it.
 */
export function stepViews(
  entries: Array<{ index: number; entry: Entry }>,
  { nextAt = null, working = false }: { nextAt?: number | null; working?: boolean } = {},
): StepView[] {
  const views: StepView[] = [];
  let reads: Array<{ seq: number; call: ToolCall }> = [];
  const flush = () => {
    if (reads.length === 1) {
      const [only] = reads;
      if (only) views.push({ kind: "tool", key: `step-${only.seq}`, call: only.call });
    } else if (reads.length > 1) {
      views.push({
        kind: "reads",
        key: `step-${reads[0]?.seq}`,
        calls: reads.map((read) => read.call),
      });
    }
    reads = [];
  };
  for (let position = 0; position < entries.length; position += 1) {
    const { index, entry } = entries[position] as { index: number; entry: Entry };
    if (entry.kind === "tool" && isRead(entry.call)) {
      reads.push({ seq: entry.seq, call: entry.call });
      continue;
    }
    flush();
    if (entry.kind === "thinking") {
      const last = index === entries[entries.length - 1]?.index;
      const end = last ? nextAt : entries[position + 1]?.entry.at;
      views.push({
        kind: "thinking",
        key: `step-${entry.seq}`,
        text: entry.text,
        seconds: end === undefined || end === null ? null : Math.max(0, end - entry.at),
        ongoing: working && last && nextAt === null,
      });
    } else if (entry.kind === "tool") {
      views.push({ kind: "tool", key: `step-${entry.seq}`, call: entry.call });
    }
  }
  flush();
  return views;
}

/** Groups `entries` into turns. `working` is whether the agent works now: the last block of steps then shows it. */
export function groupTurns(entries: Entry[], working: boolean): Turn[] {
  const turns: Turn[] = [];
  let current: Turn | null = null;
  const turn = (seq: number): Turn => {
    if (!current) {
      current = { key: `turn-${seq}`, from: seq, head: null, blocks: [] };
      turns.push(current);
    }
    return current;
  };
  let index = 0;
  while (index < entries.length) {
    const entry = entries[index] as Entry;
    switch (entry.kind) {
      case "person":
        current = { key: `turn-${entry.seq}`, from: entry.seq, head: entry, blocks: [] };
        turns.push(current);
        break;
      case "text":
        turn(entry.seq).blocks.push({ kind: "text", key: `text-${entry.seq}`, text: entry.text });
        break;
      case "error":
        turn(entry.seq).blocks.push({
          kind: "error",
          key: `error-${entry.seq}`,
          source: entry.source,
          message: entry.message,
        });
        break;
      case "mcpError":
        turn(entry.seq).blocks.push({
          kind: "mcpError",
          key: `mcp-${entry.seq}`,
          server: entry.server,
          message: entry.message,
        });
        break;
      default: {
        const start = entries[index] as Entry;
        const run: Array<{ index: number; entry: Entry }> = [];
        while (index < entries.length) {
          const next = entries[index] as Entry;
          if (next.kind !== "thinking" && next.kind !== "tool") break;
          run.push({ index, entry: next });
          index += 1;
        }
        const ongoing = working && index === entries.length;
        turn(start.seq).blocks.push({
          kind: "steps",
          key: `steps-${start.seq}`,
          steps: stepViews(run, {
            nextAt: ongoing ? null : (entries[index]?.at ?? null),
            working,
          }),
          working: ongoing,
        });
        continue;
      }
    }
    index += 1;
  }
  return turns;
}
