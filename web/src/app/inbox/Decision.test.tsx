import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { InboxItem, Snapshot } from "../../api/types/board";
import { Decision } from "./Decision";

const snapshot: Snapshot = { boards: [], harness_changes: [], inbox: [] };
const proposal: InboxItem = {
  id: 12,
  kind: "wiki_proposal",
  card_id: null,
  title: "Wiki: Say the new rule",
  data: {
    page: "Runbooks/Refunds",
    by: "Coder (agent)",
    diff: "--- a/Runbooks/Refunds.md\n+++ b/Runbooks/Refunds.md\n-Old rule.\n+New rule.\n",
  },
  resolved_at: null,
  at: 100,
  assignees: [],
};

const calls: Array<{ method: string; url: string }> = [];

function show(role: "member" | "viewer", item: InboxItem = proposal, shot: Snapshot = snapshot) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(keys.session, {
    signed_in: true,
    person: { id: 2, handle: "lea", name: "Lea", role, disabled: false, created_at: 0 },
  });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <Decision item={item} snapshot={shot} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  calls.length = 0;
  vi.unstubAllGlobals();
});

describe("the decision about a wiki proposal", () => {
  it("shows the diff of the page and the buttons that merge and dismiss it to a member", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        calls.push({ method: init?.method ?? "GET", url: String(url) });
        return new Response(JSON.stringify({ commit: "abc" }), { status: 200 });
      }),
    );
    show("member");
    expect(screen.getByText(/New rule\./)).toBeTruthy();
    expect(screen.getByText(/Old rule\./)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await waitFor(() =>
      expect(calls.map((c) => `${c.method} ${c.url}`)).toContain(
        "POST /v1/wiki/proposals/12/merge",
      ),
    );
  });

  it("dismisses by the item's id", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        calls.push({ method: init?.method ?? "GET", url: String(url) });
        return new Response("null", { status: 200 });
      }),
    );
    show("member");
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    await waitFor(() =>
      expect(calls.map((c) => `${c.method} ${c.url}`)).toContain(
        "POST /v1/wiki/proposals/12/dismiss",
      ),
    );
  });

  it("shows a viewer the diff and no button", () => {
    show("viewer");
    expect(screen.getByText(/New rule\./)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Merge" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Dismiss" })).toBeNull();
  });
});

describe("the way to the card", () => {
  it("links an item about a card to it", () => {
    const stuck: InboxItem = { ...proposal, id: 52, kind: "stuck", card_id: 7, title: "Stuck" };
    const shot = {
      boards: [{ repository: { id: 3, home_project_id: 2 }, cards: [{ id: 7 }], changes: [] }],
      harness_changes: [],
      inbox: [stuck],
    } as unknown as Snapshot;
    show("member", stuck, shot);
    expect(screen.getByTestId("open-card").getAttribute("href")).toBe(
      "/projects/2?repository=3&card=7",
    );
  });

  it("has no card link for an item that is not about a card", () => {
    show("member");
    expect(screen.queryByTestId("open-card")).toBeNull();
  });
});
