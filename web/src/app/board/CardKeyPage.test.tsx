// @vitest-environment jsdom
/** `/card/WEB-24` shows the card as a page, whatever the case of the key; a key no card has says so. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Board } from "../../api/types/board";
import { CardKeyPage } from "./CardKeyPage";
import { card } from "./cardPropertiesFixtures";
import { NewIssueProvider } from "./NewIssueHost";

const CARD = card(42, {
  number: 13,
  project_key: "WEB",
  project_number: 24,
  title: "Fix the login redirect",
});
const board: Board = {
  repository: {
    id: 1,
    name: "demo",
    path: "/r",
    forge: "none",
    default_branch: "main",
    home_project_id: 1,
    github: null,
    trusted: [],
  },
  cards: [CARD],
  round_limit: 5,
  waiting: {},
  changes: [],
};
const requested: string[] = [];

function open(path: string) {
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={[path]}>
        <NewIssueProvider>
          <Routes>
            <Route path="/card/:key" element={<CardKeyPage />} />
          </Routes>
        </NewIssueProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  requested.length = 0;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string) => {
      const path = String(input);
      requested.push(path);
      const byKey = path.startsWith("/v1/cards/by-key/");
      const found = path === "/v1/cards/by-key/WEB-24";
      const ok = !byKey || found;
      const body = found
        ? CARD
        : byKey
          ? { error: "no such card" }
          : path === "/v1/snapshot"
            ? { boards: [board], inbox: [] }
            : [];
      return { ok, status: ok ? 200 : 404, text: async () => JSON.stringify(body) };
    }),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the page of a card by its key", () => {
  it("shows the card as a page: no window, no scrim, and the card's actions", async () => {
    open("/card/web-24");
    expect(await screen.findByText("Fix the login redirect")).toBeTruthy();
    expect(screen.queryByRole("dialog", { name: "Card inspector" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Close the inspector" })).toBeNull();
    expect(screen.getByRole("button", { name: "More actions for #13" })).toBeTruthy();
    // A page inside a page has the way back at the left of its title.
    const back = screen.getByRole("link", { name: "Back to the board" });
    expect(back.getAttribute("href")).toBe("/board/1");
    expect(back.compareDocumentPosition(screen.getByText("Fix the login redirect"))).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
    expect(requested).toContain("/v1/cards/by-key/WEB-24");
  });

  it("says so when no card has the key", async () => {
    open("/card/WEB-99");
    expect(await screen.findByText("There is no such card")).toBeTruthy();
  });
});
