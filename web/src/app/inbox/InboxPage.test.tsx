// @vitest-environment jsdom
/**
 * The Inbox page's own rules: the tabs, a list of thousands drawn as a few rows, the one action on a row that
 * approves without opening the card, a click that opens the card window, and an item that is gone leaves the address.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockViewport } from "../../ui/page/virtualListTest";
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

const requests: string[] = [];
let restore: () => void;
beforeEach(() => {
  requests.length = 0;
  restore = mockViewport();
});

function open(
  address: string,
  items: unknown[] | number[],
  notices: unknown[],
  boards: unknown[] = [],
  proposals: unknown[] = [],
) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string, init?: RequestInit) => {
      requests.push(`${init?.method ?? "GET"} ${path}`);
      return new Response(
        JSON.stringify(
          path === "/v1/notices" || path === "/v1/people"
            ? path === "/v1/people"
              ? []
              : notices
            : path === "/v1/proposals"
              ? proposals
              : {
                  boards,
                  harness_changes: [],
                  inbox: items.map((each) => (typeof each === "number" ? item(each) : each)),
                },
        ),
      );
    }),
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
  restore();
  cleanup();
  vi.unstubAllGlobals();
});

describe("the inbox page", () => {
  it("has the tabs Needs you with its count, Mentions and Following, and the one primary button New issue", async () => {
    open("/inbox", [1, 2], [notice(1, "mentioned")]);
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: /Needs you/ }).textContent).toBe("Needs you3"),
    );
    await screen.findByRole("tab", { name: "Mentions" });
    await screen.findByRole("tab", { name: "Following" });
    expect(screen.queryByRole("tab", { name: "All" })).toBeNull();
    expect(screen.getByRole("button", { name: "New issue" })).toBeTruthy();
    expect(screen.queryByRole("columnheader")).toBeNull();
    expect(document.querySelector("kbd")).toBeNull();
    await screen.findAllByTestId("inbox-item");
    expect(screen.getByTestId("group-row").textContent).toBe("Needs you");
  });

  it("shows a followed card's change under Following and not under Mentions", async () => {
    open("/inbox?kind=following", [], [notice(1, "mentioned"), notice(2, "changed")]);
    await screen.findAllByTestId("notice");
    expect(screen.getAllByTestId("notice")).toHaveLength(1);
    expect(screen.getByTestId("notice").getAttribute("data-kind")).toBe("changed");
  });

  it("draws no unread dot", async () => {
    open("/inbox", [1], []);
    await screen.findAllByTestId("inbox-item");
    expect(screen.queryByTestId("unread-dot")).toBeNull();
  });

  it("draws a few of 2000 items, and End scrolls to the end of them", async () => {
    open(
      "/inbox",
      Array.from({ length: 2000 }, (_, at) => at + 1),
      [],
    );
    await screen.findAllByTestId("inbox-item");
    expect(screen.getAllByTestId("inbox-item").length).toBeLessThan(100);
    const list = screen.getByRole("list", { name: "Inbox" });
    list.focus();
    fireEvent.keyDown(list, { key: "End" });
    await waitFor(() => {
      const drawn = screen.getAllByTestId("inbox-item");
      // jsdom measures nothing, so the list lands near the end; `e2e/inbox.e2e.ts` reaches the very last row.
      expect(Number(drawn.at(-1)?.getAttribute("aria-posinset"))).toBeGreaterThan(1900);
    });
    expect(screen.getAllByTestId("inbox-item").length).toBeLessThan(100);
  }, 15_000);

  it("says nothing needs you, with one action", async () => {
    open("/inbox", [], []);
    await screen.findByText("Nothing needs you");
    expect(screen.getByRole("link", { name: "Open Fleet" }).getAttribute("href")).toBe("/fleet");
  });

  describe("an issue waiting for approval", () => {
    const waiting = {
      ...item(5),
      kind: "approve_issue",
      card_id: 1,
      title: "Wiki save overwrites",
      assignees: ["leon"],
    };
    const boards = [
      {
        repository: { id: 3, name: "demo" },
        cards: [{ id: 1, number: 1, title: "Wiki save overwrites", assignees: [], run: null }],
        changes: [],
      },
    ];

    it("is approved from its row without opening the card", async () => {
      open("/inbox", [waiting], [], boards);
      const row = await screen.findByTestId("inbox-item");
      expect(within(row).getByText("Plan ready")).toBeTruthy();
      fireEvent.click(within(row).getByRole("button", { name: "Approve plan" }));
      await waitFor(() => expect(requests).toContain("POST /v1/cards/1/approve"));
      expect(screen.getByTestId("where").textContent).toBe("/inbox");
    });

    it("opens its card in the window when the row is clicked", async () => {
      open("/inbox", [waiting], [], boards);
      fireEvent.click(await screen.findByTestId("inbox-item"));
      await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/inbox?card=1"));
    });
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

  describe("a Retro proposal", () => {
    const diff =
      "--- a/agents/Moebius.md\n+++ b/agents/Moebius.md\n@@ -1 +1 @@\n-old line\n+a probe with quotes goes in a file\n";
    const proposal = (data: Record<string, unknown>) => ({
      id: 7,
      kind: "proposal",
      card_id: null,
      title: "agents/Moebius.md: a probe with quotes inside quotes goes in a file",
      data,
      resolved_at: null,
      at: 1,
      assignees: [],
    });

    it("opens in a window that shows the kind and file in its bar, the reason, the diff and the actions", async () => {
      open(
        "/inbox?item=7",
        [proposal({ proposal: 3, reason: "A quoted probe broke twice.", diff })],
        [],
        [],
        [],
      );
      const window = await screen.findByTestId("editor-window");
      const bar = within(window).getByTestId("window-header");
      expect(bar.textContent).toContain("Retro proposal");
      expect(bar.textContent).toContain("agents/Moebius.md");
      expect(within(window).getByRole("heading", { name: /a probe with quotes/ })).toBeTruthy();
      expect(within(window).getByText("A quoted probe broke twice.")).toBeTruthy();
      expect(within(window).getByText(/a probe with quotes goes in a file/)).toBeTruthy();
      expect(within(window).getByRole("button", { name: "Merge" })).toBeTruthy();
      expect(within(window).getByRole("button", { name: "Dismiss" })).toBeTruthy();
      // The actions come before the assignee, and the foot has no second Close.
      const merge = within(window).getByRole("button", { name: "Merge" });
      const assignee = within(window).getByText("Assignee");
      expect(
        merge.compareDocumentPosition(assignee) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      expect(within(window).getAllByRole("button", { name: "Close" })).toHaveLength(1);
    });

    it("shows what the proposal offers for the item a trial ends with, which names only its proposal", async () => {
      open(
        "/inbox?item=7",
        [proposal({ proposal: 3 })],
        [],
        [],
        [
          {
            id: 3,
            retro_date: "2026-10-25",
            branch: "proposal/x",
            title: "x",
            body: "Read back from the proposal.",
            agents: ["Moebius"],
            status: "untried",
            diff,
          },
        ],
      );
      const window = await screen.findByTestId("editor-window");
      expect(await within(window).findByText("Read back from the proposal.")).toBeTruthy();
      expect(within(window).getByText(/a probe with quotes goes in a file/)).toBeTruthy();
      expect(within(window).getByRole("button", { name: "Merge" })).toBeTruthy();
      expect(within(window).getByRole("button", { name: "Dismiss" })).toBeTruthy();
      expect(within(window).getByRole("link", { name: "Open as page" }).getAttribute("href")).toBe(
        "/retro?day=2026-10-25&proposal=3",
      );
    });
  });
});
