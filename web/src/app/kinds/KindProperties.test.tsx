// @vitest-environment jsdom
/**
 * The Kind property and the properties of a kind's fields: only the kinds the card's project picked are on offer, each
 * field is edited as its type says and saves alone, a cleared value is saved as none, and nothing is drawn for a card
 * whose project offers no kind.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Card } from "../../api/types/board";
import type { Kind } from "../../api/types/kinds";
import { KindProperties } from "./KindProperties";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const KINDS: Kind[] = [
  {
    id: 1,
    name: "Bug",
    projects: [10],
    fields: [
      { id: 1, name: "Severity", type: "choice", options: ["low", "high"] },
      { id: 2, name: "Estimate", type: "number", options: [] },
      { id: 3, name: "Due", type: "date", options: [] },
      { id: 4, name: "Reporter", type: "person", options: [] },
      { id: 5, name: "Steps", type: "text", options: [] },
    ],
  },
  { id: 2, name: "Task", projects: [11], fields: [] },
];

const PEOPLE = [
  { id: 1, handle: "owner", name: "Owner", role: "owner", disabled: false, created_at: 0 },
  { id: 2, handle: "lea", name: "Lea", role: "member", disabled: false, created_at: 0 },
];

const card = (extra: Partial<Card> = {}): Card =>
  ({
    id: 7,
    repository_id: 1,
    project_id: 10,
    number: 7,
    title: "Broken",
    kind: null,
    fields: {},
    ...extra,
  }) as Card;

function server() {
  const sent: Array<[string, string, unknown]> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: { method?: string; body?: string }) => {
      const method = init?.method ?? "GET";
      const reply = (body: unknown) => ({
        ok: true,
        status: 200,
        text: async () => JSON.stringify(body),
      });
      if (method === "GET" && url === "/v1/kinds") return reply(KINDS);
      if (method === "GET" && url === "/v1/people") return reply(PEOPLE);
      sent.push([method, url, init?.body ? JSON.parse(init.body) : undefined]);
      return reply({});
    }),
  );
  return sent;
}

function draw(of: Card) {
  const saved = vi.fn(async (request: Promise<unknown>) => {
    await request;
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(
    <QueryClientProvider client={client}>
      <KindProperties card={of} saved={saved} />
    </QueryClientProvider>,
  );
  return { saved, view };
}

describe("the kind of a card", () => {
  it("offers only the kinds the project picked, and none when it picked none", async () => {
    server();
    draw(card());
    fireEvent.keyDown(await screen.findByRole("button", { name: "Kind None" }), { key: "Enter" });
    const rows = (await screen.findAllByRole("menuitemradio")).map((r) => r.textContent);
    expect(rows).toEqual(["None", "Bug"]);
  });

  it("draws nothing for a card of a project that offers no kind", async () => {
    server();
    const { view } = draw(card({ project_id: 99 }));
    await vi.waitFor(() => expect(view.container.innerHTML).toBe(""));
    expect(screen.queryByText("Kind")).toBeNull();
  });

  it("gives the card a kind and takes it off again", async () => {
    const sent = server();
    draw(card());
    fireEvent.keyDown(await screen.findByRole("button", { name: "Kind None" }), { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitemradio", { name: "Bug" }));
    await vi.waitFor(() => expect(sent).toEqual([["PUT", "/v1/cards/7/kind", { kind: "Bug" }]]));
  });
});

describe("the fields of the kind", () => {
  const bug = (fields: Card["fields"] = {}) => card({ kind: "Bug", fields });

  it("shows a property for each field, with the value the card has", async () => {
    server();
    draw(bug({ Severity: "high", Estimate: 3 }));
    expect(await screen.findByRole("button", { name: "Severity high" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Estimate 3" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Due None" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reporter None" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Steps None" })).toBeTruthy();
  });

  it("saves a choice, a number, a person and a text each alone", async () => {
    const sent = server();
    draw(bug({ Estimate: 3 }));
    fireEvent.keyDown(await screen.findByRole("button", { name: "Severity None" }), {
      key: "Enter",
    });
    fireEvent.click(await screen.findByRole("menuitemradio", { name: "low" }));
    await vi.waitFor(() =>
      expect(sent.at(-1)).toEqual(["PUT", "/v1/cards/7/fields", { fields: { Severity: "low" } }]),
    );

    const estimate = screen.getByRole("button", { name: "Estimate 3" });
    fireEvent.click(estimate);
    const input = screen.getByRole("textbox", { name: "Estimate" });
    fireEvent.change(input, { target: { value: "5" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await vi.waitFor(() =>
      expect(sent.at(-1)).toEqual(["PUT", "/v1/cards/7/fields", { fields: { Estimate: 5 } }]),
    );

    fireEvent.keyDown(screen.getByRole("button", { name: "Reporter None" }), { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitemradio", { name: "Lea (@lea)" }));
    await vi.waitFor(() =>
      expect(sent.at(-1)).toEqual(["PUT", "/v1/cards/7/fields", { fields: { Reporter: "lea" } }]),
    );

    fireEvent.click(screen.getByRole("button", { name: "Steps None" }));
    const steps = screen.getByRole("textbox", { name: "Steps" });
    fireEvent.change(steps, { target: { value: "open it" } });
    fireEvent.keyDown(steps, { key: "Enter" });
    await vi.waitFor(() =>
      expect(sent.at(-1)).toEqual(["PUT", "/v1/cards/7/fields", { fields: { Steps: "open it" } }]),
    );
  });

  it("clears a text that was emptied", async () => {
    const sent = server();
    draw(bug({ Steps: "open it" }));
    fireEvent.click(await screen.findByRole("button", { name: "Steps open it" }));
    const steps = screen.getByRole("textbox", { name: "Steps" });
    fireEvent.change(steps, { target: { value: "  " } });
    fireEvent.keyDown(steps, { key: "Enter" });
    await vi.waitFor(() =>
      expect(sent.at(-1)).toEqual(["PUT", "/v1/cards/7/fields", { fields: { Steps: null } }]),
    );
  });
});
