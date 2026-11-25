// @vitest-environment jsdom
/** The daily digest in the Inbox: what it says per repository, the tokens, what needs a person, and dismissing it. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { InboxItem, Snapshot } from "../../api/types/board";
import type { Digest } from "../../api/types/factory";
import { Decision } from "./Decision";
import { DigestView, digestOf } from "./DigestView";
import { kindTag, rowReason, why } from "./model";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const line = (over: Partial<Digest["repositories"][0]["merged"][0]> = {}) => ({
  number: 12,
  title: "Cache the plan",
  who: "alice",
  detail: "",
  measured: false,
  ...over,
});

const digest: Digest = {
  day: "2026-11-25",
  since: 0,
  until: 1,
  repositories: [
    {
      id: 1,
      name: "widgets",
      merged: [
        line({ measured: true }),
        line({ number: 13, title: "Shorter prompts", who: "moebius" }),
      ],
      refused: [line({ number: 14, title: "Fix lanes", who: null, detail: "the checks failed" })],
      reverts: [line({ number: 15, title: "Bad idea", who: null, detail: "it broke the build" })],
      review_runs: 2,
      rounds: [line({ number: 7, title: "", detail: "round 1: changes requested" })],
      taken: [line({ number: 3, title: "Docs", who: "bob" })],
      released: [line({ number: 4, title: "More docs", who: "carol" })],
      upkeep: [
        line({ number: 5, title: "Stale", who: "dave", detail: "released: taken 15 days ago" }),
      ],
      frozen: [line({ number: 6, title: "Held", who: "person", detail: "waiting for design" })],
      more: { taken: 4 },
    },
  ],
  tokens: { spent: 123456, day_budget: 500000, today: 1000 },
  needs_a_person: [
    {
      kind: "merge_stopped",
      reason: "A merge stopped",
      items: [{ id: 9, title: "Two.", card_id: 22, number: 2, repository: "widgets" }],
    },
  ],
  text: "",
};

const item: InboxItem = {
  id: 31,
  kind: "daily_digest",
  card_id: null,
  title: "Daily digest 2026-11-25",
  data: digest as unknown as Record<string, unknown>,
  resolved_at: null,
  at: 100,
  assignees: [],
};

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.search}</p>;
}

function view(node: React.ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(keys.session, {
    signed_in: true,
    person: {
      id: 1,
      handle: "owner",
      name: "Owner",
      role: "owner",
      disabled: false,
      created_at: 0,
    },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        {node}
        <Where />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the digest", () => {
  it("is told per repository, with who wrote what and which merge came with a measurement", () => {
    view(<DigestView digest={digest} />);
    const section = screen.getByRole("region", { name: "widgets" });
    const merged = within(section).getByRole("heading", { name: "Merged" })
      .parentElement as HTMLElement;
    expect(within(merged).getByText("Cache the plan")).toBeTruthy();
    expect(within(merged).getByText("by alice")).toBeTruthy();
    expect(within(merged).getAllByText("measured")).toHaveLength(1);
    expect(within(merged).getByText("by moebius")).toBeTruthy();
    expect(within(section).getByText("the checks failed")).toBeTruthy();
    expect(within(section).getByText("it broke the build")).toBeTruthy();
    expect(within(section).getByText("round 1: changes requested")).toBeTruthy();
    expect(
      within(section).getByText("2 review runs of the Reviewer on its own changes"),
    ).toBeTruthy();
    expect(within(section).getByText("@bob")).toBeTruthy();
    expect(within(section).getByText("@carol")).toBeTruthy();
    expect(within(section).getByText("released: taken 15 days ago")).toBeTruthy();
    expect(within(section).getByText("held by person")).toBeTruthy();
    // A list the server cut says how many more there were.
    expect(within(section).getByText("and 4 more")).toBeTruthy();
  });

  it("tells the tokens against the budget and what needs a person, with a link to its card", () => {
    view(<DigestView digest={digest} />);
    expect(screen.getByTestId("digest-tokens").textContent).toContain(
      "123,456 since the last digest, against a budget of 500,000 a day",
    );
    const needs = screen.getByTestId("digest-needs");
    expect(within(needs).getByText("Needs a person (1)")).toBeTruthy();
    expect(within(needs).getByText("A merge stopped (1)")).toBeTruthy();
    fireEvent.click(within(needs).getByRole("button", { name: /#2 Two\./ }));
    expect(screen.getByTestId("where").textContent).toContain("card=22");
  });

  it("says so on a quiet day, with no budget and nothing waiting", () => {
    view(
      <DigestView
        digest={{
          ...digest,
          repositories: [],
          needs_a_person: [],
          tokens: { spent: 0, day_budget: null, today: 0 },
        }}
      />,
    );
    expect(screen.getByTestId("digest-quiet")).toBeTruthy();
    expect(screen.getByText("Nothing needs a person")).toBeTruthy();
    expect(screen.getByTestId("digest-tokens").textContent).toBe("Tokens 0 since the last digest.");
  });

  it("is read from an item and from no other", () => {
    expect(digestOf(item)?.day).toBe("2026-11-25");
    expect(digestOf({ ...item, data: {} })).toBeNull();
  });
});

describe("the digest as an Inbox item", () => {
  it("is a word and a sentence, and its row says no chat", () => {
    expect(kindTag(item).label).toBe("Daily digest");
    expect(rowReason(item)).toBe("Daily digest");
    expect(why(item, {})).toContain("since the last digest");
  });

  it("opens with the digest and a button that dismisses it", async () => {
    const calls: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        calls.push(`${init?.method ?? "GET"} ${url}`);
        return new Response("{}", { status: 200, headers: { "content-type": "application/json" } });
      }),
    );
    const snapshot: Snapshot = { boards: [], harness_changes: [], inbox: [item] };
    view(<Decision item={item} snapshot={snapshot} />);
    expect(screen.getByTestId("digest")).toBeTruthy();
    expect(screen.queryByText("chat")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(calls).toContain("POST /v1/digest/31/dismiss"));
  });
});
