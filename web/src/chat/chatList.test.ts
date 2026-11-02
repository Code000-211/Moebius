import { describe, expect, it } from "vitest";
import type { Chat } from "../api/types/chat";
import { applyChatEvent, listChanged, recentChat } from "./chatList";

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

describe("listChanged", () => {
  it("is true for a chat started, ended or offered", () => {
    expect(listChanged({ kind: "run_started", data: {} })).toBe(true);
    expect(listChanged({ kind: "change_offered", data: {} })).toBe(true);
    expect(listChanged({ kind: "run_event", data: {} })).toBe(false);
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

  it("never takes a chat of the floating assistant, even when it is the newest", () => {
    const chats = [
      chat({ id: 3, started_at: 100 }),
      chat({ id: 9, started_at: 400, origin: "assistant" }),
    ];
    expect(recentChat(chats)?.id).toBe(3);
    expect(recentChat([chat({ id: 9, origin: "assistant" })])).toBeNull();
  });

  it("is null when there is nothing to open", () => {
    expect(recentChat([])).toBeNull();
  });

  it("takes the first of two that started in the same second, the order the server sends them in", () => {
    const [earlier, later] = [chat({ id: 4, started_at: 500 }), chat({ id: 6, started_at: 500 })];
    expect(recentChat([earlier, later])).toBe(earlier);
  });
});
