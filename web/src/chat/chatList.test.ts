import { describe, expect, it } from "vitest";
import type { Chat } from "../api/types/chat";
import { applyChatEvent, filterChats, groupChats, listChanged, recentChat } from "./chatList";

const chat = (over: Partial<Chat> = {}): Chat => ({
  id: 7,
  agent: "Moebius",
  status: "waiting",
  repository_id: null,
  harness_repository: false,
  started_at: 0,
  ended_at: null,
  title: "t",
  answered: 2,
  seen: 2,
  changed_files: 0,
  asks: false,
  origin: "chat",
  model: null,
  thinking: null,
  ...over,
});
const runEvent = (kind: string, seq: number, data: Record<string, unknown> = {}, run = 7) => ({
  kind: "run_event",
  data: { run, seq, kind, data },
});

describe("applyChatEvent", () => {
  it("sets the chat running when the agent starts, and waiting with the answer when it settles", () => {
    const running = applyChatEvent([chat()], runEvent("agent_start", 3));
    expect(running[0]?.status).toBe("running");
    const settled = applyChatEvent(running, runEvent("agent_settled", 9, { aborted: false }));
    expect(settled[0]).toMatchObject({ status: "waiting", answered: 9 });
  });

  it("makes a chat unread by an error too, and keeps the status of an aborted turn", () => {
    const failed = applyChatEvent([chat({ status: "running" })], runEvent("moebius_error", 5));
    expect(failed[0]).toMatchObject({ status: "running", answered: 5 });
    const aborted = applyChatEvent(
      [chat({ status: "running" })],
      runEvent("agent_settled", 6, { aborted: true }),
    );
    expect(aborted[0]?.status).toBe("running");
  });

  it("does not wake an ended chat", () => {
    const ended = applyChatEvent([chat({ status: "stopped" })], runEvent("agent_start", 3));
    expect(ended[0]?.status).toBe("stopped");
  });

  it("takes the status a run ended with", () => {
    const list = applyChatEvent([chat()], {
      kind: "run_ended",
      data: { run: 7, status: "failed" },
    });
    expect(list[0]?.status).toBe("failed");
  });

  it("returns the same list for a streamed piece, another run or another event", () => {
    const list = [chat()];
    expect(applyChatEvent(list, runEvent("message_update", 3))).toBe(list);
    expect(applyChatEvent(list, runEvent("agent_start", 3, {}, 99))).toBe(list);
    expect(applyChatEvent(list, { kind: "card_added", data: {} })).toBe(list);
  });
});

describe("a chat that was deleted", () => {
  it("leaves the list at once, and the others stay", () => {
    const list = [chat(), { ...chat(), id: 4 }];
    expect(
      applyChatEvent(list, { kind: "chat_deleted", data: { run: 7 } }).map((c) => c.id),
    ).toEqual([4]);
    expect(applyChatEvent(list, { kind: "chat_deleted", data: { run: 99 } })).toEqual(list);
  });

  it("makes the list read again", () => {
    expect(listChanged({ kind: "chat_deleted", data: { run: 3 } })).toBe(true);
  });
});

describe("listChanged", () => {
  it("is true for a chat started, ended or offered", () => {
    expect(listChanged({ kind: "run_started", data: {} })).toBe(true);
    expect(listChanged({ kind: "change_offered", data: {} })).toBe(true);
    expect(listChanged({ kind: "run_event", data: {} })).toBe(false);
  });

  it("is true for an answer held, so a chat started a moment before has its answer in the list", () => {
    expect(
      listChanged({ kind: "run_event", data: { run: 7, seq: 4, kind: "agent_settled" } }),
    ).toBe(true);
    expect(
      listChanged({ kind: "run_event", data: { run: 7, seq: 4, kind: "moebius_error" } }),
    ).toBe(true);
    expect(
      listChanged({ kind: "run_event", data: { run: 7, seq: 3, kind: "message_update" } }),
    ).toBe(false);
  });

  it("is true for a chat renamed, so every client shows the new title", () => {
    expect(listChanged({ kind: "chat_renamed", data: { run: 7, title: "New" } })).toBe(true);
  });
});

/**
 * Asking for Chat opens a conversation rather than an empty field, so the sidebar wants the newest chat of a list it
 * may have edited itself — an event has moved a chat's status without reordering anything. The newest is the one
 * started last, told by its own `started_at`, not by where it happens to sit.
 */
