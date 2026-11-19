/**
 * Helpers the tests of the conversation share: run events built from a sequence number, and `fold`, which pushes them
 * into a new `Conversation`.
 */
import type { RunEvent } from "../../api/types/chat";
import { Conversation } from "./Conversation";

export const clock = 1_790_000_000;
export const event = (seq: number, kind: string, data: Record<string, unknown>): RunEvent => ({
  seq,
  kind,
  data,
  at: clock + seq,
});
export const delta = (seq: number, text: string) =>
  event(seq, "message_update", {
    assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: text },
  });
export const thinking = (seq: number, text: string) =>
  event(seq, "message_update", { assistantMessageEvent: { type: "thinking_delta", delta: text } });
export const start = (seq: number, id: string, tool: string, args: unknown) =>
  event(seq, "tool_execution_start", { toolCallId: id, toolName: tool, args });
export const end = (seq: number, id: string, tool: string, text: string, isError = false) =>
  event(seq, "tool_execution_end", {
    toolCallId: id,
    toolName: tool,
    isError,
    result: { content: [{ type: "text", text }] },
  });
export const check = (seq: number, id: string, tool: string, decision: string, reason: string) =>
  event(seq, "tool_check", { tool, tool_call_id: id, decision, reason });

export function fold(events: RunEvent[]): Conversation {
  const conversation = new Conversation();
  for (const each of events) conversation.push(each);
  return conversation;
}
