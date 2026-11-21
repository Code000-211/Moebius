// @vitest-environment jsdom
/**
 * How much of the board renders when something small happens. A board of 120 cards must not draw in full when one card
 * changes in the snapshot, when a card is opened or closed, or when a person types in the form of a new issue: only the
 * cards whose own data or choice changed draw again. The count is the components React performed work for (see
 * `test-renders.ts`), the figure the issue measured on a real board.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation, useNavigate } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { Board, Card } from "../../api/types/board";
import type { Project } from "../../api/types/projects";
import { type Renders, watchRenders } from "../../test-renders";
import { mockViewport } from "../../ui/page/virtualListTest";
import { BoardPage } from "./BoardPage";
import { NewIssueProvider } from "./NewIssueHost";

const COLUMNS = ["Backlog", "Not started", "In progress", "In review", "Ready to merge", "Done"];
const CARDS = 120;

function makeCard(id: number): Card {
  return {
    id,
    repository_id: 1,
    project_id: 1,
    number: id,
    title: `Card ${id}`,
    body: `The body of card ${id}`,
    author: "person",
    column: COLUMNS[id % COLUMNS.length] ?? "Backlog",
    created_at: 0,
    note: null,
    retried_after: null,
    rounds: 0,
    tokens: 0,
    run: null,
    blocked_by: [],
    refusals: 0,
    change: null,
    parent: null,
    counts_as_sub_issue: true,
    closed: false,
    labels: ["web"],
    claimed_by: null,
    assignees: [],
    priority: "none",
    due: null,
    estimate: null,
  };
}

function makeSnapshot(cards: Card[]) {
  const board: Board = {
    repository: {
      id: 1,
      name: "demo",
      path: "/r",
      forge: "github",
      default_branch: "main",
      home_project_id: 1,
      github: "acme/demo",
      trusted: [],
    },
    cards,
    round_limit: 5,
    waiting: {},
    changes: [],
  };
  return { boards: [board], inbox: [] };
}

const cards = Array.from({ length: CARDS }, (_, at) => makeCard(at + 1));

/** Keeps where the router is and a way to move it, for the test to read and use. */
const where = { path: "", go: (_to: string) => {} };
function Where() {
  const location = useLocation();
  where.path = location.pathname + location.search;
  where.go = useNavigate();
  return null;
}

/** The project whose board it is: the one with the repository of `makeSnapshot`. */
const PROJECT = {
  id: 1,
  repositories: [{ id: 1, name: "demo", home: true }],
  files: null,
} as unknown as Project;

let client: QueryClient;

let restore: () => void;

beforeEach(async () => {
  // The columns draw only the cards in view, and a DOM without layout has none.
  restore = mockViewport();
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => ({
      ok: true,
      status: 200,
      // The dock reads more than the snapshot; an empty list is an answer every one of its readers accepts.
      text: async () =>
        JSON.stringify(String(url).endsWith("/snapshot") ? makeSnapshot(cards) : []),
    })),
  );
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
  });
  client.setQueryData(keys.snapshot, makeSnapshot(cards));
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/projects/1"]}>
        <Where />
        <NewIssueProvider>
          <Routes>
            <Route path="projects/:projectId" element={<BoardPage project={PROJECT} />} />
          </Routes>
        </NewIssueProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  await screen.findByTestId("card-7");
  await settle();
});

afterEach(() => {
  restore();
  cleanup();
  vi.unstubAllGlobals();
});

/** Lets what a step still has coming (a read that lands, an effect that sets state, the cache's timer) come in. */
const settle = () => act(() => new Promise((done) => setTimeout(done, 100)));

/** What rendered while `work` ran and what it caused. */
async function measure(work: () => unknown): Promise<Renders> {
  const watch = watchRenders();
  await act(async () => {
    await work();
  });
  await settle();
  return watch.stop();
}

describe("the board of 120 cards", () => {
  it("draws the cards in view of each column and not all of them", () => {
    // 20 cards a column of 36px, in a column 600px high with a screenful kept around it.
    const drawn = document.querySelectorAll('[aria-roledescription="card"]').length;
    expect(drawn).toBeGreaterThan(COLUMNS.length);
    expect(drawn).toBeLessThanOrEqual(CARDS);
  });

  it("draws only the changed card when one card changes in the snapshot", async () => {
    const changed = cards.map((card) => (card.id === 7 ? { ...card, title: "Renamed" } : card));
    const seen = await measure(() => client.setQueryData(keys.snapshot, makeSnapshot(changed)));
    expect(screen.getByTestId("card-7").textContent).toContain("Renamed");
    // A memoised card shows as two components, the memo and the card inside it.
    expect(seen.byName.BoardCard).toBe(2);
    // The six columns draw again, each with the list that windows its cards (about ten components of its own).
    expect(seen.rendered).toBeLessThan(160);
  });

  it("draws only the two cards whose choice changes when another card is opened", async () => {
    const seen = await measure(() => screen.getByTestId("card-7").click());
    expect(where.path).toContain("card=7");
    // The window lies over the board and is the shell's, not the page's: the page draws only the two cards whose
    // choice changes.
    expect(seen.byName.BoardCard).toBeLessThanOrEqual(4);
    expect(seen.rendered).toBeLessThan(200);
  });

  it("draws only the two cards whose choice changes when the card is closed", async () => {
    await measure(() => screen.getByTestId("card-7").click());
    const seen = await measure(() => where.go("/projects/1"));
    expect(seen.byName.BoardCard).toBeLessThanOrEqual(4);
    expect(seen.rendered).toBeLessThan(200);
  });

  it("draws no card when a person types in the form of a new issue", async () => {
    fireEvent.keyDown(document.body, { key: "c" });
    const dialog = await screen.findByRole("dialog", { name: "New issue" });
    const title = dialog.querySelector("input");
    if (!title) throw new Error("the form has no title field");
    await settle();
    const seen = await measure(() => {
      fireEvent.change(title, { target: { value: "A new step" } });
    });
    expect(seen.byName.BoardCard).toBeUndefined();
    expect(seen.byName.BoardPage).toBeUndefined();
  });
});
