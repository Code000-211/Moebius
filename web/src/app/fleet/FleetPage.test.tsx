/**
 * The Fleet page when its reads fail: it says what happened and offers to try again. The empty state would say that no
 * agent is working, which is the opposite of what the person needs to know.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import { watchRenders } from "../../test-renders";
import { card, board as makeBoard, running } from "../board/model/fixtures";
import { FleetPage } from "./FleetPage";

// The conversation of a run follows the event stream, which these tests have none of.
vi.mock("../../live/LiveProvider", () => ({
  useServerEvents: () => {},
  useConnection: () => "online",
}));

afterEach(() => vi.unstubAllGlobals());

describe("a fleet that could not be read", () => {
  it("says what failed and offers to try again, instead of saying nothing is working", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: false,
        status: 503,
        text: async () => "the server is starting",
      })),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <FleetPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("the server is starting");
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
    expect(screen.queryByText("No agent is working")).toBeNull();
  });
});

describe("a fleet with many runs", () => {
  it("draws only the row of the run that changed", async () => {
    const at = (cards: Card[]) => ({ boards: [makeBoard(cards)], inbox: [] });
    const runs = Array.from({ length: 30 }, (_, at) =>
      card(at + 1, { column: "In progress", run: { ...running, id: at + 1 } }),
    );
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => ({
        ok: true,
        status: 200,
        text: async () => JSON.stringify(String(url).endsWith("/snapshot") ? at(runs) : []),
      })),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
    });
    client.setQueryData(keys.snapshot, at(runs));
    client.setQueryData(keys.chats, []);
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <FleetPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    expect(await screen.findAllByTestId("fleet-row")).toHaveLength(30);
    await act(() => new Promise((done) => setTimeout(done, 100)));

    const watch = watchRenders();
    const changed = runs.map((each) => (each.id === 5 ? { ...each, tokens: 900 } : each));
    await act(async () => {
      client.setQueryData(keys.snapshot, at(changed));
      await new Promise((done) => setTimeout(done, 100));
    });
    const seen = watch.stop();
    // A memoised row shows as two components, the memo and the row inside it.
    expect(seen.byName.Row).toBe(2);
    expect(screen.getAllByTestId("fleet-row")[4]?.textContent).toContain("900");
  });
});
