// @vitest-environment jsdom
/**
 * What a card's own editors do with the server's answer: it is in the cache when the request is done, without
 * a read of the snapshot, so the property shows the new value at once while events arrive.
 */
import { QueryClient } from "@tanstack/react-query";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { Card, Snapshot } from "../../api/types/board";
import { board, card, choose, properties, server, type } from "./cardPropertiesFixtures";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** A client holding a snapshot of one board, as the page that shows the properties has. */
function held(of: Card) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const snapshot: Snapshot = { boards: [board([of])], harness_changes: [], inbox: [] };
  client.setQueryData(keys.snapshot, snapshot);
  return client;
}
const cached = (client: QueryClient, id: number): Card | undefined =>
  client
    .getQueryData<Snapshot>(keys.snapshot)
    ?.boards.flatMap((b) => b.cards)
    .find((c) => c.id === id);

/** The requests a save sent, with the reads of the snapshot told apart: there must be none. */
const reads = () =>
  (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(
    ([url]) => url === "/v1/snapshot",
  );

describe("the cache holds the answer of a save", () => {
  it("priority, due date, estimate and claim", async () => {
    const start = card(5, { priority: "low" });
    const client = held(start);
    const sent = server({
      "PUT /v1/cards/5/priority": { status: 200, body: card(5, { priority: "urgent" }) },
      "PUT /v1/cards/5/due": {
        status: 200,
        body: card(5, { priority: "urgent", due: "2026-12-31" }),
      },
      "PUT /v1/cards/5/estimate": {
        status: 200,
        body: card(5, { priority: "urgent", due: "2026-12-31", estimate: 3 }),
      },
      "POST /v1/cards/5/claim": {
        status: 200,
        body: card(5, { priority: "urgent", due: "2026-12-31", estimate: 3, claimed_by: "person" }),
      },
    });
    properties(start, [], {}, client);
    await choose("Priority Low", "Urgent");
    await waitFor(() => expect(cached(client, 5)?.priority).toBe("urgent"));
    fireEvent.click(screen.getByRole("button", { name: "Due date No date" }));
    const field = document.querySelector('input[type="date"]') as HTMLInputElement;
    fireEvent.change(field, { target: { value: "2026-12-31" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(cached(client, 5)?.due).toBe("2026-12-31"));
    type("Estimate No estimate", { role: "textbox", name: "Estimate" }, "3");
    await waitFor(() => expect(cached(client, 5)?.estimate).toBe(3));
    await choose("Claim No one", "Me");
    await waitFor(() => expect(cached(client, 5)?.claimed_by).toBe("person"));
    expect(sent).toHaveLength(4);
    expect(reads()).toHaveLength(0);
  });

  it("close, reopen and the parent", async () => {
    const client = held(card(5));
    const closed = card(5, { closed: true, column: "Done" });
    server({
      "POST /v1/cards/5/close": { status: 200, body: closed },
      "POST /v1/cards/5/reopen": {
        status: 200,
        body: { ...closed, closed: false, column: "Backlog" },
      },
      "PUT /v1/cards/5/parent": {
        status: 200,
        body: { ...closed, closed: false, column: "Backlog", parent: 2 },
      },
    });
    properties(card(5), [], {}, client);
    await choose("State Open", "Closed");
    await waitFor(() => expect(cached(client, 5)?.closed).toBe(true));
    cleanup();
    properties(closed, [], {}, client);
    await choose("State Closed", "Open");
    await waitFor(() => expect(cached(client, 5)?.closed).toBe(false));
    type("Parent None", { role: "textbox", name: "Parent" }, "2");
    await waitFor(() => expect(cached(client, 5)?.parent).toBe(2));
    expect(reads()).toHaveLength(0);
  });

  it("a label and a blocker, which the server answers with the list of blockers", async () => {
    const client = held(card(5, { blocked_by: [3] }));
    server({
      "PUT /v1/cards/5/labels": { status: 200, body: card(5, { blocked_by: [3], labels: ["ui"] }) },
      "POST /v1/cards/5/blockers": { status: 200, body: [3, 7] },
    });
    properties(card(5, { blocked_by: [3] }), [], {}, client);
    fireEvent.click(screen.getByRole("button", { name: "Add a label" }));
    const label = screen.getByRole("textbox", { name: "Add a label" });
    fireEvent.change(label, { target: { value: "ui" } });
    fireEvent.keyDown(label, { key: "Enter" });
    await waitFor(() => expect(cached(client, 5)?.labels).toEqual(["ui"]));
    await waitFor(() =>
      expect(
        screen
          .getByRole("button", { name: "Add a blocker by its number" })
          .getAttribute("aria-disabled"),
      ).toBeNull(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Add a blocker by its number" }));
    const blocker = screen.getByRole("textbox", { name: "Add a blocker by its number" });
    fireEvent.change(blocker, { target: { value: "7" } });
    fireEvent.keyDown(blocker, { key: "Enter" });
    await waitFor(() => expect(cached(client, 5)?.blocked_by).toEqual([3, 7]));
    expect(reads()).toHaveLength(0);
  });

  it("a refused save makes the board stale", async () => {
    const client = held(card(5, { priority: "low" }));
    server({ "PUT /v1/cards/5/priority": { status: 422, body: "no" } });
    properties(card(5, { priority: "low" }), [], {}, client);
    await choose("Priority Low", "Urgent");
    // Nothing shows the snapshot here, so it is marked stale for the next read.
    await waitFor(() => expect(client.getQueryState(keys.snapshot)?.isInvalidated).toBe(true));
    expect(cached(client, 5)?.priority).toBe("low");
  });
});
