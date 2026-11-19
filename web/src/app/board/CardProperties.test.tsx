// @vitest-environment jsdom
/**
 * The card's properties in the right column: every fact the dock used to list is there, each property of the issue is
 * edited in place from the keyboard and sends its own request, and what the server refuses is said under the property
 * that asked.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Board, Card } from "../../api/types/board";
import { CardProperties, issueNumber } from "./CardProperties";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const card = (id: number, extra: Partial<Card> = {}): Card => ({
  id,
  repository_id: 1,
  number: id,
  title: `Issue ${id}`,
  body: "The body",
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

const board = (cards: Card[], waiting: Record<string, string> = {}): Board => ({
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
  waiting,
  changes: [],
});

const PEOPLE = [
  { id: 1, handle: "owner", name: "Owner", role: "owner", disabled: false, created_at: 0 },
  { id: 2, handle: "lea", name: "Lea", role: "member", disabled: false, created_at: 0 },
  { id: 3, handle: "tom", name: "Tom", role: "member", disabled: true, created_at: 0 },
];

/** The requests a server was sent, as method, path and body; it answers with `answer` for each path it has one for. */
function server(answers: Record<string, { status: number; body: unknown }> = {}) {
  const sent: Array<[string, string, unknown]> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: { method?: string; body?: string }) => {
      const method = init?.method ?? "GET";
      // Who can be given the card is read once and is no save: it is answered here and left out of what was sent.
      if (method === "GET" && url === "/v1/people") {
        return { ok: true, status: 200, text: async () => JSON.stringify(PEOPLE) };
      }
      // The milestones on offer and the card's own are reads too: none exist unless an answer names them.
      if (method === "GET" && /^\/v1\/(milestones|cards\/\d+\/milestones)$/.test(url)) {
        const answer = answers[`GET ${url}`] ?? { status: 200, body: [] };
        return { ok: true, status: answer.status, text: async () => JSON.stringify(answer.body) };
      }
      // Whether the person follows the card is read too, and is not what these tests are about.
      if (method === "GET" && url === "/v1/session") {
        return {
          ok: true,
          status: 200,
          text: async () => JSON.stringify({ signed_in: true, person: PEOPLE[0] }),
        };
      }
      if (method === "GET" && url.endsWith("/followers")) {
        return { ok: true, status: 200, text: async () => "[]" };
      }
      sent.push([method, url, init?.body ? JSON.parse(init.body) : undefined]);
      const answer = answers[`${method} ${url}`] ?? { status: 200, body: {} };
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

/** The properties of `of` on a board that also has `others`; `onOpen` is what opening another issue does. */
function properties(of: Card, others: Card[] = [], waiting: Record<string, string> = {}) {
  const onOpen = vi.fn();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <CardProperties
          board={board([of, ...others], waiting)}
          card={of}
          item={undefined}
          onOpenCard={onOpen}
        />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return onOpen;
}

const region = (number: number) => screen.getByRole("region", { name: `Properties of #${number}` });
const requests = (sent: Array<[string, string, unknown]>) =>
  sent.map(([method, url]) => `${method} ${url}`);

/** Opens the menu of a value with Enter and picks a row of it, as a person at the keyboard does. */
async function choose(value: string, row: string) {
  fireEvent.keyDown(screen.getByRole("button", { name: value }), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitemradio", { name: row }));
}

/** Opens the field of a value with the keyboard, types and saves with Enter. */
function type(value: string, field: { role: "textbox"; name: string }, typed: string) {
  const button = screen.getByRole("button", { name: value });
  button.focus();
  fireEvent.click(button);
  const input = screen.getByRole(field.role, { name: field.name });
  fireEvent.change(input, { target: { value: typed } });
  fireEvent.keyDown(input, { key: "Enter" });
}

describe("the facts of a card", () => {
  it("names every fact the dock showed", () => {
    server();
    properties(
      card(5, {
        column: "In review",
        author: "leon",
        tokens: 42_000,
        note: "The tests are missing",
        refusals: 1,
        held: { reason: "needs a design", by: "person", at: 0 },
        run: { id: 9, agent: "Coder", purpose: "code", status: "running" },
        change: { id: 2, state: "open", checks: "failure", review_verdict: null },
      }),
      [],
      { "5": "waiting for review" },
    );
    const facts = within(region(5));
    const shown: Array<[string, string]> = [
      ["Status", "Blocked"],
      ["Column", "In review"],
      ["Repository", "demo"],
      ["Author", "leon"],
      ["Blocked", "needs a design (blocked by a person)"],
      ["Why it waits", "waiting for review · 1 rule broken"],
      ["Checks", "Failed"],
      ["Cost", "42k tok"],
      ["Note", "The tests are missing"],
    ];
    for (const [name, value] of shown) {
      // The name and its value stand in one row.
      const row = facts
        .getAllByText(name, { exact: true })
        .map((each) => each.parentElement?.textContent ?? "");
      expect(row).toContain(`${name}${value}`);
    }
    expect(facts.getByRole("link", { name: "Coder · code" }).getAttribute("href")).toBe("/runs/9");
  });

  it("names the parent and the sub-issues, and opens them in the dock", () => {
    const plan = card(1);
    const step = card(2, { parent: 1 });
    const done = card(3, { parent: 1, column: "Done", closed: true });
    server();
    const onOpen = properties(step, [plan, done]);
    expect(screen.getByRole("button", { name: "Parent #1 Issue 1" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open #1 Issue 1" }));
    expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ id: 1 }));
    cleanup();

    const opened = properties(plan, [step, done]);
    expect(screen.getByText("1 of 2 sub-issues done")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "#2 Issue 2" }));
    expect(opened).toHaveBeenCalledWith(expect.objectContaining({ id: 2 }));
    expect(screen.getByText("Closed")).toBeTruthy();
  });
});

