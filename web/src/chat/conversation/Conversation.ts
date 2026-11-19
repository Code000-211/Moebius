/**
 * A chat's run events folded into what the chat shows: the agent's text and thinking, its tool calls with the check's
 * decision, the person's messages and the errors, in order. It is the logic of the former desktop app's `Conversation`
 * (`widgets/conversation.rs` of the former desktop app), with the errors of the model added: a provider that refuses
 * the key, runs out of quota or answers an HTTP error ends Pi's assistant message with `stopReason: "error"`, which the
 * server passes on as a `message_end` event (a Claude Code run ends with an `agent_end` that has `is_error`).
 *
 * No React here. Events go in one by one (the stream) or as a page, in any order, and an event seen twice counts once,
 * so both ways give the same conversation. An entry that changes is replaced by a new object, so a view can skip the
 * entries that kept theirs.
 */
import type { RunEvent } from "../../api/types/chat";
import { endText, MODEL_ERROR, modelErrorOf, readableError } from "./errors";
import { type Json, object, string } from "./json";
import { resultText, toolInput } from "./tools";
import type { Entry, ToolCall } from "./types";

export class Conversation {
  private events = new Map<number, RunEvent>();
  private list: Entry[] = [];
  private calls = new Map<string, number>();
  private textOpen = false;
  private thinkingOpen = false;
  private now = 0;
  /** The `seq` of the event being folded; what an entry drawn from it is named by. */
  private seq = 0;
  private newest = 0;
  private oldest = 0;
  /** The error of the model that the turn showed already, so that `agent_end`, which repeats it, adds none. */
  private modelError: string | null = null;

  /** The highest `seq` seen; 0 when none. */
  get lastSeq(): number {
    return this.newest;
  }

  /** The lowest `seq` held; 0 when none. What a page of older events is asked to stop below. */
  get oldestSeq(): number {
    return this.oldest;
  }

  /** The time of the newest event, Unix seconds; 0 when none. */
  get lastAt(): number {
    let at = 0;
    for (const event of this.events.values()) at = Math.max(at, event.at);
    return at;
  }

  /**
   * The entries, in order. The array is new on every call. An entry is the same object as before until an event lands
   * among those already folded and the log is folded again; the entries are then new objects, equal to the old ones
   * where nothing changed, and nothing may rely on their identity.
   */
  entries(): Entry[] {
    return this.list.slice();
  }

  /**
   * Adds one event as it arrives on the stream. One seen before is ignored; one older than the newest seen sits
   * between entries already drawn, so the fold is done again and it takes its place. That is the only time a single
   * event costs more than itself, and it is rare: what arrives while a chat is open is nearly always the next one.
   */
  push(event: RunEvent): void {
    if (this.events.has(event.seq)) return;
    this.hold(event);
    if (event.seq <= this.newest) {
      this.rebuild();
      return;
    }
    this.apply(event);
    this.newest = event.seq;
  }

  /**
   * Adds the events read from the server, which come as a page of them at a time. Reading what was missed and being
   * sent what happens now are two different paths that meet in one conversation, so a page often arrives holding
   * events older than the newest one the stream already brought. Each of those is put in its place once, for the
   * whole page: folding the log again per event would cost the log again per event, which is seconds on a long chat
   * that is still answering.
   *
   * The page also carries the server's time on events the stream already brought with this browser's clock on them,
   * and a duration is worked out between two `at`s, so the stored time is the one kept.
   */
  backfill(events: readonly RunEvent[]): void {
    const fresh: RunEvent[] = [];
    let again = false;
    for (const event of events) {
      const held = this.events.get(event.seq);
      if (held) {
        if (held.at !== event.at) {
          this.events.set(event.seq, event);
          again = true;
        }
        continue;
      }
      this.hold(event);
      fresh.push(event);
      if (event.seq <= this.newest) again = true;
    }
    if (again) {
      this.rebuild();
      return;
    }
    // Nothing landed out of order and no time needed correcting, so the page is simply more of the chat: its events
    // follow the last one drawn and are applied in their own order.
    for (const event of fresh.sort((a, b) => a.seq - b.seq)) {
      this.apply(event);
      this.newest = event.seq;
    }
  }

