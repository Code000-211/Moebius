// @vitest-environment jsdom
/**
 * A `#18` in a text is a link to the card with a preview card: it opens after a short wait over the link and at once
 * on focus, closes with Escape, and shows the run that is going. It works where texts are drawn: in a chat, in the
 * comments of an issue and in the log of a run. A number that names no card stays text.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Board, Card } from "../../api/types/board";
import { ActivitySection } from "../../app/board/IssueSections";
import { RunPage } from "../../app/runs/RunPage";
import { ThreadView } from "../../chat/ThreadView";
import { PREVIEW_DELAY_MS } from "./CardLinks";
import { Markdown } from "./Markdown";

const THREAD = vi.hoisted(() => ({
  entries: [] as Array<{ at: number; seq: number; kind: "text"; text: string }>,
}));
vi.mock("../../chat/useThread", () => ({
  useThread: () => ({ entries: THREAD.entries, lastSeq: 0, loaded: true, error: null }),
}));
// A virtual list draws nothing in a DOM without a size, so it draws every item here.
vi.mock("react-virtuoso", () => ({
  Virtuoso: ({
    data,
    itemContent,
  }: {
    data: unknown[];
    itemContent: (index: number, item: unknown) => React.ReactNode;
  }) => <div>{data.map((item, index) => itemContent(index, item))}</div>,
}));
vi.mock("../../live/LiveProvider", () => ({ useServerEvents: () => {}, useWatchRun: () => {} }));

const card = (number: number, extra: Partial<Card> = {}): Card => ({
  id: number + 100,
  repository_id: 1,
  project_id: 1,
  number,
  project_number: number,
  project_key: "MOE",
  title: `Issue ${number}`,
  body: "",
  author: "person",
  column: "In progress",
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
  labels: [],
  claimed_by: null,
  assignees: [],
  priority: "none",
  due: null,
  estimate: null,
  ...extra,
});

const BOARD: Board = {
  repository: {
    id: 1,
    name: "demo",
    path: "/r",
    forge: "none",
    default_branch: "main",
    github: null,
    trusted: [],
    project: "Moebius",
    home_project_id: 1,
  },
  cards: [
    card(18, { claimed_by: "agent:Coder" }),
    // Its issue is #40 on the forge, its number in the project is 5.
    card(40, { project_number: 5 }),
    card(19, { run: { id: 7, agent: "Coder", purpose: "code", status: "running" } }),
  ],
  round_limit: 5,
  waiting: {},
  changes: [],
};

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
  project_id: 1,
  harness_repository: false,
  origin: "card",
  runner: null,
};

function serve(comments: unknown[] = []) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string) => {
      const route = (path.split("?")[0] ?? path).replace(/\/diff$/, "");
      const body = route.endsWith("/history")
        ? []
        : route.endsWith("/comments")
          ? comments
          : route === "/v1/snapshot"
            ? { boards: [BOARD], harness_changes: [], inbox: [] }
            : route === "/v1/runs/1000"
              ? RUN
              : { diff: "", truncated: false };
      return { ok: true, status: 200, text: async () => JSON.stringify(body) };
    }),
  );
}

function inApp(ui: React.ReactElement, path = "/") {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/runs/:id" element={<RunPage />} />
          <Route path="*" element={ui} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  THREAD.entries = [];
  serve();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("the preview of a link to a card", () => {
  it("opens after a short wait over the link, not before", async () => {
    inApp(<Markdown content="See #18 please" />);
    const link = await screen.findByRole("link", { name: "#18" });
    vi.useFakeTimers();
    fireEvent.mouseEnter(link);
    act(() => {
      vi.advanceTimersByTime(PREVIEW_DELAY_MS - 1);
    });
    expect(screen.queryByTestId("card-preview")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    const preview = screen.getByTestId("card-preview");
    expect(preview.textContent).toContain("Moebius");
    expect(preview.textContent).toContain("In progress");
    expect(preview.textContent).toContain("Coder");
    expect(link.getAttribute("href")).toBe("/?card=118");
    fireEvent.mouseLeave(link);
    expect(screen.queryByTestId("card-preview")).toBeNull();
  });

  it("opens at once on focus and closes with Escape", async () => {
    inApp(<Markdown content="See #18" />);
    const link = await screen.findByRole("link", { name: "#18" });
    fireEvent.focus(link);
    expect(screen.getByTestId("card-preview")).toBeTruthy();
    expect(link.getAttribute("aria-describedby")).toBe(screen.getByTestId("card-preview").id);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByTestId("card-preview")).toBeNull();
  });

  it("shows the run the card has in progress, and none for a card without one", async () => {
    inApp(<Markdown content="#19 and #18" />);
    const running = await screen.findByRole("link", { name: "#19" });
    fireEvent.focus(running);
    expect(screen.getByTestId("card-preview-run").textContent).toContain("Coder is running");
    fireEvent.blur(running);
    fireEvent.focus(screen.getByRole("link", { name: "#18" }));
    expect(screen.queryByTestId("card-preview-run")).toBeNull();
  });

  it("links a card key on any board, and #5 means the card numbered 5 in the project", async () => {
    inApp(<Markdown content="See MOE-18, moe-18, UTF-8, MOE-99 and #5, not #40" />);
    const key = await screen.findByRole("link", { name: "MOE-18" });
    expect(key.getAttribute("href")).toBe("/?card=118");
    const same = screen.getByRole("link", { name: "#5" });
    expect(same.getAttribute("href")).toBe("/?card=140");
    fireEvent.focus(same);
    expect(screen.getByTestId("card-preview").textContent).toContain("MOE-5");
    expect(screen.getAllByRole("link").map((l) => l.textContent)).toEqual(["MOE-18", "#5", "#40"]);
  });

  it("does not link a key that goes on into another number", async () => {
    inApp(<Markdown content="Not MOE-18-2 here" />);
    await screen.findByText(/Not MOE-18-2/);
    await act(async () => {});
    expect(screen.queryAllByRole("link")).toHaveLength(0);
  });

  it("leaves a number that names no card as text", async () => {
    inApp(<Markdown content="Not #99, nor `#18` in code" />);
    await screen.findByText(/Not #99/);
    await act(async () => {});
    expect(screen.queryAllByRole("link")).toHaveLength(0);
  });
});

describe("where the preview is drawn", () => {
  it("works in a chat", async () => {
    inApp(
      <ThreadView
        entries={[{ at: 1, seq: 1, kind: "text", text: "Fixed in #18." }]}
        working={false}
        sent={0}
      />,
    );
    fireEvent.focus(await screen.findByRole("link", { name: "#18" }));
    expect(screen.getByTestId("card-preview")).toBeTruthy();
  });

  it("works in the comments of an issue", async () => {
    serve([{ id: 1, card_id: 118, author: "person", body: "Blocked by #19", at: 1 }]);
    inApp(<ActivitySection card={BOARD.cards[0] as Card} />);
    fireEvent.focus(await screen.findByRole("link", { name: "#19" }));
    expect(screen.getByTestId("card-preview").textContent).toContain("Issue 19");
  });

  it("works in the log of a run", async () => {
    THREAD.entries = [{ at: 1, seq: 1, kind: "text", text: "Looking at #18 now" }];
    inApp(<div />, "/runs/1000");
    fireEvent.focus(await screen.findByRole("link", { name: "#18" }));
    expect(screen.getByTestId("card-preview").textContent).toContain("Issue 18");
  });
});
