/**
 * The errors of the model in a run's events: the message of a provider's refusal, quota or HTTP error, the text that
 * ends a Claude Code run in error, and the sentence a gateway wraps in JSON. Part of the conversation of
 * `Conversation.ts`.
 */
import { type Json, object, string } from "./json";

/** The text of the last assistant message of an `agent_end`, which says why a Claude Code run ended in error. */
export function endText(data: Json): string {
  const messages = Array.isArray(data.messages) ? data.messages : [];
  const content = object(messages.at(-1)).content;
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .map((block) => string(object(block).text))
    .filter((text) => text !== "")
    .join("\n");
}

export const MODEL_ERROR = "The model returned an error.";

/**
 * What a model's error says, as a person reads it. A gateway wraps its own sentence in JSON — `404: {"message":"no
 * route for POST …","type":"not_found_error",…}` — and the only part worth reading is the sentence, so it comes out
 * with whatever stood in front of the braces. Anything that is not that shape is left exactly as it came.
 */
export function readableError(text: string): string {
  const start = text.indexOf("{");
  if (start === -1 || !text.trimEnd().endsWith("}")) return text;
  let body: unknown;
  try {
    body = JSON.parse(text.slice(start));
  } catch {
    return text;
  }
  const inner = object(object(body).error).message;
  const sentence = string(object(body).message) || string(inner);
  if (sentence === "") return text;
  const lead = text.slice(0, start).trim();
  return lead === "" ? sentence : `${lead} ${sentence}`;
}

/** The error of an assistant message that the provider ended with an error; null for any other message. */
export function modelErrorOf(message: Json): string | null {
  if (message.role !== "assistant" || message.stopReason !== "error") return null;
  return string(message.errorMessage) || MODEL_ERROR;
}
