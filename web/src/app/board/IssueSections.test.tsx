// @vitest-environment jsdom
/**
 * The comments of the issue in the dock, read and written. Its properties are tested in `CardProperties.test.tsx`.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Card } from "../../api/types/board";
import { ActivitySection } from "./IssueSections";

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
  assignees: [],
  priority: "none",
  due: null,
  ...extra,
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
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the activity of an issue", () => {
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
    inClient(<ActivitySection card={card(5)} />);
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
    inClient(<ActivitySection card={card(5)} />);
    expect(await screen.findByText("Nothing has happened yet.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Comment" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(sent.every(([method]) => method === "GET")).toBe(true);
  });
});

describe("the timeline of an issue", () => {
  const entry = (at: number, kind: string, data: Record<string, unknown>) => ({ at, kind, data });
  const made = {
    "GET /v1/cards/5/history": {
      status: 200,
      body: [
        entry(1, "moved", { from: "Not started", to: "In progress", note: null }),
        entry(2, "inbox_added", { item: 1, kind: "approve_issue", title: "Approve the issue" }),
        entry(3, "inbox_resolved", { item: 1, kind: "approve_issue", title: "Approve the issue" }),
        entry(4, "run_started", { run: 9, agent: "Coder", purpose: "code" }),
        entry(6, "decided", {
          judgement: 1,
          kind: "tool_call",
          choice: "deny",
          decider: "no-force-push",
          reason: "git push --force",
          outcome: null,
        }),
      ],
    },
    "GET /v1/cards/5/comments": {
      status: 200,
      body: [{ id: 1, card_id: 5, author: "tom", body: "Happens on iOS too.", at: 5 }],
    },
  };

  it("shows comments, runs and refusals in order, with the small changes folded", async () => {
    server(made);
    inClient(<ActivitySection card={card(5)} />);
    const fold = await screen.findByRole("button", { name: /3 changes/ });
    const order = [
      fold,
      screen.getByText(/Run 9 started/),
      screen.getByText("Happens on iOS too."),
      screen.getByText(/no-force-push refused a tool call/),
    ];
    for (let i = 1; i < order.length; i++) {
      const before = order[i - 1] as HTMLElement;
      const after = order[i] as HTMLElement;
      expect(before.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
  });

  it("opens a fold to the changes it holds", async () => {
    server(made);
    inClient(<ActivitySection card={card(5)} />);
    const fold = await screen.findByRole("button", { name: /3 changes/ });
    expect(fold.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("Moved from Not started to In progress")).toBeNull();
    fireEvent.click(fold);
    expect(fold.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Moved from Not started to In progress")).toBeTruthy();
    expect(screen.getByText("Asked you: Approve the issue")).toBeTruthy();
    expect(screen.getByText("Answered: Approve the issue")).toBeTruthy();
  });

  it("links a run to its page", async () => {
    server(made);
    inClient(<ActivitySection card={card(5)} />);
    const link = await screen.findByRole("link", { name: "View run 9" });
    expect(link.getAttribute("href")).toBe("/runs/9");
  });
});
