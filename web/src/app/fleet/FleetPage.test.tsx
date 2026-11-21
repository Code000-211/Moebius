/**
 * The Fleet page when its reads fail: it says what happened and offers to try again. The empty state would say that no
 * agent is working, which is the opposite of what the person needs to know.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { Card, CardRun, InboxItem } from "../../api/types/board";
import { ToastProvider } from "../../ui/page/Toast";
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
    expect(screen.queryAllByTestId("group-row")).toHaveLength(0);
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
        <ToastProvider>
          <MemoryRouter>
            <FleetPage />
          </MemoryRouter>
        </ToastProvider>
      </QueryClientProvider>,
    );
  };

  it("offers to pause, and says who paused when they are paused", async () => {
    const posted = serve(false);
    show();
    fireEvent.click(await screen.findByRole("button", { name: "Pause all" }));
    await waitFor(() => expect(posted).toContain("/v1/runs/pause"));
    expect(await screen.findByText("All agents paused.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(posted).toContain("/v1/runs/resume"));
  });

  it("offers to resume while paused", async () => {
    const posted = serve(true);
    show();
    await screen.findByText("Paused by @ada");
    fireEvent.click(await screen.findByRole("button", { name: "Resume all" }));
    await waitFor(() => expect(posted).toContain("/v1/runs/resume"));
  });
});

function Where() {
  const at = useLocation();
  return <output data-testid="where">{`${at.pathname}${at.search}`}</output>;
}

describe("the tabs of the fleet", () => {
  let restore: () => void;
  beforeEach(() => {
    restore = mockViewport({ height: 600 });
  });
  afterEach(() => restore());

  const finished = (id: number, status: "succeeded" | "failed" | "stopped", endedAt: number) => ({
    run: {
      id,
      card_id: id,
      agent: "Coder",
      harness: "pi-bolt",
      purpose: "code",
      status,
      worktree: null,
      started_at: endedAt - 120,
      ended_at: endedAt,
      summary: null,
      repository_id: null,
      harness_repository: false,
      origin: "chat",
      runner: null,
      title: null,
    },
    card: {
      id,
      repository_id: 1,
      number: id,
      project_key: "WEB",
      project_number: id,
      title: `Finished ${id}`,
    },
    tokens: id === 1 ? 48_000 : 0,
  });

  function show(address: string) {
    const now = Math.floor(Date.now() / 1000);
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
    });
    client.setQueryData(keys.snapshot, { boards: [makeBoard([])], inbox: [], harness_changes: [] });
    client.setQueryData(keys.chats, []);
    client.setQueryData(keys.finishedRuns, [
      finished(1, "succeeded", now - 60),
      finished(2, "failed", now - 30),
      finished(3, "stopped", now - 10),
    ]);
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={[address]}>
          <FleetPage />
          <Where />
        </MemoryRouter>
      </QueryClientProvider>,
    );
  }

  it("opens on Active, and choosing Finished puts the tab in the address", async () => {
    show("/fleet");
    expect(screen.getByRole("tab", { name: "Active" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.click(screen.getByRole("tab", { name: "Finished" }));
    expect(screen.getByTestId("where").textContent).toBe("/fleet?tab=finished");
    expect(await screen.findByText("Finished 1")).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Active" }));
    expect(screen.getByTestId("where").textContent).toBe("/fleet");
  });

  it("reads the tab from the address, so a reload keeps it", async () => {
    show("/fleet?tab=finished");
    expect(screen.getByRole("tab", { name: "Finished" }).getAttribute("aria-selected")).toBe(
      "true",
    );
    expect(await screen.findByText("Finished 3")).toBeTruthy();
  });

  it("lists the finished runs newest first under Today, with outcome, id, tokens and duration", async () => {
    show("/fleet?tab=finished");
    const rows = await screen.findAllByTestId("list-row");
    expect(screen.getAllByTestId("group-row").map((row) => row.textContent)).toEqual(["Today"]);
    expect(rows.map((row) => row.textContent?.match(/Finished \d/)?.[0])).toEqual([
      "Finished 3",
      "Finished 2",
      "Finished 1",
    ]);
    expect(within(rows[0] as HTMLElement).getByTestId("outcome-stopped")).toBeTruthy();
    expect(within(rows[1] as HTMLElement).getByTestId("outcome-failed")).toBeTruthy();
    const last = rows[2] as HTMLElement;
    expect(within(last).getByTestId("outcome-succeeded")).toBeTruthy();
    expect(last.textContent).toContain("WEB-1");
    expect(last.textContent).toContain("48k tokens");
    expect(last.textContent).toContain("2m");
    // No Stop and no Approve plan on a run that is over.
    expect(screen.queryByRole("button", { name: /^Stop/ })).toBeNull();
  });

  it("has the same header on both tabs, except Pause all, which means nothing for runs that are over", async () => {
    show("/fleet");
    expect(screen.getByRole("button", { name: "Filter" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Pause all" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Finished" }));
    expect(screen.getByRole("button", { name: "Filter" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Pause all" })).toBeNull();
  });

  it("filters the finished runs by agent", async () => {
    show("/fleet?tab=finished");
    expect(await screen.findAllByTestId("list-row")).toHaveLength(3);
    cleanup();
    show("/fleet?tab=finished&agent=Nobody");
    expect(await screen.findByText("No run has finished")).toBeTruthy();
  });
});
