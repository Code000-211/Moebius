// @vitest-environment jsdom
/**
 * The cycles are rows on one time axis: each bar spans its own dates on the axis of all of them, the cycle under way is
 * the one marked as current, and nothing in the list is a raised card.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CycleView } from "../../api/types/cycles";
import type { Project } from "../../api/types/projects";
import { CyclesTab, Timeline } from "./CyclesTab";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const cycle = (id: number, starts_on: string, ends_on: string, closed_at: number | null = null) =>
  ({
    id,
    project_id: 1,
    name: `Cycle ${id}`,
    starts_on,
    ends_on,
    closed_at,
    created_at: 0,
    cards: 2,
    done: 1,
    points: 8,
    points_done: 4,
  }) satisfies CycleView;

const list = [cycle(1, "2026-11-01", "2026-11-10", 5), cycle(2, "2026-11-11", "2026-11-20")];

describe("the timeline of cycles", () => {
  it("spans each bar over its dates on the shared axis", () => {
    render(<Timeline cycles={list} now="2026-11-15" onOpen={vi.fn()} />);
    const bar = (name: string) =>
      within(screen.getByText(name).closest("li") as HTMLElement).getByRole("progressbar");
    expect(bar("Cycle 1").style.left).toBe("0%");
    expect(bar("Cycle 1").style.width).toBe("50%");
    expect(bar("Cycle 2").style.left).toBe("50%");
    expect(bar("Cycle 2").style.width).toBe("50%");
    expect(bar("Cycle 1").getAttribute("aria-valuenow")).toBe("50");
  });

  it("marks the current cycle and today on it, and no other", () => {
    render(<Timeline cycles={list} now="2026-11-15" onOpen={vi.fn()} />);
    const current = screen.getAllByTestId("cycle").filter((li) => li.hasAttribute("aria-current"));
    expect(current).toHaveLength(1);
    expect(within(current[0] as HTMLElement).getByText(/Current/)).toBeTruthy();
    expect(screen.getAllByTestId("time-bar-today").length).toBeGreaterThan(0);
    expect(document.querySelector(".shadow-card")).toBeNull();
  });

  it("opens a cycle on a click", () => {
    const onOpen = vi.fn();
    render(<Timeline cycles={list} now="2026-11-15" onOpen={onOpen} />);
    within(screen.getByText("Cycle 1").closest("li") as HTMLElement)
      .getByRole("button")
      .click();
    expect(onOpen).toHaveBeenCalledWith(1);
  });
});

const project = (id: number): Project =>
  ({
    id,
    name: `P${id}`,
    key: `P${id}`,
    created_at: 0,
    private: false,
    repositories: [{ id, name: "core", home: true }],
    members: [],
    open_cards: 0,
    files: null,
    kinds: [],
    intake: false,
    member: true,
  }) as Project;

describe("the Cycles tab", () => {
  it("lists the cycles of its own project only", async () => {
    // The server answers the cycles of the project in the address; the tab asks for no other.
    const asked: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        asked.push(url);
        const body =
          url === "/v1/projects/1/cycles"
            ? [{ ...cycle(1, "2026-11-01", "2026-11-10"), name: "Sprint of one" }]
            : url === "/v1/projects/2/cycles"
              ? [{ ...cycle(2, "2026-11-01", "2026-11-10"), project_id: 2, name: "Sprint of two" }]
              : [];
        return { ok: true, status: 200, text: async () => JSON.stringify(body) };
      }),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={["/projects/2/cycles"]}>
          <CyclesTab project={project(2)} cycleId={null} onTerminal={() => {}} />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    await screen.findByText("Sprint of two");
    expect(screen.queryByText("Sprint of one")).toBeNull();
    expect(asked).not.toContain("/v1/projects/1/cycles");
  });
});
