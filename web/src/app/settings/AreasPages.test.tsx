// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import { NotificationsPage } from "./NotificationsPage";
import { ProfilePage } from "./ProfilePage";
import { RepositoriesPage } from "./RepositoriesPage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const lea = { id: 2, handle: "lea", name: "Lea", role: "member", disabled: false, created_at: 1 };

function show(ui: React.ReactNode, seed: Array<[readonly unknown[], unknown]> = [], routes = {}) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input), "http://x").pathname;
      if (init?.method === "DELETE") return new Response(null, { status: 204 });
      return new Response(JSON.stringify((routes as Record<string, unknown>)[path] ?? []), {
        headers: { "content-type": "application/json" },
      });
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  for (const [key, data] of seed) client.setQueryData(key, data);
  return {
    client,
    ...render(
      <QueryClientProvider client={client}>
        <MemoryRouter>{ui}</MemoryRouter>
      </QueryClientProvider>,
    ),
  };
}

describe("Profile", () => {
  it("shows the signed-in person and signs out from the page", async () => {
    show(<ProfilePage />, [[keys.session, { signed_in: true, person: lea }]]);
    expect(screen.getByText("Lea")).toBeTruthy();
    expect(screen.getByText("@lea")).toBeTruthy();
    expect(screen.getByText("member")).toBeTruthy();
    screen.getByRole("button", { name: "Sign out" }).click();
    await screen.findByRole("button", { name: "Signing out" });
    const calls = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
    expect(
      calls.some(([url, init]) => String(url).includes("/v1/session") && init?.method === "DELETE"),
    ).toBe(true);
  });
});

describe("Notifications", () => {
  it("lists the kinds of notice and the unread count, with the way to the Inbox", async () => {
    const notice = (id: number, read: boolean) => ({
      id,
      kind: "mentioned",
      by: "lea",
      card_id: null,
      inbox_item_id: null,
      title: "t",
      body: "b",
      at: 1,
      read,
    });
    show(<NotificationsPage />, [[keys.notices, [notice(1, false), notice(2, true)]]]);
    for (const kind of ["Assigned", "Mentions", "Following"])
      expect(screen.getByText(kind)).toBeTruthy();
    expect(screen.getByText("1 unread")).toBeTruthy();
    expect(screen.getAllByText("0 unread")).toHaveLength(2);
    expect(screen.getByRole("link", { name: "Open Inbox" }).getAttribute("href")).toBe("/inbox");
  });
});

describe("Repositories", () => {
  it("lists the repositories of every project and links to adding one", async () => {
    const project = (id: number, name: string, repos: string[]) => ({
      id,
      name,
      key: name.toUpperCase(),
      created_at: 1,
      members: [],
      open_cards: 0,
      files: null,
      repositories: repos.map((r, i) => ({ id: id * 10 + i, name: r, home: i === 0 })),
    });
    show(<RepositoriesPage />, [
      [keys.projects, [project(1, "Web", ["web-app", "design"]), project(2, "Desk", ["desktop"])]],
    ]);
    expect(screen.getByText("web-app")).toBeTruthy();
    expect(screen.getByText("desktop")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Settings of design" }).getAttribute("href")).toBe(
      "/projects/11",
    );
    expect(screen.getByRole("link", { name: /Add a repository/ }).getAttribute("href")).toBe(
      "/projects/new",
    );
  });
});
