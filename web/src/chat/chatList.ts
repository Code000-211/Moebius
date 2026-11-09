/**
 * What a `run_event` or `run_ended` does to the list of chats in the cache, so that a chat's status and unread marker
 * follow the stream at once, without reading the list again (the former desktop app's `Chats::on_event`), and the one the
 * sidebar opens when there is no chat id in the address.
 */

import type { Snapshot } from "../api/types/board";
import type { Chat } from "../api/types/chat";
import type { ServerEvent } from "../api/types/events";
import { placeName } from "./places";

/** The chats after `event`; the same array when it changes none. */
export function applyChatEvent(chats: Chat[], event: ServerEvent): Chat[] {
  const data = event.data;
  const run = data.run;
  if (typeof run !== "number") return chats;
  if (event.kind === "chat_deleted") return chats.filter((chat) => chat.id !== run);
  if (event.kind === "run_ended" && typeof data.status === "string") {
    return replace(chats, run, (chat) => ({ ...chat, status: data.status as Chat["status"] }));
  }
  if (event.kind !== "run_event") return chats;
  const kind = data.kind;
  const seq = typeof data.seq === "number" ? data.seq : 0;
  const inner =
    typeof data.data === "object" && data.data !== null
      ? (data.data as Record<string, unknown>)
      : {};
  if (kind === "agent_settled" || kind === "moebius_error") {
    return replace(chats, run, (chat) => {
      const open = ["starting", "running", "waiting"].includes(chat.status);
      const status =
        kind === "agent_settled" && inner.aborted !== true && open ? "waiting" : chat.status;
      return { ...chat, answered: Math.max(chat.answered, seq), status };
    });
  }
  if (kind === "agent_start") {
    return replace(chats, run, (chat) =>
      ["starting", "running", "waiting"].includes(chat.status)
        ? { ...chat, status: "running" }
        : chat,
    );
  }
  return chats;
}

/** The project the sidebar's list of chats is narrowed to: every chat, a repository's, or those with none. */
export type ChatFilter = "all" | "none" | number;

/** The words a filter is kept under in this browser: `"all"`, `"none"`, or the repository's id. */
export function formatChatFilter(filter: ChatFilter): string {
  return typeof filter === "number" ? String(filter) : filter;
}

/**
 * The filter its saved words name, `"all"` when nothing was saved or the words name nothing: a repository may have
 * been removed since the choice was remembered, and a list that stays empty because of it reads as a broken page.
 */
export function parseChatFilter(stored: string | null): ChatFilter {
  if (stored === "none") return "none";
  const id = Number(stored);
  return stored !== null && stored !== "" && Number.isInteger(id) && id > 0 ? id : "all";
}

/**
 * The list narrowed to the project chosen. A repository keeps only its own chats; "Without project" keeps the chats
 * of the harness configuration repository and of no known place — the ones `placeName` does not name after a board.
 */
export function filterChats(
  chats: Chat[],
  snapshot: Snapshot | undefined,
  filter: ChatFilter,
): Chat[] {
  if (filter === "all") return chats;
  if (filter === "none") {
    return chats.filter((chat) => chat.harness_repository || placeName(snapshot, chat) === null);
  }
  return chats.filter((chat) => chat.repository_id === filter);
}

function replace(chats: Chat[], id: number, change: (chat: Chat) => Chat): Chat[] {
  const index = chats.findIndex((chat) => chat.id === id);
  const chat = chats[index];
  if (!chat) return chats;
  const changed = change(chat);
  if (changed === chat || (changed.status === chat.status && changed.answered === chat.answered)) {
    return chats;
  }
  const next = chats.slice();
  next[index] = changed;
  return next;
}

/** A day in seconds, so the chats are grouped by whole days the way `ago` splits its words, not by clocks. */
const DAY = 86_400;

/** The chats of one day of the list, named as the sidebar says it. */
export interface ChatGroup {
  label: string;
  chats: Chat[];
}

/**
 * Every chat falls under one of three days, in a fixed order the cache's updates cannot shake: "Today", then
 * "Yesterday", then "Older", each group's own chats newest first. The day a chat belongs to is the whole day of its
 * `started_at` against `now` (both Unix seconds), so a chat begun at 23:59 stays of the day it was begun; a clock a
 * little behind the server's still puts a chat of tomorrow under Today. A day with no chat is dropped.
 */
export function groupChats(chats: Chat[], now: number): ChatGroup[] {
  const today = Math.floor(now / DAY);
  const inGroup = (chat: Chat): number =>
    Math.min(2, Math.max(0, today - Math.floor(chat.started_at / DAY)));
  const buckets: [string, Chat[]][] = [
    ["Today", []],
    ["Yesterday", []],
    ["Older", []],
  ];
  for (const chat of chats) buckets[inGroup(chat)]?.[1].push(chat);
  return buckets
    .map(([label, chats]) => ({ label, chats: chats.sort((a, b) => b.started_at - a.started_at) }))
    .filter((group) => group.chats.length > 0);
}

/**
 * The chat to open when someone asks for Chat: the one started most recently, or null when there is none — wherever
 * it was started, since a conversation begun in the floating window is a chat of the Chat page like any other. The
 * list from the server is newest first, but the newest of an edited or partially updated cache is picked by its own
 * time rather than by where it happens to sit.
 */
export function recentChat(chats: Chat[]): Chat | null {
  let newest: Chat | null = null;
  for (const chat of chats) {
    if (newest === null || chat.started_at > newest.started_at) newest = chat;
  }
  return newest;
}

/**
 * Whether an event is one of a chat's runs worth reading the list again for: a chat started, ended, renamed or offered,
 * or an answer held. An answer is put into the list at once (see `applyChatEvent`), but a chat that was just started
 * may not be in the list yet, or a read from before the answer may still be on its way; read again, the list has it.
 */
export function listChanged(event: ServerEvent): boolean {
  if (event.kind === "run_event") {
    return event.data.kind === "agent_settled" || event.data.kind === "moebius_error";
  }
  return (
    event.kind === "run_started" ||
    event.kind === "run_ended" ||
    event.kind === "change_offered" ||
    event.kind === "change_updated" ||
    event.kind === "chat_renamed" ||
    event.kind === "chat_deleted"
  );
}
