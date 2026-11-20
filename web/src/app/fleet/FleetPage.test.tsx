/**
 * The Fleet page when its reads fail: it says what happened and offers to try again. The empty state would say that no
 * agent is working, which is the opposite of what the person needs to know.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { Card, CardRun, InboxItem } from "../../api/types/board";
import { mockViewport } from "../../ui/page/virtualListTest";
import { card, board as makeBoard, running } from "../board/model/fixtures";
import { FleetPage } from "./FleetPage";

// The conversation of a run follows the event stream, which these tests have none of.
vi.mock("../../live/LiveProvider", () => ({
  useServerEvents: () => {},
  useWatchRun: () => {},
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
  let restore: () => void;
  beforeEach(() => {
    restore = mockViewport({ height: 600 });
  });
  afterEach(() => restore());

  const at = (cards: Card[], inbox: InboxItem[] = []) => ({
    boards: [makeBoard(cards)],
    inbox,
    harness_changes: [],
  });
  function show(snapshot: ReturnType<typeof at>) {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => ({
        ok: true,
        status: 200,
        text: async () => JSON.stringify(String(url).endsWith("/snapshot") ? snapshot : []),
      })),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
    });
    client.setQueryData(keys.snapshot, snapshot);
    client.setQueryData(keys.chats, []);
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <FleetPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    return client;
  }
  const runs = (n: number, status: CardRun["status"] = "running") =>
    Array.from({ length: n }, (_, i) =>
      card(i + 1, { column: "In progress", run: { ...running, id: i + 1, status } }),
    );

  it("draws only the rows in view of 500 runs", async () => {
    show(at(runs(500)));
    const rows = await screen.findAllByTestId("list-row");
    expect(rows.length).toBeGreaterThan(5);
    expect(rows.length).toBeLessThan(60);
  });

  it("has Stop only on the row that is hovered or selected", async () => {
    show(at(runs(3)));
    const rows = await screen.findAllByTestId("list-row");
    expect(screen.queryByRole("button", { name: /^Stop/ })).toBeNull();
    fireEvent.pointerOver(rows[1] as HTMLElement);
    expect(await screen.findByRole("button", { name: "Stop Card 2" })).toBeTruthy();
    expect(screen.getAllByRole("button", { name: /^Stop/ })).toHaveLength(1);
    fireEvent.pointerLeave(screen.getByTestId("virtual-list").parentElement as HTMLElement);
    expect(screen.queryByRole("button", { name: /^Stop/ })).toBeNull();
    // The selected row keeps it with the pointer away.
    fireEvent.click(rows[2] as HTMLElement);
    expect(await screen.findByRole("button", { name: "Stop Card 3" })).toBeTruthy();
  });

  it("puts the run that waits for you first, with its Approve plan, and no avatar", async () => {
    const waiting = card(7, { run: { ...running, id: 7, status: "waiting" } });
    const item: InboxItem = {
      id: 1,
      kind: "approve_issue",
      card_id: 7,
      title: "",
      data: {},
      resolved_at: null,
      at: 1,
      assignees: [],
    };
    show(at([...runs(3), waiting], [item]));
    const rows = await screen.findAllByTestId("list-row");
    expect(screen.getAllByTestId("group-row").map((row) => row.textContent)).toEqual([
      "Needs you",
      "Running",
    ]);
    expect(rows[0]?.textContent).toContain("Card 7");
    expect(rows[0]?.textContent).toContain("Plan ready");
    expect(
      within(rows[0] as HTMLElement).getByRole("button", { name: "Approve plan" }),
    ).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Approve plan" })).toHaveLength(1);
  });

  it("says what is empty with an icon, one line and one action", async () => {
    show(at([]));
    expect(await screen.findByText("No agent is working")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Start a chat" })).toBeTruthy();
  });
});

describe("pausing all agents from Fleet", () => {
  function serve(paused: boolean) {
    const posted: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: { method?: string }) => {
        const path = String(url).replace(/^https?:\/\/[^/]+/, "");
        if (init?.method === "POST") posted.push(path);
        const body = path.startsWith("/v1/snapshot")
          ? {
              boards: [],
              harness_changes: [],
              inbox: [],
              ...(paused ? { paused: { by: "person:ada", at: 1 } } : {}),
            }
          : path.startsWith("/v1/chats")
            ? []
            : { paused: !paused };
        return {
          ok: true,
          status: 200,
          json: async () => body,
          text: async () => JSON.stringify(body),
        };
      }),
    );
    return posted;
  }
  const show = () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <FleetPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
  };

  it("offers to pause, and says who paused when they are paused", async () => {
    const posted = serve(false);
    show();
    fireEvent.click(await screen.findByRole("button", { name: "Pause all agents" }));
    await waitFor(() => expect(posted).toContain("/v1/runs/pause"));
  });

  it("offers to resume while paused", async () => {
    const posted = serve(true);
    show();
    await screen.findByText(/All agents are paused by @ada/);
    fireEvent.click(screen.getByRole("button", { name: "Resume all agents" }));
    await waitFor(() => expect(posted).toContain("/v1/runs/resume"));
  });
});
