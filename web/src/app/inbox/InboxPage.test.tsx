// @vitest-environment jsdom
/**
 * The Inbox page's own rules: its headline counts what the sidebar's badge counts, and an item that is gone leaves
 * the address.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { InboxPage } from "./InboxPage";

const item = (id: number) => ({
  id,
  kind: "stuck",
  card_id: null,
  title: `Item ${id}`,
  data: {},
  resolved_at: null,
  at: 1,
  assignees: [],
});
const notice = (id: number, kind: string, read = false) => ({
  id,
  kind,
  by: null,
  card_id: null,
  inbox_item_id: null,
  title: `Notice ${id}`,
  body: "",
  at: 1,
  read,
});

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.pathname + location.search}</p>;
}

function open(address: string, items: number[], notices: unknown[]) {
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async (path: string) =>
        new Response(
          JSON.stringify(
            path === "/v1/notices"
              ? notices
              : { boards: [], harness_changes: [], inbox: items.map(item) },
          ),
        ),
    ),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={[address]}>
        <Where />
        <Routes>
          <Route path="/inbox" element={<InboxPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the inbox page", () => {
  it("counts items and the unread notices that wait for the person, not those of following", async () => {
    open(
      "/inbox",
      [1, 2],
      [notice(1, "mentioned"), notice(2, "changed"), notice(3, "assigned", true)],
    );
    await screen.findByRole("tab", { name: "All 2" });
    await screen.findByRole("tab", { name: "Mentions 1" });
    await screen.findByRole("tab", { name: "Assigned 1" });
    await screen.findByRole("tab", { name: "Following 1" });
  });

  it("drops an item that is not there from the address", async () => {
    open("/inbox?item=52", [1], []);
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/inbox"));
  });

  it("keeps the address of an item that is there", async () => {
    open("/inbox?item=1", [1], []);
    await screen.findByTestId("inbox-decision");
    expect(screen.getByTestId("where").textContent).toBe("/inbox?item=1");
  });
});
