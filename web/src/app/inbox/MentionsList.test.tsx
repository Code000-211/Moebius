// @vitest-environment jsdom
/**
 * `?notice=<id>` on the Mentions tab, which the desktop app's notification opens: the notice is read and its card is
 * shown, and a notice that is gone leaves the tab as it is.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Snapshot } from "../../api/types/board";
import { mockViewport } from "../../ui/page/virtualListTest";
import { MentionsList } from "./MentionsList";

const notice = {
  id: 4,
  kind: "assigned",
  by: "tom",
  card_id: 7,
  inbox_item_id: null,
  title: "Card seven",
  body: "",
  at: 1,
  read: false,
};
const snapshot = {
  boards: [{ repository: { id: 3 }, cards: [{ id: 7 }] }],
  harness_changes: [],
  inbox: [],
} as unknown as Snapshot;
let requests: string[];
let restore: () => void;

beforeEach(() => {
  restore = mockViewport();
  requests = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string, init: RequestInit) => {
      requests.push(`${init.method ?? "GET"} ${path}`);
      return new Response(
        JSON.stringify(path === "/v1/notices" ? [notice] : { ...notice, read: true }),
      );
    }),
  );
});
afterEach(() => {
  restore();
  cleanup();
  vi.unstubAllGlobals();
});

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.pathname + location.search}</p>;
}

function open(address: string) {
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={[address]}>
        <Where />
        <Routes>
          <Route path="/inbox" element={<MentionsList snapshot={snapshot} now={2} />} />
          <Route path="*" element={null} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("opening a notice from the address", () => {
  it("reads the notice and shows its card", async () => {
    open("/inbox?kind=mentions&notice=4");
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/board/3?card=7"));
    expect(requests).toContain("POST /v1/notices/4/read");
  });

  it("leaves the tab alone for a notice that is not there", async () => {
    open("/inbox?kind=mentions&notice=99");
    await waitFor(() =>
      expect(screen.getByTestId("where").textContent).toBe("/inbox?kind=mentions"),
    );
    expect(requests).not.toContain("POST /v1/notices/99/read");
  });
});

describe("the list of notices", () => {
  it("lists every kind of notice, unread until chosen, and reads one when it is chosen", async () => {
    open("/inbox?kind=mentions");
    const row = await screen.findByTestId("notice");
    expect(row.getAttribute("data-unread")).toBe("");
    expect(screen.getByTestId("unread-dot")).toBeTruthy();
    row.click();
    await waitFor(() => expect(requests).toContain("POST /v1/notices/4/read"));
  });
});
