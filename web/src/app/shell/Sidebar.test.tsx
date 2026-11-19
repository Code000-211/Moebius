/**
 * What the sidebar owes a person: the words the wide sidebar shows next to a name — a board's name, its work, the
 * connection — stay reachable when it collapses to a rail of icons, and the mark is where the app is recognised from
 * (`docs/design-web.md`: status is never colour alone, icon-only controls get accessible names, and a row is the same
 * height open or collapsed). Exactly one entry says which page is on screen, Chat leads to a conversation rather than
 * to an empty field, and the way out of the session is an item of the Settings list rather than a foot on every page.
 * The look of the app is chosen there too, not here.
 */
import { cleanup, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { chat, ROUTES, show } from "./SidebarTestHelpers";

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe("the wide sidebar", () => {
  it("keeps a name that is too long to fit reachable", async () => {
    show();
    // The switcher's name is the chosen project's, whole in its title.
    await screen.findByRole("link", { name: /^Board/ });
    const switcher = screen.getByRole("button", { name: "Change project" });
    expect(switcher.getAttribute("title")).toBe("moebius-platform");
  });

  /**
   * The state most boards are in is the mark alone: "Idle" printed on every row is the word nobody reads. The word
   * goes, the mark takes its place at the end of the row, and the name of the row keeps the word for whoever cannot
   * take the colour in.
   */
  it("marks a board that is waiting rather than printing Idle on its row", async () => {
    show({ at: "/board/2" });
    const idle = await screen.findByRole("link", { name: "Board. Idle" });
    expect(idle.textContent).toBe("Board");
    expect(idle.querySelector("span[aria-hidden='true']")).not.toBeNull();
  });

  it("counts a board's live work beside its mark, in a number alone", async () => {
    show();
    const live = await screen.findByRole("link", { name: "Board. 2" });
    expect(live.textContent).toBe("Board2");
    expect(live.querySelector("span.text-progress-text")?.textContent).toBe("2");
  });

  it("names the page switcher by what it does, and says the same on screen", () => {
    show();
    const button = screen.getByRole("button", { name: "Search" });
    expect(button.textContent).toContain("Search");
  });

  it("carries the mark, so the app is recognisable at a glance", async () => {
    const { container } = show();
    await screen.findByRole("link", { name: /^Board/ });
    expect(container.querySelector("svg[viewBox='0 0 22 16']")).not.toBeNull();
  });
});

describe("where an entry leads", () => {
  it("marks the one entry that leads to the page on screen", async () => {
    show({ at: "/inbox" });
    await screen.findByRole("link", { name: /^Inbox/ });
    const marked = screen
      .getAllByRole("link")
      .filter((link) => link.getAttribute("aria-current") !== null);
    expect(marked).toHaveLength(1);
    expect(marked[0]?.getAttribute("aria-current")).toBe("page");
    expect(marked[0]?.getAttribute("href")).toBe("/inbox");
  });

  it("marks a board by its own address, and not the group it sits under", async () => {
    show({ at: "/board/2" });
    const entry = await screen.findByRole("link", { name: /^Board/ });
    expect(entry.getAttribute("aria-current")).toBe("page");
    const marked = screen
      .getAllByRole("link")
      .filter((link) => link.getAttribute("aria-current") !== null);
    expect(marked).toHaveLength(1);
  });

  it("leaves Chat off the wide list once the chat list is in the sidebar", async () => {
    show({ at: "/chat/9", chats: [chat(9, 400), chat(21, 900)] });
    await screen.findByRole("link", { name: /^Inbox/ });
    expect(screen.queryByRole("link", { name: /^Chat/ })).toBeNull();
  });

  it("marks no everyday page on a conversation, the wide list having no Chat row to stand in", async () => {
    show({ at: "/chat/9" });
    await screen.findByRole("link", { name: /^Inbox/ });
    // Chat was the row that stood for a conversation; with it gone from the wide list, no everyday page matches the
    // address, so none of them says "you are here" — the rail keeps the standing Chat icon.
    const marked = screen
      .getAllByRole("link")
      .filter((link) => link.getAttribute("aria-current") !== null);
    expect(marked).toHaveLength(0);
  });

  it("keeps the rail's Chat icon leading to the newest conversation, with the unread trail", async () => {
    show({
      at: "/inbox",
      collapsed: true,
      chats: [chat(9, 400), { ...chat(21, 900), answered: 2, seen: 1 }],
    });
    // The entry is drawn while the list is still on its way, and moves to the newest conversation once it lands.
    const entry = await waitFor(() => {
      const link = screen.getByRole("link", { name: /^Chat/ });
      expect(link.getAttribute("href")).toBe("/chat/21");
      return link;
    });
    expect(entry.getAttribute("aria-current")).toBeNull();
    // The unread trail stands alone by the icon: the conversation the server counts not-yet-read, on the row's name.
    expect(entry.getAttribute("aria-label")).toBe("Chat. 1");
  });

  it("keeps Chat at the front of the rail while a conversation is open there", async () => {
    show({ at: "/chat/9", collapsed: true, chats: [chat(9, 400), chat(21, 900)] });
    await waitFor(() =>
      expect(screen.getByRole("link", { name: /^Chat/ }).getAttribute("href")).toBe("/chat/21"),
    );
    // A chat has an address of its own, so the entry stays standing on every conversation rather than leading back to
    // the newest while it is read on screen.
    expect(screen.getByRole("link", { name: /^Chat/ }).getAttribute("aria-current")).toBe("page");
  });
});

describe("the rail", () => {
  it("keeps a board's work in the name of its icon", async () => {
    show({ collapsed: true });
    await screen.findByRole("link", { name: "Board. 2" });
  });

  it("names a board's work where its words are gone", async () => {
    show({ collapsed: true, at: "/board/2" });
    const idle = await screen.findByRole("link", { name: "Board. Idle" });
    expect(idle.getAttribute("aria-label")).toBe("Board. Idle");
    // The rail draws the icon alone — and the dot beside it is decoration, so the words are what say a board is idle.
    expect(idle.textContent).toBe("");
    expect(idle.querySelector("svg")).not.toBeNull();
    expect(idle.querySelector("span[aria-hidden='true']")).not.toBeNull();
  });

  it("keeps the shape of a row when a tooltip wraps it", async () => {
    show({ collapsed: true });
    const inbox = await screen.findByRole("link", { name: /^Inbox/ });
    const row = inbox.getAttribute("class") ?? "";
    expect(row).toContain("min-h-8");
    expect(row).toContain("justify-center");
    // A tooltip's trigger folds a function `className` into text rather than calling it, which is how the rail's rows
    // once lost their whole shape and left their icons uncentred at half height.
    expect(row).not.toContain("isActive");
  });

  it("holds the mark and the way to change the shape in the head, in both shapes", async () => {
    // The way to narrow and widen the list was moved to the foot of the list once, and a comment came back asking for
    // it: at the foot it sat out of sight of the thing it changes, and the head was the only place it was ever looked
    // for. Both shapes keep it there, so it does not move when the shape does.
    // One height for both shapes: a head that asked for less in the rail would climb every row under it
    // a step the moment the sidebar narrowed, and it is tall enough that the rail's stacked pair has
    // room above the mark rather than sitting on the top edge of the window.
    let asked: string[] | null = null;
    for (const [collapsed, name] of [
      [true, "Expand the sidebar"],
      [false, "Collapse the sidebar"],
    ] as const) {
      show({ collapsed });
      const pages = await screen.findByRole("navigation", { name: "Pages" });
      const head = pages.firstElementChild;
      const toggle = screen.getByRole("button", { name });
      expect(head?.contains(toggle)).toBe(true);
      expect(pages.lastElementChild?.contains(toggle)).toBe(false);
      const height = [...(head?.classList ?? [])].filter((each) => each.startsWith("h-"));
      expect(height).toHaveLength(1);
      asked ??= height;
      expect(height).toEqual(asked);
      // The rail's head holds the mark as well, which is where the connection's words live.
      if (collapsed) {
        expect(head?.querySelector("[data-testid='connection']")).not.toBeNull();
      }
      cleanup();
    }
  });

  it("says the connection rather than only colouring a dot", async () => {
    show({ collapsed: true });
    await screen.findByRole("button", { name: "Expand the sidebar" });
    expect(screen.getAllByText("Reconnecting")).toHaveLength(1);
    // The rail has no line of words under the name, so the mark itself carries them.
    expect((await screen.findByTestId("connection")).textContent).toContain("Reconnecting");
  });
});

describe("the pins and recent places", () => {
  const places = {
    favourites: [
      { id: 1, kind: "project", repository_id: 1, key: "", title: "moebius-platform" },
      { id: 2, kind: "card", repository_id: 1, key: "7", title: "Fix the login" },
    ],
    recent: [
      { id: 5, kind: "card", repository_id: 2, key: "9", title: "Write notes" },
      { id: 6, kind: "card", repository_id: 1, key: "7", title: "Fix the login" },
    ],
  };

  it("lists the pins with the means to move and remove them, and the recent places not pinned yet", async () => {
    ROUTES["/v1/places"] = places;
    try {
      show();
      await screen.findByRole("link", { name: "Fix the login" });
      expect(screen.getByRole("button", { name: "Move moebius-platform up" })).toHaveProperty(
        "disabled",
        true,
      );
      expect(screen.getByRole("button", { name: "Move Fix the login down" })).toHaveProperty(
        "disabled",
        true,
      );
      expect(screen.getByRole("button", { name: "Unpin Fix the login" })).toBeTruthy();
      // A place that is pinned is not offered for pinning again.
      expect(screen.getAllByRole("link", { name: "Fix the login" })).toHaveLength(1);
      expect(screen.getByRole("button", { name: "Pin Write notes" })).toBeTruthy();
    } finally {
      delete ROUTES["/v1/places"];
    }
  });

  it("shows neither list in the rail", async () => {
    ROUTES["/v1/places"] = places;
    try {
      show({ collapsed: true });
      // The rail has read the snapshot once its board row is there; by then the places have been asked for too.
      await screen.findByRole("link", { name: /^Board/ });
      expect(screen.queryByRole("link", { name: "Write notes" })).toBeNull();
    } finally {
      delete ROUTES["/v1/places"];
    }
  });
});
