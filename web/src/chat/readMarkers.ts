/**
 * Read markers: the server counts a chat unread while its `answered` is greater than its `seen`, and learns how far a
 * person has read from `POST /v1/chats/{id}/seen`. The page tells it once per answer, not at every streamed piece of
 * it (the desktop app's fix for one request per token): only when the chat has an answer the person has not marked
 * and the events held reach it.
 */
import type { Chat } from "../api/types/chat";

/**
 * The `seq` to mark `chat` read up to, given the highest `seq` the page holds; null when nothing is to be told: the
 * chat has no answer the person has not read, or the page does not hold the answer yet (a streamed piece of it is
 * not the answer).
 */
export function seqToMark(chat: Chat | undefined, lastSeq: number): number | null {
  if (!chat) return null;
  if (chat.answered <= chat.seen || lastSeq <= chat.seen || lastSeq < chat.answered) return null;
  return lastSeq;
}

/** How many chats have an answer the person has not read; the chats of the floating window never count. */
export function unreadCount(chats: Chat[]): number {
  return chats.filter((chat) => chat.origin !== "assistant" && chat.answered > chat.seen).length;
}

/** How many chats of the Chat page wait for an answer to a question their agent asked. */
export function askingCount(chats: Chat[]): number {
  return chats.filter(
    (chat) => chat.origin !== "assistant" && chat.status === "waiting" && chat.asks,
  ).length;
}

/** The chats the page lists: all but those of the floating window, unless one is open here. */
export function listedChats(chats: Chat[], open: number | null): Chat[] {
  return chats.filter((chat) => chat.origin !== "assistant" || chat.id === open);
}
