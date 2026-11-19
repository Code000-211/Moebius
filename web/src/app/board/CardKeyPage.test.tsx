// @vitest-environment jsdom
/** `/card/WEB-24` opens the card on its board, whatever the case of the key; a key no card has says so. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CardKeyPage } from "./CardKeyPage";

const BOARD = {
  repository: { id: 3, name: "Parser", forge: "none" },
  cards: [{ id: 42, number: 13, project_number: 24, project_key: "WEB", title: "Docs" }],
};

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
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({
      ok: true,
      status: 200,
      text: async () => JSON.stringify({ boards: [BOARD], harness_changes: [], inbox: [] }),
    })),
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

  it("says so when no card has the key", async () => {
    open("/card/WEB-99");
    expect(await screen.findByText("There is no such card")).toBeTruthy();
  });
});