  /** Keeps a new event, and the lowest `seq` with it, so that asking for it costs nothing. */
  private hold(event: RunEvent): void {
    this.events.set(event.seq, event);
    if (this.oldest === 0 || event.seq < this.oldest) this.oldest = event.seq;
  }

  /** Draws every event held again, in the order their `seq`s give them. */
  private rebuild(): void {
    const sorted = [...this.events.values()].sort((a, b) => a.seq - b.seq);
    this.list = [];
    this.calls = new Map();
    this.textOpen = this.thinkingOpen = false;
    this.modelError = null;
    this.newest = 0;
    for (const event of sorted) {
      this.apply(event);
      this.newest = event.seq;
    }
  }

  private addModelError(raw: string): void {
    const message = readableError(raw);
    if (this.modelError === message) return;
    this.modelError = message;
    this.add({ at: this.now, seq: this.seq, kind: "error", source: "model", message });
  }

  private add(entry: Entry): number {
    this.list.push(entry);
    return this.list.length - 1;
  }

  private close(): void {
    this.textOpen = this.thinkingOpen = false;
  }

  /** The tool call with this id, made when it is first seen; calls without an id are always new. */
  private call(id: string, tool: string): [number, ToolCall] {
    let index = id === "" ? undefined : this.calls.get(id);
    if (index === undefined) {
      index = this.add({
        at: this.now,
        seq: this.seq,
        kind: "tool",
        call: {
          id,
          tool,
          input: { type: "unknown" },
          check: null,
          mcp: null,
          execution: null,
          output: "",
        },
      });
      this.calls.set(id, index);
    }
    this.close();
    return [index, (this.list[index] as Extract<Entry, { kind: "tool" }>).call];
  }

  private setCall(index: number, call: ToolCall): void {
    const entry = this.list[index] as Extract<Entry, { kind: "tool" }>;
    this.list[index] = { ...entry, call };
  }

  /** Appends `delta` to the open entry of `kind`, or starts one. */
  private stream(kind: "text" | "thinking", delta: string): void {
    const last = this.list.at(-1);
    const open = kind === "text" ? this.textOpen : this.thinkingOpen;
    if (last && last.kind === kind && open) {
      this.list[this.list.length - 1] = { ...last, text: last.text + delta };
    } else {
      this.add({ at: this.now, seq: this.seq, kind, text: delta });
    }
    this.close();
    if (kind === "text") this.textOpen = true;
    else this.thinkingOpen = true;
  }

