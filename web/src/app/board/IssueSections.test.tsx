// @vitest-environment jsdom
/**
 * The comments of the issue in the dock, read and written. Its properties are tested in `CardProperties.test.tsx`.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Card } from "../../api/types/board";
import { CommentsSection } from "./IssueSections";

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
