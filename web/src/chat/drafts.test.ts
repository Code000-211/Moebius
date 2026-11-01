import { describe, expect, it } from "vitest";
import { draftKey, readDraft, saveDraft } from "./drafts";

describe("drafts", () => {
  it("names the place of a draft by the chat it belongs to", () => {
    expect(draftKey(null)).toBe("new");
    expect(draftKey(7)).toBe("chat:7");
    expect(draftKey(8)).not.toBe(draftKey(7));
  });

  it("keeps what was typed until it is sent or cleared", () => {
    const key = draftKey(41);
    expect(readDraft(key)).toBe("");
    saveDraft(key, "half a sentence");
    expect(readDraft(key)).toBe("half a sentence");
    // Sending the message leaves nothing behind.
    saveDraft(key, "");
    expect(readDraft(key)).toBe("");
  });
});
