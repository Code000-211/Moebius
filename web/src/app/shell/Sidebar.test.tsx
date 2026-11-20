/**
 * What the sidebar owes a person (`docs/design-web.md`, Sidebar): the list of places in its order, rows of 36px, a
 * count on Inbox that is said even at zero, Fleet's "n running" only while something runs, one chosen row, the same
 * list on a Settings page with Settings chosen, and nothing the standard leaves out — no key hint, no mask, no live
 * box, no card for the app's name.
 */
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { board, show } from "./SidebarTestHelpers";

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

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
    ).toEqual(["Search"]);
    expect(names().map((name) => name?.replace(/\d+( running)?$/, ""))).toEqual([
      "Inbox",
      "My issues",
      "Fleet",
      "Projects",
      "moebius-platform",
      "notes",
      "Wiki",
      "Cycles",
      "Views",
      "More",
      "Settings",
    ]);
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
    expect(href("moebius-platform")).toBe("/board/1");
    expect(href("More")).toBe("/more");
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
    expect(screen.queryByText("Improve")).toBeNull();
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
    show({ at: "/board/2" });
    const notes = await screen.findByRole("link", { name: "notes" });
    expect(notes.getAttribute("aria-current")).toBe("page");
    expect(
      screen.getByRole("link", { name: "moebius-platform" }).getAttribute("aria-current"),
    ).toBeNull();
  });

  it("keeps the same list on a Settings page, with Settings chosen and no Back row", async () => {
    show({ at: "/models" });
    await screen.findByRole("link", { name: "notes" });
    expect(names().length).toBe(11);
    expect(screen.getByRole("link", { name: "Settings" }).getAttribute("aria-current")).toBe(
      "page",
    );
    expect(screen.queryByRole("link", { name: /Back/ })).toBeNull();
    expect(screen.queryByRole("link", { name: "General" })).toBeNull();
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
    show({ at: "/board/2" });
    await waitFor(() =>
      expect(noted()).toEqual([JSON.stringify({ kind: "project", repository_id: 2 })]),
    );
  });

  it("asks nothing about a board the snapshot does not hold, as a private project of someone else", async () => {
    show({ at: "/board/12" });
    await screen.findByRole("link", { name: "notes" });
    expect(noted()).toEqual([]);
  });
});
