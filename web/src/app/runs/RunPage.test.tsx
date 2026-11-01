/**
 * The tabs of the run view: the row is a tablist whose chosen tab is the only stop of the tab order, names the panel
 * it shows, and is named back by that panel, so the person hearing the page knows which view they are in and how to
 * move to another. An arrow key moves the choice and the focus with it. A run whose read failed is checked too: the
 * page says so and reads it again when asked, rather than leaving the person at a dead end.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RunPage } from "./RunPage";

// The events of a run and the event stream are not what these tests are about.
vi.mock("../../chat/useThread", () => ({
  useThread: () => ({ entries: [], lastSeq: 0, loaded: true, error: null }),
}));
vi.mock("../../live/LiveProvider", () => ({ useServerEvents: () => {} }));

const RUN = {
  id: 1000,
  card_id: null,
  agent: "Coder",
  harness: "pi-bolt",
  purpose: "code",
  status: "running",
  worktree: null,
  started_at: 1,
  ended_at: null,
  summary: null,
  repository_id: null,
  harness_repository: false,
  origin: "card",
  runner: null,
};

function show() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string) => {
      const route = (path.split("?")[0] ?? path).replace(/\/diff$/, "");
      const body =
        route === "/v1/runs/1000"
          ? RUN
          : route === "/v1/snapshot"
            ? { boards: [] }
            : { diff: "", truncated: false };
      return { ok: true, status: 200, text: async () => JSON.stringify(body) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/runs/1000"]}>
        <Routes>
          <Route path="/runs/:id" element={<RunPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("the tabs of a run", () => {
  it("binds the chosen tab to the panel it shows", async () => {
    show();
    const row = await screen.findByRole("tablist", { name: "Views of the run" });
    const conversation = within(row).getByRole("tab", { name: "Conversation" });
    const changes = within(row).getByRole("tab", { name: "Changes" });
    // Only the chosen tab is a stop of the tab order, so Tab steps out of the row.
    expect(conversation.getAttribute("tabindex")).toBe("0");
    expect(changes.getAttribute("tabindex")).toBe("-1");

    const shown = screen.getByRole("tabpanel");
    expect(shown.id).toBe(conversation.getAttribute("aria-controls"));
    expect(shown.getAttribute("aria-labelledby")).toBe(conversation.id);
  });

  it("moves the choice, the panel and the focus together on an arrow key", async () => {
    show();
    const row = await screen.findByRole("tablist", { name: "Views of the run" });
    const changes = within(row).getByRole("tab", { name: "Changes" });
    fireEvent.keyDown(row, { key: "ArrowLeft" });

    expect(changes.getAttribute("tabindex")).toBe("0");
    expect(document.activeElement).toBe(changes);
    const shown = await screen.findByRole("tabpanel");
    expect(shown.id).toBe(changes.getAttribute("aria-controls"));
    expect(shown.getAttribute("aria-labelledby")).toBe(changes.id);
  });
});

describe("a run that could not be read", () => {
  it("says what failed and reads the run again when asked", async () => {
    let failing = true;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (path: string) => {
        const route = (path.split("?")[0] ?? path).replace(/\/diff$/, "");
        if (route === "/v1/runs/1000" && failing) {
          return { ok: false, status: 503, text: async () => "the server is starting" };
        }
        const body =
          route === "/v1/runs/1000"
            ? RUN
            : route === "/v1/snapshot"
              ? { boards: [] }
              : { diff: "", truncated: false };
        return { ok: true, status: 200, text: async () => JSON.stringify(body) };
      }),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={["/runs/1000"]}>
          <Routes>
            <Route path="/runs/:id" element={<RunPage />} />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    );

    expect(await screen.findByText("Cannot read the run")).toBeTruthy();
    const retry = screen.getByRole("button", { name: "Try again" });

    failing = false;
    fireEvent.click(retry);
    await screen.findByRole("tablist", { name: "Views of the run" });
    expect(screen.queryByText("Cannot read the run")).toBeNull();
  });
});
