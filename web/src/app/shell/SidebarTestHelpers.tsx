/**
 * What the sidebar's tests share: the fake server's routes and a board to put in them, and `show`, which
 * renders the `Sidebar` on a page of the app with the fake in place of `fetch`.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { vi } from "vitest";
import { keys } from "../../api/keys";
import { Sidebar } from "./Sidebar";

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
  "/v1/start": { done: true, steps: [], warnings: [] },
};

export function show({
  at = "/inbox",
  session,
  snapshot,
  onSearch = () => {},
}: {
  /** The session the app holds, as App.tsx keeps it. */
  session?: unknown;
  /** The page the sidebar is opened on. */
  at?: string;
  /** What the server answers to the snapshot, when a test needs another. */
  snapshot?: unknown;
  onSearch?: () => void;
} = {}) {
  const routes: Record<string, unknown> = { ...ROUTES };
  if (snapshot !== undefined) routes["/v1/snapshot"] = snapshot;
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
        <Sidebar onSearch={onSearch} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}
