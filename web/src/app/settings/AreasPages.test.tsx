// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import { ColumnScriptsPage } from "./ColumnScriptsPage";
import { ExportPage } from "./ExportPage";
import { NotificationsPage } from "./NotificationsPage";
import { ProfilePage } from "./ProfilePage";
import { RepositoriesPage } from "./RepositoriesPage";
import { SchedulesPage } from "./SchedulesPage";

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
      "/projects/1/repositories/11",
    );
    expect(screen.getByRole("link", { name: /Add a repository/ }).getAttribute("href")).toBe(
      "/projects/new",
    );
  });
});

describe("Column scripts", () => {
  it("lists the columns of a board and links each to its editor", async () => {
    const project = {
      id: 4,
      name: "Shop",
      repositories: [{ id: 9, name: "shop", home: true }],
    };
    show(<ColumnScriptsPage />, [
      [keys.projects, [project]],
      [
        keys.projectScripts(4),
        [
          { column: "Backlog", path: "p/backlog.luau", source: "built-in", text: "" },
          { column: "In progress", path: "p/progress.luau", source: "project", text: "" },
        ],
      ],
    ]);
    expect(screen.getByText("Backlog")).toBeTruthy();
    expect(screen.getByText("This project's own")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Script of In progress" }).getAttribute("href")).toBe(
      "/projects/4/settings/columns/In%20progress?repository=9",
    );
  });
});

describe("Schedules", () => {
  it("shows the nightly times and the automations with their next firing", async () => {
    const value = (v: string) => ({ value: v, default: true });
    show(<SchedulesPage />, [
      [
        keys.settings,
        {
          file: "moebius.toml",
          values: {
            retro_at: value("03:00"),
            "trial_window.start": value("00:00"),
            "trial_window.end": value("06:00"),
          },
          secrets: [],
          restart_needed: false,
          restart_keys: [],
        },
      ],
      [
        keys.automations,
        [
          {
            id: 3,
            name: "Weekly digest",
            schedule: "0 9 * * 1",
            agent: "moebius",
            prompt: "",
            repository_id: null,
            harness_repository: false,
            enabled: true,
            next_firing: Math.floor(Date.now() / 1000) + 7200,
            last_firing: null,
          },
        ],
      ],
    ]);
    expect(screen.getByText("03:00")).toBeTruthy();
    expect(screen.getByText("00:00 – 06:00")).toBeTruthy();
    expect(screen.getByText("Weekly digest")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Open Weekly digest" }).getAttribute("href")).toBe(
      "/automations?automation=3",
    );
  });
});

describe("Export", () => {
  it("lists the exports with their judgements, and leads to the Decisions page", async () => {
    show(<ExportPage />, [
      [
        keys.exports,
        [
          {
            id: 2,
            at: Math.floor(Date.now() / 1000) - 7200,
            kinds: [
              { kind: "merge", count: 5, with_outcome: 5, by_person: 1, held_out: 0 },
              { kind: "tool_call", count: 3, with_outcome: 0, by_person: 0, held_out: 0 },
            ],
          },
        ],
      ],
    ]);
    expect(screen.getByText("Export 2")).toBeTruthy();
    expect(screen.getByText("2 decision types")).toBeTruthy();
    expect(screen.getByText(/8 judgements/)).toBeTruthy();
    expect(screen.getByRole("link", { name: "Open Decisions" }).getAttribute("href")).toBe(
      "/decision-model",
    );
  });
});
