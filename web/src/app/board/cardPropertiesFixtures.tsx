/**
 * What the tests of the card's properties share: cards and boards to build on, a fake server that records the requests
 * it is sent, the properties rendered on a board, and the keyboard steps that open a menu or a field and save.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { vi } from "vitest";
import type { Board, Card } from "../../api/types/board";
import { CardProperties } from "./CardProperties";

export const card = (id: number, extra: Partial<Card> = {}): Card => ({
  id,
  repository_id: 1,
  project_id: 1,
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
  estimate: null,
  ...extra,
});

export const board = (cards: Card[], waiting: Record<string, string> = {}): Board => ({
  repository: {
    id: 1,
    name: "demo",
    path: "/r",
    forge: "none",
    default_branch: "main",
    home_project_id: 1,
    github: null,
    trusted: [],
  },
  cards,
  round_limit: 5,
  waiting,
  changes: [],
});

export const PEOPLE = [
  { id: 1, handle: "owner", name: "Owner", role: "owner", disabled: false, created_at: 0 },
  { id: 2, handle: "lea", name: "Lea", role: "member", disabled: false, created_at: 0 },
  { id: 3, handle: "tom", name: "Tom", role: "member", disabled: true, created_at: 0 },
];

/** The requests a server was sent, as method, path and body; it answers with `answer` for each path it has one for. */
export function server(answers: Record<string, { status: number; body: unknown }> = {}) {
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
      // The cycles on offer and the card's own are reads too; the card is in none unless an answer says so.
      if (method === "GET" && /^\/v1\/(projects\/\d+\/cycles|cards\/\d+\/cycle)$/.test(url)) {
        const answer = answers[`GET ${url}`] ?? {
          status: 200,
          body: url.endsWith("/cycle") ? null : [],
        };
        return { ok: true, status: answer.status, text: async () => JSON.stringify(answer.body) };
      }
      // The project's repositories are read too: the card's project has the card's own only unless a test names more.
      if (method === "GET" && url === "/v1/projects") {
        const answer = answers["GET /v1/projects"] ?? { status: 200, body: [] };
        return { ok: true, status: answer.status, text: async () => JSON.stringify(answer.body) };
      }
      // No kind of issue is defined here; the Kind property is tested with `KindProperties`.
      if (method === "GET" && url === "/v1/kinds") {
        return { ok: true, status: 200, text: async () => "[]" };
      }
      // Whether the person follows the card is read too, and is not what these tests are about.
      if (method === "GET" && url === "/v1/session") {
        return {
          ok: true,
          status: 200,
          text: async () => JSON.stringify({ signed_in: true, person: PEOPLE[0] }),
        };
      }
      // The wiki pages that link to the card are read too: none do here.
      if (method === "GET" && url.startsWith("/v1/wiki/cards/")) {
        return { ok: true, status: 200, text: async () => "[]" };
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
export function properties(
  of: Card,
  others: Card[] = [],
  waiting: Record<string, string> = {},
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
) {
  const onOpen = vi.fn();
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

export const region = (number: number) =>
  screen.getByRole("region", { name: `Properties of #${number}` });
export const requests = (sent: Array<[string, string, unknown]>) =>
  sent.map(([method, url]) => `${method} ${url}`);

/** Opens the menu of a value with Enter and picks a row of it, as a person at the keyboard does. */
export async function choose(value: string, row: string) {
  fireEvent.keyDown(screen.getByRole("button", { name: value }), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitemradio", { name: row }));
}

/** Opens the field of a value with the keyboard, types and saves with Enter. */
export function type(value: string, field: { role: "textbox"; name: string }, typed: string) {
  const button = screen.getByRole("button", { name: value });
  button.focus();
  fireEvent.click(button);
  const input = screen.getByRole(field.role, { name: field.name });
  fireEvent.change(input, { target: { value: typed } });
  fireEvent.keyDown(input, { key: "Enter" });
}