describe("recentChat", () => {
  it("takes the one started last, wherever it sits in the list", () => {
    const chats = [
      chat({ id: 3, started_at: 100 }),
      chat({ id: 9, started_at: 400 }),
      chat({ id: 5, started_at: 250 }),
    ];
    expect(recentChat(chats)?.id).toBe(9);
  });

  it("takes the newest chat wherever it was started", () => {
    // A conversation begun in the floating window is on the Chat page like any other, so it is the one to open.
    const chats = [
      chat({ id: 3, started_at: 100 }),
      chat({ id: 9, started_at: 400, origin: "assistant" }),
    ];
    expect(recentChat(chats)?.id).toBe(9);
    expect(recentChat([chat({ id: 9, origin: "assistant" })])?.id).toBe(9);
  });

  it("is null when there is nothing to open", () => {
    expect(recentChat([])).toBeNull();
  });

  it("takes the first of two that started in the same second, the order the server sends them in", () => {
    const [earlier, later] = [chat({ id: 4, started_at: 500 }), chat({ id: 6, started_at: 500 })];
    expect(recentChat([earlier, later])).toBe(earlier);
  });
});

describe("filterChats", () => {
  it("keeps the chats whose title holds the words typed, whatever their case", () => {
    const a = chat({ id: 1, title: "Fix the login flow" });
    const b = chat({ id: 2, title: "Write the tests" });
    expect(filterChats([a, b], "  LOGIN ")).toEqual([a]);
  });

  it("reads a chat without a title as the New chat its row names it", () => {
    const untitled = chat({ id: 1, title: "" });
    expect(filterChats([untitled], "new ch")).toEqual([untitled]);
    expect(filterChats([untitled], "work")).toEqual([]);
  });

  it("keeps every chat while the field is empty", () => {
    const chats = [chat({ id: 1 }), chat({ id: 2 })];
    expect(filterChats(chats, "")).toBe(chats);
  });

  it("combines with the project filter: a chat passes only when both take it", () => {
    const mine = chat({ id: 1, title: "Fix the login flow", repository_id: 1 });
    const other = chat({ id: 2, title: "Fix the notes feed", repository_id: 2 });
    const project = (c: Chat): boolean => c.repository_id === 1;
    // The search never broadens the project filter, and the project filter never keeps a chat the search drops.
    expect(filterChats([other], "fix the", project)).toEqual([]);
    expect(filterChats([mine, other], "notes", project)).toEqual([]);
    expect(filterChats([mine, other], "login", project)).toEqual([mine]);
  });
});

describe("groupChats", () => {
  // A day is 86,400 seconds; the days here are the last three the helper can name.
  const now = 4 * 86_400 + 12_000;
  it("puts each chat under its own day, in the fixed order Today, Yesterday, Older", () => {
    const chats = [
      chat({ id: 1, started_at: now - 60 }), // Today
      chat({ id: 2, started_at: now - 86_400 }), // Yesterday
      chat({ id: 3, started_at: now - 2 * 86_400 - 1 }), // Older
    ];
    expect(groupChats(chats, now)).toEqual([
      { label: "Today", chats: [chats[0]] },
      { label: "Yesterday", chats: [chats[1]] },
      { label: "Older", chats: [chats[2]] },
    ]);
  });

  it("drops a day with no chat", () => {
    const chats = [chat({ id: 1, started_at: now - 60 }), chat({ id: 2, started_at: 100 })];
    expect(groupChats(chats, now)).toEqual([
      { label: "Today", chats: [chats[0]] },
      { label: "Older", chats: [chats[1]] },
    ]);
  });

  it("puts the newest of each group first, whatever order the list arrives in", () => {
    const later = chat({ id: 5, started_at: now - 10 });
    const earlier = chat({ id: 6, started_at: now - 300 });
    expect(groupChats([earlier, later], now)[0]?.chats.map((c) => c.id)).toEqual([5, 6]);
  });

  it("keeps a chat of just before midnight of the day before yesterday in Older", () => {
    const old = chat({ id: 7, started_at: 3 * 86_400 - 1 });
    expect(groupChats([old], now)[0]?.label).toBe("Older");
  });
});
