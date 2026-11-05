// @vitest-environment jsdom
/**
 * The issue tracker of the dock: each control sends its own request, the labels and blockers are changed one at a time,
 * a refusal is said under the control that asked, and the comments of the issue are read and written.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Board, Card } from "../../api/types/board";
import { CommentsSection, TrackerSection } from "./IssueSections";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const card = (id: number, extra: Partial<Card> = {}): Card => ({
  id,
  repository_id: 1,
  number: id,
  title: `Issue ${id}`,
  body: "",
  author: "person",
  column: "Not started",
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
  ...extra,
});

const board = (cards: Card[]): Board => ({
  repository: {
    id: 1,
    name: "demo",
    path: "/r",
    forge: "none",
    default_branch: "main",
    github: null,
    trusted: [],
  },
  cards,
  round_limit: 5,
  waiting: {},
  changes: [],
});

/** The requests a server was sent, as method, path and body; it answers with `answer` for each path it has one for. */
function server(answers: Record<string, { status: number; body: unknown }> = {}) {
  const sent: Array<[string, string, unknown]> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: { method?: string; body?: string }) => {
      const method = init?.method ?? "GET";
      sent.push([method, url, init?.body ? JSON.parse(init.body) : undefined]);
      const answer = answers[`${method} ${url}`] ?? { status: 200, body: [] };
      return {
        ok: answer.status < 300,
        status: answer.status,
        text: async () =>
          typeof answer.body === "string" ? answer.body : JSON.stringify(answer.body),
      };
    }),
  );
  return sent;
}

function inClient(ui: React.ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

describe("where the issue stands", () => {
  const tracker = (of: Card, others: Card[] = []) => {
    const onOpen = vi.fn();
    inClient(<TrackerSection board={board([of, ...others])} card={of} onOpen={onOpen} />);
    return onOpen;
  };

  it("names the parent and the sub-issues, and opens them in the dock", () => {
    const plan = card(1);
    const step = card(2, { parent: 1 });
    const done = card(3, { parent: 1, column: "Done", closed: true });
    server();
    const onOpen = tracker(step, [plan, done]);
    fireEvent.click(screen.getByRole("button", { name: "#1 Issue 1" }));
    expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: 1 }));
    cleanup();

    const opened = tracker(plan, [step, done]);
    expect(screen.getByText("1 of 2 sub-issues done")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "#2 Issue 2" }));
    expect(opened).toHaveBeenCalledWith(expect.objectContaining({ id: 2 }));
    expect(screen.getByText("Closed")).toBeTruthy();
  });

  it("claims, releases, closes and reopens", async () => {
    const sent = server();
    tracker(card(5));
    fireEvent.click(screen.getByRole("button", { name: "Claim" }));
    fireEvent.click(screen.getByRole("button", { name: "Close issue" }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent.map(([method, url]) => `${method} ${url}`)).toEqual([
      "POST /v1/cards/5/claim",
      "POST /v1/cards/5/close",
    ]);
    cleanup();

    const again = server();
    tracker(card(5, { claimed_by: "session 1", closed: true, column: "Done" }));
    expect(screen.getByText("Claimed by session 1")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Release" }));
    fireEvent.click(screen.getByRole("button", { name: "Reopen issue" }));
    await waitFor(() => expect(again).toHaveLength(2));
    expect(again.map(([method, url]) => `${method} ${url}`)).toEqual([
      "DELETE /v1/cards/5/claim",
      "POST /v1/cards/5/reopen",
    ]);
  });

  it("says what the server refused under the control that asked", async () => {
    server({
      "POST /v1/cards/5/claim": { status: 409, body: "#5 is claimed by session 2 already" },
    });
    tracker(card(5));
    fireEvent.click(screen.getByRole("button", { name: "Claim" }));
    expect((await screen.findByRole("alert")).textContent).toContain("claimed by session 2");
  });

  it("adds and removes a blocker by its number, and a cycle is the server's to refuse", async () => {
    const sent = server();
    tracker(card(5, { blocked_by: [3] }));
    fireEvent.click(screen.getByRole("button", { name: "Remove blocker #3" }));
    // One request at a time: the controls wait for the answer.
    await waitFor(() => expect(sent).toHaveLength(1));
    await waitFor(() =>
      expect(
        (screen.getByRole("button", { name: "Remove blocker #3" }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    fireEvent.change(screen.getByLabelText("Blocked by issue number"), { target: { value: "#7" } });
    fireEvent.click(screen.getByRole("button", { name: "Add blocker" }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent).toEqual([
      ["DELETE", "/v1/cards/5/blockers/3", undefined],
      ["POST", "/v1/cards/5/blockers", { blocker: 7 }],
    ]);
    cleanup();

    server({
      "POST /v1/cards/5/blockers": {
        status: 422,
        body: "#7 already waits for #5, so it cannot block it",
      },
    });
    tracker(card(5));
    fireEvent.change(screen.getByLabelText("Blocked by issue number"), { target: { value: "7" } });
    fireEvent.click(screen.getByRole("button", { name: "Add blocker" }));
    expect((await screen.findByRole("alert")).textContent).toContain("cannot block it");
  });

  it("sets the whole list of labels with one added or one taken away", async () => {
    const sent = server();
    tracker(card(5, { labels: ["bug", "wayfinder:spike"] }));
    fireEvent.click(screen.getByRole("button", { name: "Remove label bug" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    await waitFor(() =>
      expect(
        (screen.getByRole("button", { name: "Remove label bug" }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    fireEvent.change(screen.getByLabelText("New label"), { target: { value: " ui " } });
    fireEvent.click(screen.getByRole("button", { name: "Add label" }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent).toEqual([
      ["PUT", "/v1/cards/5/labels", { labels: ["wayfinder:spike"] }],
      ["PUT", "/v1/cards/5/labels", { labels: ["bug", "wayfinder:spike", "ui"] }],
    ]);
  });
});

describe("the comments of an issue", () => {
  it("lists them oldest first and writes a new one", async () => {
    const sent = server({
      "GET /v1/cards/5/comments": {
        status: 200,
        body: [
          { id: 1, card_id: 5, author: "leon", body: "Why a queue?", at: 1 },
          { id: 2, card_id: 5, author: "agent:Moebius", body: "Because it is ordered.", at: 2 },
        ],
      },
    });
    inClient(<CommentsSection card={card(5)} />);
    expect(await screen.findByText("Why a queue?")).toBeTruthy();
    expect(screen.getByText("Because it is ordered.")).toBeTruthy();
    expect(screen.getByText(/agent:Moebius/)).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Write a comment"), { target: { value: " Decided. " } });
    fireEvent.click(screen.getByRole("button", { name: "Comment" }));
    await waitFor(() => expect(sent.some(([method]) => method === "POST")).toBe(true));
    expect(sent.find(([method]) => method === "POST")).toEqual([
      "POST",
      "/v1/cards/5/comments",
      { body: "Decided." },
    ]);
  });

  it("says there are none, and does not send an empty comment", async () => {
    const sent = server({ "GET /v1/cards/5/comments": { status: 200, body: [] } });
    inClient(<CommentsSection card={card(5)} />);
    expect(await screen.findByText("No comments yet.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Comment" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(sent.every(([method]) => method === "GET")).toBe(true);
  });
});
