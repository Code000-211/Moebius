import { describe, expect, it } from "vitest";
import type { Chat } from "../api/types/chat";
import { askingCount, seqToMark, unreadCount } from "./readMarkers";

const chat = (id: number, over: Partial<Chat> = {}): Chat => ({
  id,
  agent: "Moebius",
  status: "waiting",
  repository_id: null,
  harness_repository: false,
  started_at: 0,
  ended_at: null,
  title: "t",
  answered: 0,
  seen: 0,
  changed_files: 0,
  asks: false,
  origin: "chat",
  model: null,
  thinking: null,
  ...over,
});

describe("seqToMark", () => {
  it("tells nothing for a chat without an unread answer", () => {
    expect(seqToMark(chat(1, { answered: 5, seen: 5 }), 9)).toBeNull();
    expect(seqToMark(chat(1, { answered: 0, seen: 0 }), 9)).toBeNull();
    expect(seqToMark(undefined, 9)).toBeNull();
  });

  it("tells nothing while the answer is still streaming in", () => {
    // The answer ended at event 12; the page holds up to 10 of it.
    expect(seqToMark(chat(1, { answered: 12, seen: 4 }), 10)).toBeNull();
  });

  it("marks the chat read up to the last event once the answer is held", () => {
    expect(seqToMark(chat(1, { answered: 12, seen: 4 }), 12)).toBe(12);
    expect(seqToMark(chat(1, { answered: 12, seen: 4 }), 14)).toBe(14);
  });

  it("tells once per answer: after the marker moved there is nothing to tell", () => {
    const unread = chat(1, { answered: 12, seen: 4 });
    const seq = seqToMark(unread, 12);
    expect(seq).toBe(12);
    expect(seqToMark({ ...unread, seen: seq as number }, 13)).toBeNull();
  });
});

describe("counts", () => {
  const chats = [
    chat(1, { answered: 3, seen: 1 }),
    chat(2, { answered: 3, seen: 3 }),
    chat(3, { answered: 9, seen: 0, origin: "assistant", asks: true }),
    chat(4, { asks: true }),
    chat(5, { asks: true, status: "running" }),
  ];

  it("counts every unread chat, whoever started it", () => {
    // The conversation of the floating window is a chat of this page like any other, and unread until it is read.
    expect(unreadCount(chats)).toBe(2);
  });

  it("counts the chats that wait for an answer to a question", () => {
    expect(askingCount(chats)).toBe(2);
  });
});
