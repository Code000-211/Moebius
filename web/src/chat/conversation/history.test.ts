import { describe, expect, it } from "vitest";
import type { RunEvent } from "../../api/types/chat";
import { clock, delta, event } from "./helpers";
import type { Entry } from "./index";
import { Conversation } from "./index";

describe("a page of history read behind the stream", () => {
  // Reading what was missed and being sent what happens now are two paths into one conversation, so the page often
  // arrives holding events older than the newest the stream already brought. Putting each in its place one at a time
  // folded the whole log again per event, which is seconds on a long chat that is still answering.
  const late = (seq: number, text: string, at: number): RunEvent => ({
    ...delta(seq, text),
    at,
  });
  const textOf = (entry?: Entry) => (entry?.kind === "text" ? entry.text : "");

  it("folds a live event and the page that follows it into one conversation", () => {
    const conversation = new Conversation();
    // The stream is ahead: 101 landed before the read of 1..100 came back.
    conversation.push(late(101, " and the end", clock + 101));
    const history: RunEvent[] = [];
    for (let seq = 1; seq <= 100; seq += 1) history.push(delta(seq, ` ${seq}`));
    conversation.backfill(history);
    const entries = conversation.entries();
    expect(entries).toHaveLength(1);
    expect(textOf(entries[0])).toMatch(/^ 1 2 3/);
    expect(textOf(entries[0]).endsWith(" 100 and the end")).toBe(true);
    expect(conversation.lastSeq).toBe(101);
  });

  it("keeps the server's time over the clock of the browser that saw the event first", () => {
    // An event off the stream has no time on it, so the browser's own clock stands in; a duration is worked out
    // between two of these, so a machine an hour ahead read a forty second thought as two hours until the stored
    // event arrived to say when it really happened.
    const conversation = new Conversation();
    conversation.push(late(2, " the answer", 1_700_007_200));
    conversation.backfill([
      event(1, "said", { message: "the question" }),
      { ...delta(2, " the answer"), at: clock + 2 },
    ]);
    const at = conversation.entries().map((each) => each.at);
    expect(at).toEqual([clock + 1, clock + 2]);
    expect(conversation.lastAt).toBe(clock + 2);
  });

  it("takes a page that is only new events without folding anything again", () => {
    const conversation = new Conversation();
    conversation.backfill([delta(1, "a"), delta(2, "b")]);
    conversation.backfill([delta(3, "c")]);
    expect(conversation.entries()).toHaveLength(1);
    expect(textOf(conversation.entries()[0])).toBe("abc");
    expect(conversation.lastSeq).toBe(3);
  });
});
