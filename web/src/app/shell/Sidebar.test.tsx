/**
 * What the sidebar owes a person: the words the wide sidebar shows next to a name — a board's name, its work, the
 * connection — stay reachable when it collapses to a rail of icons, and the mark is where the app is recognised from
 * (`docs/design-web.md`: status is never colour alone, icon-only controls get accessible names, and a row is the same
 * height open or collapsed). Exactly one entry says which page is on screen, Chat leads to a conversation rather than
 * to an empty field, and the way out of the session is an item of the Settings list rather than a foot on every page.
 * The look of the app is chosen there too, not here.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Chat } from "../../api/types/chat";
import { SETTINGS_PAGES, settingsLabel } from "../pages";
import { Sidebar } from "./Sidebar";

/** The pages inside the two collapsible groups, in the order the sidebar lists them. */
const AUTOMATION_PAGES: readonly string[] = ["Automations", "Workflows"];
const IMPROVE_PAGES: readonly string[] = ["Retro", "Experiments", "Decisions", "Usage"];

/** The keys the open/closed state of each group is kept under, like `moebius.sidebar.collapsed` is. */
const GROUP_KEYS = {
  Automation: "moebius.sidebar.group.automation",
  Improve: "moebius.sidebar.group.improve",
};

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

/** A board with `name`, with a running card for each of `working`. */
function board(id: number, name: string, working: number) {
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

const ROUTES: Record<string, unknown> = {
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
function chat(id: number, startedAt: number, over: Partial<Chat> = {}) {
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
    ...over,
  };
}

function show({
  collapsed = false,
  at = "/inbox",
  chats = [],
}: {
  collapsed?: boolean;
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

describe("the wide sidebar", () => {
  it("keeps a name that is too long to fit reachable", async () => {
    show();
    // The name of the link is its whole row: the board's name, then the words about its work.
    const link = await screen.findByRole("link", { name: /^moebius-platform/ });
    const label = link.querySelector("span[title='moebius-platform']");
    expect(label?.textContent).toBe("moebius-platform");
  });

  /**
   * The state most boards are in is the mark alone: "Idle" printed on every row of a quiet list is the word nobody
   * reads, and the list of repositories is not a list of states. The word goes, the mark takes its place at the end of
   * the row, and the name of the row keeps the word for whoever cannot take the colour in.
   */
  it("marks a board that is waiting rather than printing Idle on its row", async () => {
    show();
    const idle = await screen.findByRole("link", { name: "notes. Idle" });
    // The row shows a name and a mark; the word the mark stands for is in the name of the link, which is where it
    // stays when the colour does not carry, and is not printed under every row of a list that is all waiting.
    expect(idle.getAttribute("aria-label")).toBe("notes. Idle");
    expect(idle.textContent).toBe("notes");
    expect(idle.querySelector("span[aria-hidden='true']")).not.toBeNull();
  });

  it("counts a board's live work beside its mark, in a number alone", async () => {
    show();
    const live = await screen.findByRole("link", { name: "moebius-platform. 2" });
    expect(live.textContent).toBe("moebius-platform2");
    expect(live.querySelector("span.text-progress-text")?.textContent).toBe("2");
  });

  it("names the page switcher by what it does, and says the same on screen", () => {
    show();
    const button = screen.getByRole("button", { name: "Search" });
    expect(button.textContent).toContain("Search");
  });

  it("carries the mark, so the app is recognisable at a glance", async () => {
    const { container } = show();
    await screen.findByRole("link", { name: /^moebius-platform/ });
    expect(container.querySelector("svg[viewBox='0 0 22 16']")).not.toBeNull();
  });
});

describe("the pages and the settings", () => {
  it("lists the everyday pages, with one entry for Settings", async () => {
    show();
    const pages = await screen.findByRole("navigation", { name: "Pages" });
    expect(pages.textContent).toContain("Fleet");
    expect(pages.textContent).toContain("Retro");
    // The pages that set the server up are not listed beside them.
    expect(pages.textContent).not.toContain("Models");
    expect(pages.textContent).not.toContain("MCP servers");
    expect(screen.getByRole("link", { name: "Settings" })).not.toBeNull();
  });

  it("becomes the list of settings pages on one of them, and says how to get back", async () => {
    show({ at: "/models" });
    const pages = await screen.findByRole("navigation", { name: "Pages" });
    expect(pages.textContent).toContain("General");
    expect(pages.textContent).toContain("MCP servers");
    // The everyday pages give way to the settings they were swapped for.
    expect(pages.textContent).not.toContain("Fleet");
    // The way back is a row of the list, not a control above it; the rail's tooltip carries the whole sentence.
    const back = screen.getByRole("link", { name: "Back" });
    expect(back.getAttribute("aria-label")).toBeNull();
  });

  it("gives every settings row the mark of its own page", async () => {
    show({ at: "/models" });
    await screen.findByRole("navigation", { name: "Pages" });
    // The collapsed rail is nothing but these marks, so a row without one is a row nobody can find.
    for (const page of SETTINGS_PAGES) {
      const row = screen.getByRole("link", { name: settingsLabel(page) });
      expect(row.querySelector("svg"), page.path).not.toBeNull();
    }
  });

  it("carries the way out of this browser as an item of its own list", async () => {
    show({ at: "/models" });
    const pages = await screen.findByRole("navigation", { name: "Pages" });
    expect(pages.textContent).toContain("General");
    // Signing out does something rather than going somewhere, so it is a row of the list and not a page.
    expect(screen.getByRole("button", { name: /Sign out/ })).not.toBeNull();
  });

  it("keeps the way out with the settings, off the everyday list", async () => {
    show({ at: "/inbox" });
    await screen.findByRole("link", { name: /^Inbox/ });
    expect(screen.queryByRole("button", { name: /Sign out/ })).toBeNull();
  });

  it("goes back to the page the person was last on", async () => {
    show({ at: "/fleet" });
    await screen.findByRole("link", { name: /^Fleet/ });
    // Entering Settings from Fleet, back returns to Fleet rather than to the first page.
    fireEvent.click(screen.getByRole("link", { name: "Settings" }));
    const pages = await screen.findByRole("navigation", { name: "Pages" });
    expect(pages.textContent).toContain("General");
    fireEvent.click(screen.getByRole("link", { name: "Back" }));
    await screen.findByRole("link", { name: /^Fleet/ });
  });
});

/**
 * Which entry says "you are here", and where Chat leads: since the chat list lives in the sidebar, the wide list leaves
 * Chat to the rail, whose single icon still opens a conversation rather than an empty field — the newest of them, with
 * the unread trail on its name.
 */
/**
 * The two collapsible groups. Automation holds Automations and Workflows; Improve holds Retro, Experiments,
 * Decisions and Usage. Both are open before anybody has chosen anything, the choice is kept per browser like the
 * sidebar's own collapsed state is, and the rail — which has no room for a name on its group labels — never
 * collapses them: there the groups behave as they always did, the label's room kept and every row under it.
 * The group's name itself goes nowhere (`docs/design-web.md`); the caret beside it is the toggle.
 */
describe("the collapsible groups", () => {
  it("holds its pages, and is open before anybody has chosen anything", async () => {
    show();
    const automation = screen.getByRole("button", { name: "Collapse Automation" });
    expect(automation.getAttribute("aria-expanded")).toBe("true");
    for (const page of AUTOMATION_PAGES) {
      expect(await screen.findByRole("link", { name: new RegExp(`^${page}`) })).not.toBeNull();
    }
    const improve = screen.getByRole("button", { name: "Collapse Improve" });
    expect(improve.getAttribute("aria-expanded")).toBe("true");
    for (const page of IMPROVE_PAGES) {
      expect(screen.getByRole("link", { name: new RegExp(`^${page}`) })).not.toBeNull();
    }
    // The groups that are not touched by this stay what they were.
    expect(screen.getByRole("link", { name: /^Inbox/ })).not.toBeNull();
    expect(screen.getByRole("link", { name: /^Fleet/ })).not.toBeNull();
  });

  it("closes on its own caret, and leaves the rows beside it standing", async () => {
    show();
    await screen.findByRole("link", { name: /^Automations/ });
    fireEvent.click(screen.getByRole("button", { name: "Collapse Automation" }));
    expect(screen.queryByRole("link", { name: /^Automations/ })).toBeNull();
    expect(screen.queryByRole("link", { name: /^Workflows/ })).toBeNull();
    // The rows of the other groups are not the closed group's to take down.
    expect(screen.getByRole("link", { name: /^Inbox/ })).not.toBeNull();
    expect(
      screen.getByRole("button", { name: "Expand Automation" }).getAttribute("aria-expanded"),
    ).toBe("false");
    // The choice is kept in this browser the way the sidebar's shape is.
    expect(localStorage.getItem(GROUP_KEYS.Automation)).toBe("0");
  });

  it("opens again on the same caret", async () => {
    show();
    await screen.findByRole("link", { name: /^Retro/ });
    fireEvent.click(screen.getByRole("button", { name: "Collapse Improve" }));
    expect(screen.queryByRole("link", { name: /^Retro/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Expand Improve" }));
    expect(screen.getByRole("link", { name: /^Retro/ })).not.toBeNull();
    expect(localStorage.getItem(GROUP_KEYS.Improve)).toBe("1");
  });

  it("keeps a closed group closed the next time the sidebar is built", async () => {
    show();
    await screen.findByRole("link", { name: /^Retro/ });
    fireEvent.click(screen.getByRole("button", { name: "Collapse Improve" }));
    cleanup();
    show();
    await screen.findByRole("link", { name: /^Inbox/ });
    expect(screen.queryByRole("link", { name: /^Retro/ })).toBeNull();
    expect(screen.queryByRole("link", { name: /^Experiments/ })).toBeNull();
    // A group left open is remembered open; nothing about the state of one group touches another.
    expect(screen.getByRole("link", { name: /^Automations/ })).not.toBeNull();
  });

  it("keeps every row in the rail, whatever the groups were left as", async () => {
    localStorage.setItem(GROUP_KEYS.Automation, "0");
    localStorage.setItem(GROUP_KEYS.Improve, "0");
    show({ collapsed: true });
    // The rail's labels keep the room they always took, and carry no toggle there.
    await screen.findByRole("link", { name: /^Inbox/ });
    expect(screen.queryByRole("button", { name: "Collapse Automation" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Collapse Improve" })).toBeNull();
    for (const page of [...AUTOMATION_PAGES, ...IMPROVE_PAGES]) {
      expect(screen.getByRole("link", { name: new RegExp(`^${page}`) })).not.toBeNull();
    }
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
    const entry = await screen.findByRole("link", { name: /^notes/ });
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

describe("the list of chats", () => {
  const now = Math.floor(Date.now() / 1000);
  const DAY = 86_400;

  it("groups the chats under their day, above Projects, in the wide sidebar only", async () => {
    show({
      at: "/inbox",
      chats: [chat(9, now - 60), chat(4, now - DAY - 60), chat(2, now - 3 * DAY - 60)],
    });
    // The list stands above the Projects one, its groups in the fixed order of the days.
    const labels = await screen.findAllByTestId("chat-group-label");
    expect(labels.map((label) => label.textContent)).toEqual(["Today", "Yesterday", "Older"]);
  });

  it("orders the groups itself, the days fixed and the newest chat first within one", async () => {
    const today = chat(5, now - 10);
    const older = chat(6, now - 3 * DAY);
    show({ at: "/inbox", chats: [older, today] });
    const labels = await screen.findAllByTestId("chat-group-label");
    expect(labels[0]?.textContent).toBe("Today");
    expect(labels[1]?.textContent).toBe("Older");
  });

  it("opens a chat on its row, and marks the one on screen", async () => {
    show({ at: "/chat/9", chats: [chat(9, now - 60), chat(4, now - DAY)] });
    const rows = await screen.findAllByTestId("chat-entry");
    const active = rows.find((row) => row.getAttribute("aria-current"));
    expect(active?.getAttribute("href")).toBe("/chat/9");
    expect(rows).toHaveLength(2);
    expect(rows.map((row) => row.getAttribute("href"))).toEqual(["/chat/9", "/chat/4"]);
  });

  it("carries an unread dot for a chat with an answer nobody has read", async () => {
    show({
      at: "/inbox",
      chats: [chat(9, now - 60, { answered: 4, seen: 2 }), chat(4, now - DAY)],
    });
    const rows = await screen.findAllByTestId("chat-entry");
    const unread = rows.find((row) => row.getAttribute("href") === "/chat/9");
    expect(unread?.getAttribute("data-unread")).toBe("true");
    expect(unread?.textContent).toContain("New answers");
    const read = rows.find((row) => row.getAttribute("href") === "/chat/4");
    expect(read?.getAttribute("data-unread")).toBeNull();
  });

  it("sits a compact way to a new chat above the list", async () => {
    show({ at: "/chat" });
    const fresh = await screen.findByRole("link", { name: "Start a new chat" });
    expect(fresh.getAttribute("href")).toBe("/chat");
  });

  it("names the place a chat works in, and keeps the rail with its one Chat entry", async () => {
    show({ collapsed: true, at: "/inbox", chats: [chat(9, now - 60)] });
    await screen.findByRole("link", { name: /^Chat/ });
    // The rail is a rail of pages: no list of conversations, no group of days.
    expect(screen.queryByRole("link", { name: "Work", current: "true" })).toBeNull();
    expect(screen.queryByTestId("chat-group-label")).toBeNull();
  });

  it("names the place a chat sits in", async () => {
    show({
      at: "/inbox",
      chats: [chat(9, now - 60, { repository_id: 1 })],
    });
    const entry = await screen.findByTestId("chat-entry");
    expect(entry.textContent).toContain("moebius-platform");
  });
});

/**
 * The compact dropdown beside "New chat" narrows the list by project: All projects, one per repository of the
 * snapshot, and Without project for the chats whose place is the harness configuration repository or nowhere the
 * snapshot can name. The choice is kept in this browser, and it filters the list itself.
 */
describe("the filter of the list of chats", () => {
  const now = Math.floor(Date.now() / 1000);

  /** The dropdown of the list of chats, read by the accessible name of its trigger. */
  const trigger = () => screen.getByRole("button", { name: "Filter chats by project" });
  // Radix menus open on a pointer press and commit a choice on its release, not on a click.
  const choose = async (name: string) => {
    fireEvent.pointerDown(trigger());
    const item = await screen.findByRole("menuitemradio", { name });
    fireEvent.pointerUp(item);
    // Committing a choice closes the menu.
    expect(screen.queryByRole("menu", { hidden: false })).toBeNull();
  };

  it("offers All projects, the repositories of the snapshot, and Without project", async () => {
    show({ at: "/inbox", chats: [chat(9, now - 60)] });
    await screen.findAllByTestId("chat-entry");
    fireEvent.pointerDown(trigger());
    const choices = await screen.findAllByRole("menuitemradio");
    expect(choices.map((choice) => choice.textContent)).toEqual([
      "All projects",
      "moebius-platform",
      "notes",
      "Without project",
    ]);
    // The choice stands on its item, and is named on the trigger.
    expect(choices[0]?.getAttribute("data-state")).toBe("checked");
    expect(trigger().textContent).toContain("All projects");
  });

  it("filters the list to the repository chosen", async () => {
    show({
      at: "/inbox",
      chats: [chat(9, now - 60, { repository_id: 1 }), chat(4, now - 60, { repository_id: 2 })],
    });
    await screen.findAllByTestId("chat-entry");
    await choose("notes");
    const rows = screen.getAllByTestId("chat-entry");
    expect(rows.map((row) => row.getAttribute("href"))).toEqual(["/chat/4"]);
    expect(trigger().textContent).toContain("notes");
  });

  it("puts the harness repository and the chats with no place under Without project", async () => {
    show({
      at: "/inbox",
      chats: [
        chat(9, now - 60, { harness_repository: true }),
        chat(4, now - 60),
        chat(2, now - 60, { repository_id: 1 }),
      ],
    });
    await screen.findAllByTestId("chat-entry");
    await choose("Without project");
    const rows = screen.getAllByTestId("chat-entry");
    expect(rows.map((row) => row.getAttribute("href"))).toEqual(["/chat/9", "/chat/4"]);
  });

  it("keeps the chosen filter for the next visit of this browser", async () => {
    show({ at: "/inbox", chats: [chat(9, now - 60, { repository_id: 1 })] });
    await screen.findAllByTestId("chat-entry");
    await choose("moebius-platform");
    // The filter is remembered not as a repository name, but as its id: a renamed repository keeps working.
    expect(localStorage.getItem("moebius.sidebar.chatsRepository")).toBe("1");
    cleanup();
    show({ at: "/inbox", chats: [chat(9, now - 60, { repository_id: 1 })] });
    await screen.findAllByTestId("chat-entry");
    expect(trigger().textContent).toContain("moebius-platform");
  });

  it("falls back to every chat when what is stored names no repository", async () => {
    localStorage.setItem("moebius.sidebar.chatsRepository", "gone");
    show({ at: "/inbox", chats: [chat(9, now - 60)] });
    await screen.findAllByTestId("chat-entry");
    expect(trigger().textContent).toContain("All projects");
    expect(screen.getAllByTestId("chat-entry")).toHaveLength(1);
  });
});

describe("the rail", () => {
  it("keeps a board's work in the name of its icon", async () => {
    show({ collapsed: true });
    await screen.findByRole("link", { name: "moebius-platform. 2" });
    await screen.findByRole("link", { name: "notes. Idle" });
  });

  it("names a board's work where its words are gone", async () => {
    show({ collapsed: true });
    const idle = await screen.findByRole("link", { name: "notes. Idle" });
    expect(idle.getAttribute("aria-label")).toBe("notes. Idle");
    // The rail draws the icon alone — and the dot beside it is decoration, so the words are what say a board is idle.
    expect(idle.textContent).toBe("");
    expect(idle.querySelector("svg")).not.toBeNull();
    expect(idle.querySelector("span[aria-hidden='true']")).not.toBeNull();
  });

  it("keeps the shape of a row when a tooltip wraps it", async () => {
    show({ collapsed: true });
    const inbox = await screen.findByRole("link", { name: /^Inbox/ });
    const row = inbox.getAttribute("class") ?? "";
    expect(row).toContain("min-h-10");
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
