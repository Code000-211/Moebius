// @vitest-environment jsdom
/**
 * The comments of the issue in the dock, read and written. Its properties are tested in `CardProperties.test.tsx`.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createRef } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import { CardThread, ReplyField } from "./IssueSections";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const card = (id: number, extra: Partial<Card> = {}): Card => ({
  id,
  repository_id: 1,
  project_id: 1,
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
  estimate: null,
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

function inClient(
  ui: React.ReactElement,
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
) {
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>,
  );
}

/** The thread and the reply field of a card, as the card window draws them. */
function Card_({ card: c, ...rest }: { card: Card } & Partial<Parameters<typeof CardThread>[0]>) {
  return (
    <>
      <CardThread
        card={c}
        live={false}
        busy={false}
        onStop={() => {}}
        needsYou={() => null}
        {...rest}
      />
      <ReplyField card={c} fieldRef={createRef()} />
    </>
  );
}

describe("the thread of an issue", () => {
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
    inClient(<Card_ card={card(5)} />);
    expect(await screen.findByText("Why a queue?")).toBeTruthy();
    expect(screen.getByText("Because it is ordered.")).toBeTruthy();
    expect(screen.getByText("agent:Moebius")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Write a comment"), { target: { value: " Decided. " } });
    fireEvent.click(screen.getByRole("button", { name: "Comment" }));
    await waitFor(() => expect(sent.some(([method]) => method === "POST")).toBe(true));
    expect(sent.find(([method]) => method === "POST")).toEqual([
      "POST",
      "/v1/cards/5/comments",
      { body: "Decided." },
    ]);
  });

  it("sends the reply on Enter, and keeps a new line for Shift+Enter", async () => {
    const sent = server();
    inClient(<Card_ card={card(5)} />);
    const box = (await screen.findByLabelText("Write a comment")) as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: "Looks right" } });
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    expect(sent.some(([method]) => method === "POST")).toBe(false);
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(sent.some(([method]) => method === "POST")).toBe(true));
  });

  it("shows the text of the issue as the first entry, with its author", async () => {
    server();
    inClient(<Card_ card={card(5, { body: "Keep it small.", author: "lea" })} />);
    expect(await screen.findByText("Keep it small.")).toBeTruthy();
    expect(screen.getByText("lea")).toBeTruthy();
  });

  it("keeps a comment being typed and its list of people while the card's data is read again", async () => {
    const sent = server({
      "GET /v1/people": {
        status: 200,
        body: [
          { id: 1, handle: "owner", name: "Owner", role: "owner", disabled: false, created_at: 0 },
        ],
      },
    });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    inClient(<Card_ card={card(5)} />, client);
    const box = (await screen.findByRole("combobox", {
      name: "Write a comment",
    })) as HTMLTextAreaElement;
    const text = "Please look, @ow";
    fireEvent.change(box, { target: { value: text, selectionStart: text.length } });
    await screen.findByRole("option", { name: /@owner/ });
    const reads = () => sent.filter(([method]) => method === "GET").length;
    const before = reads();
    // What an event does to the card's data: everything it was read from is read again.
    await client.invalidateQueries({ queryKey: keys.comments(5) });
    await client.invalidateQueries({ queryKey: keys.history(5) });
    await client.invalidateQueries({ queryKey: keys.people });
    await waitFor(() => expect(reads()).toBeGreaterThanOrEqual(before + 3));
    expect(box.value).toBe(text);
    expect(screen.getByRole("option", { name: /@owner/ })).toBeTruthy();
    expect(screen.getByRole("combobox", { name: "Write a comment" })).toBe(box);
  });

  it("does not send an empty comment", async () => {
    const sent = server({ "GET /v1/cards/5/comments": { status: 200, body: [] } });
    inClient(<Card_ card={card(5)} />);
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
        entry(5, "run_ended", { run: 9, status: "done" }),
        entry(6, "run_started", { run: 10, agent: "Reviewer", purpose: "review" }),
        entry(7, "run_ended", { run: 10, status: "done" }),
        entry(8, "decided", {
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

  it("shows a run and a review as two collapsed lines that open to their details", async () => {
    server(made);
    inClient(<Card_ card={card(5)} />);
    const code = await screen.findByRole("button", { name: "Code run" });
    const review = screen.getByRole("button", { name: "Review" });
    expect(code.getAttribute("aria-expanded")).toBe("false");
    expect(review.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("link", { name: "View run 9" })).toBeNull();
    fireEvent.click(code);
    expect(code.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Coder · done · 1s")).toBeTruthy();
    expect(screen.getByRole("link", { name: "View run 9" }).getAttribute("href")).toBe("/runs/9");
    expect(review.getAttribute("aria-expanded")).toBe("false");
  });

  it("opens the live run to its live view and its stop, which stops it", async () => {
    server({
      "GET /v1/cards/5/history": {
        status: 200,
        body: [entry(4, "run_started", { run: 9, agent: "Coder", purpose: "code" })],
      },
    });
    const stopped: number[] = [];
    inClient(
      <Card_
        card={card(5, { run: { id: 9, agent: "Coder", purpose: "code" } as Card["run"] })}
        live
        onStop={() => stopped.push(1)}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Code run" }));
    expect(screen.getByRole("link", { name: "Open live view" }).getAttribute("href")).toBe(
      "/runs/9",
    );
    fireEvent.click(screen.getByRole("button", { name: "Stop run" }));
    expect(stopped).toEqual([1]);
  });

  it("shows moves as small lines and opens a fold to the changes it holds", async () => {
    server(made);
    inClient(<Card_ card={card(5)} />);
    expect(await screen.findByText("In progress")).toBeTruthy();
    const fold = screen.getByRole("button", { name: /2 changes/ });
    expect(fold.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("Asked you: Approve the issue")).toBeNull();
    fireEvent.click(fold);
    expect(screen.getByText("Asked you: Approve the issue")).toBeTruthy();
    expect(screen.getByText("Answered: Approve the issue")).toBeTruthy();
  });

  it("shows comments and refusals in order", async () => {
    server(made);
    inClient(<Card_ card={card(5)} />);
    const order = [
      await screen.findByText("Happens on iOS too."),
      screen.getByRole("button", { name: "Review" }),
      screen.getByText(/no-force-push refused a tool call/),
    ];
    for (let i = 1; i < order.length; i++) {
      const before = order[i - 1] as HTMLElement;
      const after = order[i] as HTMLElement;
      expect(before.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
  });

  it("ends with the needs-you block of the card that waits for a person, and has none otherwise", async () => {
    server(made);
    const waiting = {
      id: 3,
      kind: "approve_issue",
      card_id: 5,
      title: "Plan ready",
      data: {},
      resolved_at: null,
      at: 9,
      assignees: [],
    };
    const { unmount } = inClient(
      <Card_
        card={card(5)}
        item={waiting}
        needsYou={(item) => <div data-testid="needs-you-block">{item.title}</div>}
      />,
    );
    expect(await screen.findAllByTestId("needs-you-block")).toHaveLength(1);
    unmount();
    inClient(<Card_ card={card(5)} />);
    await screen.findByRole("button", { name: "Review" });
    expect(screen.queryAllByTestId("needs-you-block")).toHaveLength(0);
  });
});