describe("the properties of an issue, edited in place", () => {
  it("claims and closes, each with a request of its own", async () => {
    const sent = server();
    properties(card(5));
    await choose("Claim No one", "Me");
    await waitFor(() => expect(sent).toHaveLength(1));
    await choose("State Open", "Closed");
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(requests(sent)).toEqual(["POST /v1/cards/5/claim", "POST /v1/cards/5/close"]);
    cleanup();

    const again = server();
    properties(card(5, { claimed_by: "session 1", closed: true, column: "Done" }));
    await choose("Claim session 1", "No one");
    await waitFor(() => expect(again).toHaveLength(1));
    await choose("State Closed", "Open");
    await waitFor(() => expect(again).toHaveLength(2));
    expect(requests(again)).toEqual(["DELETE /v1/cards/5/claim", "POST /v1/cards/5/reopen"]);
  });

  it("renames the issue and keeps its body", async () => {
    const sent = server();
    properties(card(5));
    type("Title Issue 5", { role: "textbox", name: "Title" }, "Issue five");
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["PUT", "/v1/cards/5", { title: "Issue five", body: "The body" }]);
  });

  it("sets the parent by its number, and takes it away with an empty field", async () => {
    const sent = server();
    properties(card(5, { parent: 1 }), [card(1)]);
    type("Parent #1 Issue 1", { role: "textbox", name: "Parent" }, "#7");
    await waitFor(() => expect(sent).toHaveLength(1));
    type("Parent #1 Issue 1", { role: "textbox", name: "Parent" }, "");
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent).toEqual([
      ["PUT", "/v1/cards/5/parent", { parent: 7 }],
      ["PUT", "/v1/cards/5/parent", { parent: null }],
    ]);
  });

  it("says what the server refused under the property that asked, and only there", async () => {
    server({
      "POST /v1/cards/5/claim": { status: 409, body: "#5 is claimed by session 2 already" },
    });
    properties(card(5));
    await choose("Claim No one", "Me");
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("claimed by session 2");
    const claim = screen.getByRole("button", { name: "Claim No one" });
    expect(
      document.getElementById(claim.getAttribute("aria-describedby") ?? "")?.textContent,
    ).toContain("claimed by session 2");
    // The other properties are not described by it.
    expect(
      screen.getByRole("button", { name: "State Open" }).getAttribute("aria-describedby"),
    ).toBeNull();
  });

  it("adds and removes a blocker by its number, and a cycle is the server's to refuse", async () => {
    const sent = server();
    properties(card(5, { blocked_by: [3] }));
    fireEvent.click(screen.getByRole("button", { name: "Remove blocker #3" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    // One request at a time: the next waits for the answer.
    await waitFor(() =>
      expect(
        screen
          .getByRole("button", { name: "Add a blocker by its number" })
          .getAttribute("aria-disabled"),
      ).toBeNull(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Add a blocker by its number" }));
    const field = screen.getByRole("textbox", { name: "Add a blocker by its number" });
    fireEvent.change(field, { target: { value: "#7" } });
    fireEvent.keyDown(field, { key: "Enter" });
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
    properties(card(5));
    fireEvent.click(screen.getByRole("button", { name: "Add a blocker by its number" }));
    const again = screen.getByRole("textbox", { name: "Add a blocker by its number" });
    fireEvent.change(again, { target: { value: "7" } });
    fireEvent.keyDown(again, { key: "Enter" });
    expect((await screen.findByRole("alert")).textContent).toContain("cannot block it");
  });

  it("refuses a blocker that is not a number before anything is sent", async () => {
    const sent = server();
    properties(card(5));
    fireEvent.click(screen.getByRole("button", { name: "Add a blocker by its number" }));
    const field = screen.getByRole("textbox", { name: "Add a blocker by its number" });
    fireEvent.change(field, { target: { value: "the login one" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect((await screen.findByRole("alert")).textContent).toContain(
      "is not the number of an issue",
    );
    expect(sent).toHaveLength(0);
  });

  it("sets the whole list of labels with one added or one taken away", async () => {
    const sent = server();
    properties(card(5, { labels: ["bug", "wayfinder:spike"] }));
    fireEvent.click(screen.getByRole("button", { name: "Remove label bug" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Add a label" }).getAttribute("aria-disabled"),
      ).toBeNull(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Add a label" }));
    const field = screen.getByRole("textbox", { name: "Add a label" });
    fireEvent.change(field, { target: { value: " ui " } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent).toEqual([
      ["PUT", "/v1/cards/5/labels", { labels: ["wayfinder:spike"] }],
      ["PUT", "/v1/cards/5/labels", { labels: ["bug", "wayfinder:spike", "ui"] }],
    ]);
  });
});

describe("the priority and the due date of a card", () => {
  it("sets the priority from its menu and the due date in place, each with a request of its own", async () => {
    const sent = server();
    properties(card(5, { priority: "low", due: null, labels: ["bug", "priority:low"] }));
    // The labels that hold them are not shown as labels of their own.
    expect(screen.queryByText("priority:low")).toBeNull();
    await choose("Priority Low", "Urgent");
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["PUT", "/v1/cards/5/priority", { priority: "urgent" }]);

    fireEvent.click(screen.getByRole("button", { name: "Due date No date" }));
    const field = document.querySelector('input[type="date"]') as HTMLInputElement;
    fireEvent.change(field, { target: { value: "2026-12-01" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1]).toEqual(["PUT", "/v1/cards/5/due", { due: "2026-12-01" }]);
  });
});

describe("the number of an issue as typed", () => {
  it("takes 12 and #12, and refuses anything else", () => {
    expect(issueNumber("12")).toBe(12);
    expect(issueNumber(" #12 ")).toBe(12);
    expect(() => issueNumber("0")).toThrow();
    expect(() => issueNumber("1.5")).toThrow();
    expect(() => issueNumber("twelve")).toThrow();
  });
});

describe("the assignees of a card", () => {
  it("offers the people who are not disabled and adds the one person chosen", async () => {
    const sent = server();
    properties(card(5, { assignees: ["owner"] }));
    fireEvent.keyDown(screen.getByRole("button", { name: "Assignee @owner" }), { key: "Enter" });
    expect(
      (await screen.findByRole("menuitemcheckbox", { name: "Owner (@owner)" })).getAttribute(
        "aria-checked",
      ),
    ).toBe("true");
    expect(
      screen.getByRole("menuitemcheckbox", { name: "Lea (@lea)" }).getAttribute("aria-checked"),
    ).toBe("false");
    expect(screen.queryByRole("menuitemcheckbox", { name: /tom/ })).toBeNull();
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Lea (@lea)" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["PUT", "/v1/cards/5/assignees/lea", undefined]);
  });

  it("takes a person off, and keeps showing one who was disabled after being assigned", async () => {
    const sent = server();
    properties(card(5, { assignees: ["lea", "tom"] }));
    fireEvent.keyDown(screen.getByRole("button", { name: /^Assignee @lea/ }), {
      key: "Enter",
    });
    // The people are read once the menu is open, so wait for a name only they give.
    const lea = await screen.findByRole("menuitemcheckbox", { name: "Lea (@lea)" });
    expect(
      screen.getByRole("menuitemcheckbox", { name: "@tom" }).getAttribute("aria-checked"),
    ).toBe("true");
    fireEvent.click(lea);
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["DELETE", "/v1/cards/5/assignees/lea", undefined]);
  });

  it("says no one while the card is given to nobody", () => {
    server();
    properties(card(5));
    expect(screen.getByRole("button", { name: "Assignee No one" })).toBeTruthy();
  });
});

describe("the milestones of a card", () => {
  const launch = {
    id: 1,
    name: "Launch",
    target_date: "2027-01-31",
    description: "",
    created_at: 0,
  };
  const report = {
    id: 2,
    name: "Report",
    target_date: "2027-03-01",
    description: "",
    created_at: 0,
  };
  const view = (m: typeof launch) => ({ ...m, cards: 0, done: 0, projects: [] });

  it("says none while the card is in no milestone", async () => {
    server();
    properties(card(5));
    expect(await screen.findByRole("button", { name: "Milestone None" })).toBeTruthy();
  });

  it("shows the milestones the card is in and saves the whole set when another is added", async () => {
    const sent = server({
      "GET /v1/cards/5/milestones": { status: 200, body: [launch] },
      "GET /v1/milestones": { status: 200, body: [view(launch), view(report)] },
    });
    properties(card(5));
    const property = await screen.findByRole("button", { name: "Milestone Launch" });
    fireEvent.keyDown(property, { key: "Enter" });
    expect(
      (await screen.findByRole("menuitemcheckbox", { name: "Launch" })).getAttribute(
        "aria-checked",
      ),
    ).toBe("true");
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "Report" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual(["PUT", "/v1/cards/5/milestones", { milestones: [1, 2] }]);
  });
});
