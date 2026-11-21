/**
 * What the sidebar owes a person (`docs/design-web.md`, Sidebar): the list of places in its order, rows of 36px, a
 * count on Inbox that is said even at zero, Fleet's "n running" only while something runs, one chosen row, the same
 * list on a Settings page with Settings chosen, and nothing the standard leaves out — no key hint, no mask, no live
 * box, no card for the app's name.
 */
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { board, show } from "./SidebarTestHelpers";

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

const OWNER = { person: { id: 1, name: "Olive", handle: "olive", role: "owner" } };

const names = () =>
  within(screen.getByRole("navigation", { name: "Pages" }))
    .getAllByRole("link")
    .map((link) => link.textContent);

describe("the list of places", () => {
  it("lists the places in the order of the standard, the search button first", async () => {
    show();
    await screen.findByRole("link", { name: "notes" });
    const nav = screen.getByRole("navigation", { name: "Pages" });
    expect(
      within(nav)
        .getAllByRole("button")
        .map((b) => b.getAttribute("aria-label")),
    ).toEqual(["Search", "Add a project or a repository"]);
    // Without a role, as a member sees it: Build and Insight are the owner's.
    expect(names().map((name) => name?.replace(/\d+( running)?$/, ""))).toEqual([
      "Inbox",
      "My issues",
      "Fleet",
      "Chat",
      "Projects",
      "moebius-platform",
      "notes",
      "Wiki",
      "Views",
      "Settings",
    ]);
  });

  it("lists, for an owner, Build with Agents and Rules and Insight with Insights and Improve", async () => {
    show({ session: OWNER });
    await screen.findByRole("link", { name: "notes" });
    const nav = screen.getByRole("navigation", { name: "Pages" });
    // The order of the DOM: the group names and the rows between them, as the approved mockup draws them.
    const lines = Array.from(nav.querySelectorAll("a, div")).flatMap((node) => {
      if (node.tagName === "A") return [node.textContent?.replace(/\d+( running)?$/, "") ?? ""];
      return node.className.includes("text-sidebar-group") && !node.closest("a")
        ? [`# ${node.textContent}`]
        : [];
    });
    expect(lines.filter((line) => !["moebius-platform", "notes"].includes(line))).toEqual([
      "Inbox",
      "My issues",
      "Fleet",
      "Chat",
      "Projects",
      "# Workspace",
      "Wiki",
      "Views",
      "# Build",
      "Agents",
      "Rules",
      "# Insight",
      "Insights",
      "Improve",
      "Settings",
    ]);
    for (const gone of ["More", "Cycles", "Milestones", "Intake", "Workflows", "Automations"])
      expect(screen.queryByRole("link", { name: gone })).toBeNull();
  });

  it("leaves Build and Insight out for a member", async () => {
    show({ session: { person: { id: 2, name: "Mia", handle: "mia", role: "member" } } });
    await screen.findByRole("link", { name: "notes" });
    for (const gone of ["Agents", "Rules", "Insights", "Improve"])
      expect(screen.queryByRole("link", { name: gone })).toBeNull();
    expect(screen.queryByText("Build")).toBeNull();
    expect(screen.queryByText("Insight")).toBeNull();
  });

  it("shows Get started above Settings only while a step is missing", async () => {
    show({ start: { done: false, steps: [{ id: "a", done: false }], warnings: [] } });
    const row = await screen.findByRole("link", { name: "Get started" });
    expect(row.getAttribute("href")).toBe("/start");
    expect(names().slice(-2)).toEqual(["Get started", "Settings"]);
    cleanup();
    show();
    await screen.findByRole("link", { name: "notes" });
    expect(screen.queryByRole("link", { name: "Get started" })).toBeNull();
  });

  it("adds a project or a repository from the + of Projects", async () => {
    show();
    fireEvent.pointerDown(
      await screen.findByRole("button", { name: "Add a project or a repository" }),
      {
        button: 0,
        ctrlKey: false,
      },
    );
    expect(await screen.findByRole("menuitem", { name: "New project" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Add repository" })).toBeTruthy();
  });

  it("shows the server's name at the head with the search button beside it, and no rail of servers", async () => {
    show();
    const head = (await screen.findByTestId("server-switch")).parentElement as HTMLElement;
    expect(head.textContent).toContain("Moebius");
    expect(within(head).getByRole("button", { name: "Search" })).toBeTruthy();
    expect(screen.queryByLabelText("Servers")).toBeNull();
  });

  it("opens the search from its button", async () => {
    const onSearch = vi.fn();
    show({ onSearch });
    fireEvent.click(await screen.findByRole("button", { name: "Search" }));
    expect(onSearch).toHaveBeenCalledOnce();
  });

  it("leads each place where its name says, and a project to its board", async () => {
    show();
    const href = (name: string | RegExp) => screen.getByRole("link", { name }).getAttribute("href");
    await screen.findByRole("link", { name: "notes" });
    expect(href("Projects")).toBe("/projects");
    expect(href("moebius-platform")).toBe("/projects/1");
    expect(href("Chat")).toBe("/chat");
    expect(href("Settings")).toBe("/settings");
    expect(href(/^Inbox/)).toBe("/inbox");
  });

  it("gives every row the height of an item row", async () => {
    show();
    const settings = await screen.findByRole("link", { name: "Settings" });
    const rows = within(screen.getByRole("navigation", { name: "Pages" }))
      .getAllByRole("link")
      .filter((link) => link.textContent !== "Projects");
    expect(rows).toContain(settings);
    for (const row of rows) expect(row.className).toContain("h-(--row-list)");
  });

  it("says the count of Inbox even when it is nothing, and Fleet's work only while there is some", async () => {
    show();
    expect((await screen.findByRole("link", { name: /^Inbox/ })).textContent).toBe("Inbox0");
    await waitFor(() =>
      expect(screen.getByRole("link", { name: /^Fleet/ }).textContent).toBe("Fleet2 running"),
    );
  });

  it("shows only the green dot on Fleet's row while the Fleet page is open", async () => {
    show({ at: "/fleet" });
    const row = await screen.findByRole("link", { name: /^Fleet/ });
    await waitFor(() => expect(row.querySelector(".sr-only")?.textContent).toBe("2 running"));
    // The words are for a screen reader only; the dot is the one thing drawn.
    expect(row.querySelector("[aria-hidden=true]")).not.toBeNull();
    expect(
      row.querySelector(".text-progress-text > span:not(.sr-only):not([aria-hidden])"),
    ).toBeNull();
  });

  it("leaves Fleet's work unsaid when nothing runs", async () => {
    show({
      snapshot: { boards: [board(1, "notes", 0)], harness_changes: [], inbox: [] },
    });
    await screen.findByRole("link", { name: "notes" });
    expect(screen.getByRole("link", { name: "Fleet" }).textContent).toBe("Fleet");
  });

  it("holds nothing the standard leaves out", async () => {
    const { container } = show();
    await screen.findByRole("link", { name: "notes" });
    expect(container.querySelector("kbd")).toBeNull();
    expect(container.querySelector("[data-testid=connection]")).toBeNull();
    for (const element of container.querySelectorAll("*"))
      expect(element.className.toString()).not.toMatch(/mask|scroll-fade|rail/);
    expect(screen.queryByText(/^Back$/)).toBeNull();
  });
});

describe("where an entry leads", () => {
  it("marks the one entry that leads to the page on screen", async () => {
    show({ at: "/fleet" });
    await screen.findByRole("link", { name: /^Fleet/ });
    const current = screen
      .getAllByRole("link")
      .filter((link) => link.getAttribute("aria-current") === "page");
    expect(current.map((link) => link.textContent?.replace(/\d.*/, ""))).toEqual(["Fleet"]);
  });

  it("marks a project by the address of its board, with a card open on it too", async () => {
    show({ at: "/projects/2" });
    const notes = await screen.findByRole("link", { name: "notes" });
    expect(notes.getAttribute("aria-current")).toBe("page");
    expect(
      screen.getByRole("link", { name: "moebius-platform" }).getAttribute("aria-current"),
    ).toBeNull();
  });

  it("marks a project on its list and the editor of a column's script, and not on its settings or a repository", async () => {
    const current = async (at: string) => {
      show({ at });
      await screen.findByRole("link", { name: "notes" });
      const marked = screen
        .getAllByRole("link")
        .filter((link) => link.getAttribute("aria-current") === "page")
        .map((link) => link.textContent);
      cleanup();
      return marked;
    };
    expect(await current("/projects/2/list")).toEqual(["notes"]);
    expect(await current("/projects/2/settings/columns/Backlog")).toEqual(["notes"]);
    expect(await current("/projects/2?repository=2")).toEqual(["notes"]);
    expect(await current("/projects/2/cycles")).toEqual(["notes"]);
    expect(await current("/projects/2/workflows")).toEqual(["notes"]);
    expect(await current("/projects/2/settings")).toEqual([]);
    expect(await current("/projects/2/repositories/2")).toEqual([]);
  });

  it("keeps the same list on a Settings page, with Settings chosen and no Back row", async () => {
    show({ at: "/models" });
    await screen.findByRole("link", { name: "notes" });
    expect(names().length).toBe(10);
    expect(screen.getByRole("link", { name: "Settings" }).getAttribute("aria-current")).toBe(
      "page",
    );
    expect(screen.queryByRole("link", { name: /Back/ })).toBeNull();
    expect(screen.queryByRole("link", { name: "General" })).toBeNull();
  });

  it("chooses the row of Build, not Settings, on a page both list", async () => {
    show({ at: "/agents", session: OWNER });
    await screen.findByRole("link", { name: "notes" });
    expect(screen.getByRole("link", { name: "Agents" }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("link", { name: "Settings" }).getAttribute("aria-current")).toBeNull();
  });

  it("chooses Settings on the pages of its own list as well", async () => {
    show({ at: "/profile" });
    expect(
      (await screen.findByRole("link", { name: "Settings" })).getAttribute("aria-current"),
    ).toBe("page");
  });
});

describe("noting the places a person opens", () => {
  const noted = () =>
    vi
      .mocked(fetch)
      .mock.calls.filter(([path]) => String(path).startsWith("/v1/recent"))
      .map(([, init]) => (init as RequestInit | undefined)?.body);

  it("notes a project's board once the snapshot has it", async () => {
    show({ at: "/projects/2" });
    await waitFor(() =>
      expect(noted()).toEqual([JSON.stringify({ kind: "project", repository_id: 2 })]),
    );
  });

  it("asks nothing about a board the snapshot does not hold, as a private project of someone else", async () => {
    show({ at: "/projects/12" });
    await screen.findByRole("link", { name: "notes" });
    expect(noted()).toEqual([]);
  });
});
