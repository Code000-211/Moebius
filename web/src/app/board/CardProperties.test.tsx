// @vitest-environment jsdom
/**
 * The card's properties in the right column: every fact the dock used to list is there, each property of the issue is
 * edited in place from the keyboard and sends its own request, and what the server refuses is said under the property
 * that asked. The people, milestones and cycles are in `CardPlanning.test.tsx`, the repositories and changes in
 * `CardRepositories.test.tsx`.
 */
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { issueNumber } from "./CardProperties";
import { card, choose, properties, region, requests, server, type } from "./cardPropertiesFixtures";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

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

describe("the estimate of a card", () => {
  it("shows its points and sets and clears them in place, without showing the label that holds them", async () => {
    const sent = server();
    properties(card(5, { estimate: 3, labels: ["bug", "estimate:3"] }));
    expect(screen.queryByText("estimate:3")).toBeNull();
    type("Estimate 3 points", { role: "textbox", name: "Estimate" }, "8");
    await waitFor(() => expect(sent).toHaveLength(1));
    type("Estimate 3 points", { role: "textbox", name: "Estimate" }, "");
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent).toEqual([
      ["PUT", "/v1/cards/5/estimate", { estimate: 8 }],
      ["PUT", "/v1/cards/5/estimate", { estimate: null }],
    ]);
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
