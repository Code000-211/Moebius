/** The calls that change who a card is given to send one person at a time, so two changes made together both stay. */
import { afterEach, describe, expect, it, vi } from "vitest";
import { assignTo, changeAssignees } from "./board";

afterEach(() => vi.unstubAllGlobals());

function server() {
  const sent: Array<[string, string]> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: { method?: string }) => {
      sent.push([init?.method ?? "GET", url]);
      return { ok: true, status: 200, text: async () => "{}" };
    }),
  );
  return sent;
}

describe("the assignees of a card", () => {
  it("adds or takes off the one person named, never the whole set", async () => {
    const sent = server();
    await assignTo(5, "lea", true);
    await assignTo(5, "tom", false);
    expect(sent).toEqual([
      ["PUT", "/v1/cards/5/assignees/lea"],
      ["DELETE", "/v1/cards/5/assignees/tom"],
    ]);
  });

  it("sends only what differs between the set before and the set after", async () => {
    const sent = server();
    await changeAssignees(5, ["owner", "tom"], ["tom", "lea"]);
    expect(sent).toEqual([
      ["DELETE", "/v1/cards/5/assignees/owner"],
      ["PUT", "/v1/cards/5/assignees/lea"],
    ]);
  });
});
