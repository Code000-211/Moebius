import { describe, expect, it } from "vitest";
import { ago, isOpen, isWorking, statusTag } from "./format";
import { pageContext } from "./pageContext";

describe("ago", () => {
  it("says it in the unit a person reads", () => {
    expect(ago(1000, 990)).toBe("just now");
    expect(ago(1000, 1000 - 300)).toBe("5 min ago");
    expect(ago(100_000, 100_000 - 7200)).toBe("2 h ago");
    expect(ago(1_000_000, 1_000_000 - 200_000)).toBe("2 d ago");
    expect(ago(1000, 2000)).toBe("just now");
  });
});

describe("statusTag", () => {
  it("has no tag for a chat that waits without a question", () => {
    expect(statusTag("waiting", false)).toBeNull();
    expect(statusTag("waiting", true)).toEqual({ label: "Asks you", tone: "amber" });
    expect(statusTag("failed", false)).toEqual({ label: "Failed", tone: "red" });
  });

  it("knows which chats take messages and which work", () => {
    expect([isOpen("waiting"), isOpen("running"), isOpen("succeeded")]).toEqual([
      true,
      true,
      false,
    ]);
    expect([isWorking("starting"), isWorking("waiting")]).toEqual([true, false]);
  });
});

describe("pageContext", () => {
  const snapshot = {
    boards: [{ repository: { id: 4, name: "demo" } }],
    harness_changes: [],
    inbox: [],
  } as never;

  it("names the page and its group", () => {
    expect(pageContext("/inbox", undefined)).toEqual({ text: "Page: Inbox", repository: null });
    expect(pageContext("/agents", undefined).text).toBe("Page: Build > Agents");
  });

  it("names the repository of a board and starts chats there", () => {
    const context = pageContext("/board/4", snapshot);
    expect(context.repository).toBe(4);
    expect(context.text).toContain("Repository: demo (id 4)");
  });

  it("names an open chat", () => {
    expect(pageContext("/chat/12", undefined).text).toBe("Page: Chat\nOpen chat: 12");
  });
});