  private apply(event: RunEvent): void {
    this.now = event.at;
    this.seq = event.seq;
    const data = event.data;
    const id = string(data.toolCallId);
    const tool = string(data.toolName);
    switch (event.kind) {
      case "message_update": {
        const update = object(data.assistantMessageEvent);
        if (update.type === "text_delta" && typeof update.delta === "string") {
          this.stream("text", update.delta);
        }
        if (update.type === "thinking_delta" && typeof update.delta === "string") {
          this.stream("thinking", update.delta);
        }
        break;
      }
      case "message_start":
        this.close();
        break;
      case "message_end": {
        this.close();
        const error = modelErrorOf(object(data.message));
        if (error !== null) this.addModelError(error);
        break;
      }
      case "agent_start":
        this.modelError = null;
        break;
      case "agent_end": {
        this.close();
        // Pi's `agent_end` holds the messages of the turn, the failed one last; a chat's terminal records it alone.
        const messages = Array.isArray(data.messages) ? data.messages : [];
        const error = modelErrorOf(object(messages.at(-1)));
        if (error !== null) {
          this.addModelError(error);
        } else if (data.is_error === true) {
          // A Claude Code run ends with `is_error` and the text of its last message.
          this.addModelError(endText(data) || MODEL_ERROR);
        }
        break;
      }
      case "tool_execution_start": {
        const [index, call] = this.call(id, tool);
        this.setCall(index, {
          ...call,
          tool,
          input: toolInput(tool, data.args),
          execution: { state: "running" },
        });
        break;
      }
      case "tool_execution_update": {
        const [index, call] = this.call(id, tool);
        this.setCall(index, { ...call, output: resultText(data.partialResult) });
        break;
      }
      case "tool_execution_end": {
        const [index, call] = this.call(id, tool);
        this.setCall(index, {
          ...call,
          output: resultText(data.result),
          execution: { state: "finished", failed: data.isError === true },
        });
        break;
      }
      case "tool_check": {
        const [index, call] = this.call(string(data.tool_call_id), string(data.tool));
        this.setCall(index, {
          ...call,
          check: { allowed: data.decision === "allow", reason: string(data.reason) },
        });
        break;
      }
      case "mcp_call":
        this.mcpCall(data);
        break;
      case "mcp_error":
        this.add({
          at: this.now,
          seq: this.seq,
          kind: "mcpError",
          server: string(data.server),
          message: string(data.message),
        });
        this.close();
        break;
      case "steered":
      case "said":
        this.modelError = null;
        this.add({
          at: this.now,
          seq: this.seq,
          kind: "person",
          said: event.kind === "steered" ? "steered" : "said",
          message: string(data.message),
        });
        this.close();
        break;
      case "moebius_error":
        this.add({
          at: this.now,
          seq: this.seq,
          kind: "error",
          source: "run",
          message: string(data.message),
        });
        this.close();
        break;
    }
  }

  /**
   * A call through the MCP gateway. With Pi-Bolt the harness has the call under its own name, `mcp__<server>__<tool>`:
   * the gateway's part goes into that call (the one with the same id, else the latest of that name without a
   * gateway part yet) so that it is drawn once; else it is a call of its own.
   */
  private mcpCall(data: Json): void {
    const server = string(data.server);
    const tool = string(data.tool);
    const id = string(data.tool_call_id);
    const ownName = `__${server}__${tool}`;
    const isTool = (entry: Entry): entry is Extract<Entry, { kind: "tool" }> =>
      entry.kind === "tool";
    let index: number | undefined;
    const known = id === "" ? undefined : this.calls.get(id);
    if (known !== undefined) {
      const entry = this.list[known];
      if (entry && isTool(entry) && entry.call.mcp === null) index = known;
    }
    if (index === undefined) {
      for (let i = this.list.length - 1; i >= 0; i--) {
        const entry = this.list[i];
        if (
          entry &&
          isTool(entry) &&
          entry.call.mcp === null &&
          entry.call.tool.startsWith("mcp__") &&
          entry.call.tool.endsWith(ownName)
        ) {
          index = i;
          break;
        }
      }
    }
    if (index === undefined) {
      index = this.add({
        at: this.now,
        seq: this.seq,
        kind: "tool",
        call: {
          id,
          tool: `mcp__${server}__${tool}`,
          input:
            data.input === null || data.input === undefined
              ? { type: "unknown" }
              : { type: "other", json: JSON.stringify(data.input) },
          check: null,
          mcp: null,
          execution: null,
          output: "",
        },
      });
      if (id !== "") this.calls.set(id, index);
    }
    this.close();
    const call = (this.list[index] as Extract<Entry, { kind: "tool" }>).call;
    const result = resultText(data.result);
    this.setCall(index, {
      ...call,
      execution: call.execution ?? {
        state: "finished",
        failed: object(data.result).isError === true,
      },
      output: call.output === "" ? result : call.output,
      mcp: {
        server,
        tool,
        check: { allowed: data.decision === "allow", reason: string(data.reason) },
      },
    });
  }
}
