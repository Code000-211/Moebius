/**
 * What the sidebar's tests share: the fake server's routes, a board and a chat to put in them, and `show`, which
 * renders the `Sidebar` on a page of the app with the fake in place of `fetch`.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { vi } from "vitest";
import { keys } from "../../api/keys";
import type { Chat } from "../../api/types/chat";
import { Sidebar } from "./Sidebar";

/** The keys the open/closed state of each group is kept under, like `moebius.sidebar.collapsed` is. */
export const GROUP_KEYS = {
  Improve: "moebius.sidebar.group.improve",
  Project: "moebius.sidebar.project",
};

/** A board with `name`, with a running card for each of `working`. */
export function board(id: number, name: string, working: number) {
  return {
    repository: {
      id,
      name,
      path: `/srv/moebius/repos/${name}`,
      forge: "none",
      default_branch: "main",
      github: null,
      trusted: [],
    },
    cards: Array.from({ length: working }, (_, each) => ({
      run: { id: each, agent: "Coder", purpose: "Work", status: "running" },
    })),
    round_limit: 3,
    waiting: {},
    changes: [],
  };
}

export const ROUTES: Record<string, unknown> = {
  "/v1/snapshot": {
    boards: [board(1, "moebius-platform", 2), board(2, "notes", 0)],
    harness_changes: [],
    inbox: [],
  },
  "/v1/health": { version: "0.9.0" },
  "/v1/chats": [],
  "/v1/start": { done: true, steps: [], warnings: [] },
};

/** A conversation started at `startedAt`, read to its last answer, unless a field says otherwise. */
export function chat(id: number, startedAt: number, over: Partial<Chat> = {}) {
  return {
    id,
    agent: "Moebius",
    status: "waiting",
    repository_id: null,
    harness_repository: false,
    started_at: startedAt,
    ended_at: null,
    title: "Work",
    answered: 1,
    seen: 1,
    changed_files: 0,
    asks: false,
    origin: "chat",
    model: null,
    thinking: null,
    owner: null,
    shared: false,
    mine: true,
    ...over,
  };
}

export function show({
  collapsed = false,
  at = "/inbox",
  chats = [],
  session,
}: {
  collapsed?: boolean;
  /** The session the app holds, as App.tsx keeps it. */
  session?: unknown;
  /** The page the sidebar is opened on, which decides the list it shows. */
  at?: string;
  /** The conversations the server lists, newest first as it sends them. */
  chats?: unknown[];
} = {}) {
  const routes: Record<string, unknown> = { ...ROUTES, "/v1/chats": chats };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string) => ({
      ok: true,
      status: 200,
      text: async () => JSON.stringify(routes[path.split("?")[0] ?? path] ?? []),
    })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (session !== undefined) client.setQueryData(keys.session, session);
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[at]}>
        <Sidebar
          connection="reconnecting"
          collapsed={collapsed}
          onToggle={() => {}}
          onSearch={() => {}}
        />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}
