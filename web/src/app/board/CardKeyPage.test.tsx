// @vitest-environment jsdom
/** `/card/WEB-24` opens the card on its board, whatever the case of the key; a key no card has says so. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CardKeyPage } from "./CardKeyPage";

const CARD = { id: 42, number: 13, project_id: 7, repository_id: 3, column: "Backlog" };
const requested: string[] = [];

function Where() {
  const at = useLocation();
  return <p>{`at ${at.pathname}${at.search}`}</p>;
}

function open(path: string) {
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/card/:key" element={<CardKeyPage />} />
          <Route path="*" element={<Where />} />
        </Routes>
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
      // Only the snapshot is slow to answer: the card's page must not wait for it.
      if (path === "/v1/snapshot") return new Promise(() => {});
      const byKey = path.startsWith("/v1/cards/by-key/");
      const found = path === "/v1/cards/by-key/WEB-24";
      const ok = !byKey || found;
      return {
        ok,
        status: ok ? 200 : 404,
        text: async () => JSON.stringify(found ? CARD : byKey ? { error: "no such card" } : []),
      };
    }),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the address of a card by its key", () => {
  it("opens the card on its board", async () => {
    open("/card/web-24");
    expect(await screen.findByText("at /board/3?card=42")).toBeTruthy();
  });

  it("asks for the card by its key and starts the card's reads while the snapshot is still on its way", async () => {
    open("/card/web-24");
    await screen.findByText("at /board/3?card=42");
    await vi.waitFor(() => expect(requested).toContain("/v1/cards/42/comments"));
    expect(requested).toContain("/v1/cards/by-key/WEB-24");
    expect(requested).toContain("/v1/cards/42/history");
    expect(requested).toContain("/v1/projects/7/scripts/Backlog");
    expect(requested.filter((path) => path === "/v1/snapshot")).toHaveLength(1);
  });

  it("says so when no card has the key", async () => {
    open("/card/WEB-99");
    expect(await screen.findByText("There is no such card")).toBeTruthy();
  });
});
