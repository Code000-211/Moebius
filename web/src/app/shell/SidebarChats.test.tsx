/**
 * The list of chats in the wide sidebar: its groups by day, its rows and their unread dots, the dropdown that narrows it
 * by project, and the field that narrows it by title.
 */
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { chat, show } from "./SidebarTestHelpers";

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe("the list of chats", () => {
  const now = Math.floor(Date.now() / 1000);
  const DAY = 86_400;

  it("groups the chats under their day, below Projects and Improve, in the wide sidebar only", async () => {
    show({
      at: "/inbox",
      chats: [chat(9, now - 60), chat(4, now - DAY - 60), chat(2, now - 3 * DAY - 60)],
    });
    // The list stands below the boards of Projects and the rows of Improve, its groups in the fixed order of the days.
    const labels = await screen.findAllByTestId("chat-group-label");
    expect(labels.map((label) => label.textContent)).toEqual(["Today", "Yesterday", "Older"]);
  });

  it("stands the project's places and Improve above the list of chats", async () => {
    show({ at: "/inbox", chats: [chat(9, now - 60)] });
    const chats = await screen.findByText(/^Chats$/, { exact: true });
    const improve = screen.getByRole("button", { name: "Collapse Improve" });
    const board = await screen.findByRole("link", { name: /^Board/ });
    expect(improve.compareDocumentPosition(chats) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(board.compareDocumentPosition(chats) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
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
    const fresh = await screen.findByRole("link", { name: "Start a chat in the sidebar" });
    expect(fresh.getAttribute("href")).toBe("/chat");
    // The compact way is a bare `+`: the words that name it are the accessible name, never printed beside it.
    expect(fresh.textContent).toBe("");
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

describe("the chat search", () => {
  const now = Math.floor(Date.now() / 1000);
  const DAY = 86_400;

  /** The field of the list of chats, read by the accessible name it is given. */
  const field = () => screen.getByRole("searchbox", { name: "Filter the chats" });
  const type = (text: string) => fireEvent.change(field(), { target: { value: text } });

  it("narrows the list to the chats whose title holds the words typed, as they are typed", async () => {
    show({
      at: "/inbox",
      chats: [
        chat(9, now - 60, { title: "Fix the login flow" }),
        chat(4, now - DAY - 60, { title: "Write the tests" }),
      ],
    });
    await screen.findAllByTestId("chat-entry");
    type("login");
    await waitFor(() => {
      const rows = screen.getAllByTestId("chat-entry");
      expect(rows).toHaveLength(1);
      expect(rows[0]?.getAttribute("href")).toBe("/chat/9");
    });
  });

  it("says No chats match when nothing the search is given fits, and gives the list back on a clear", async () => {
    show({
      at: "/inbox",
      chats: [
        chat(9, now - 60, { title: "Fix the login flow" }),
        chat(4, now - DAY, { title: "Other" }),
      ],
    });
    await screen.findAllByTestId("chat-entry");
    type("zzz");
    await waitFor(() => {
      expect(screen.queryByTestId("chat-entry")).toBeNull();
    });
    expect(screen.getByText("No chats match", { selector: "p" })).toBeTruthy();
    type("");
    await waitFor(() => {
      expect(screen.getAllByTestId("chat-entry")).toHaveLength(2);
    });
    expect(screen.queryByText("No chats match")).toBeNull();
  });

  it("combines with the project dropdown: a chat passes only when both take it", async () => {
    show({
      at: "/inbox",
      chats: [
        chat(9, now - 60, { title: "Fix the login flow", repository_id: 1 }),
        chat(4, now - DAY, { title: "Fix the login feed of notes", repository_id: 2 }),
      ],
    });
    await screen.findAllByTestId("chat-entry");
    // The project only takes the chats of moebius-platform; from those, the words only take the first one.
    fireEvent.pointerDown(screen.getByRole("button", { name: "Filter chats by project" }));
    const item = await screen.findByRole("menuitemradio", { name: "moebius-platform" });
    fireEvent.pointerUp(item);
    type("login");
    await waitFor(() => {
      const rows = screen.getAllByTestId("chat-entry");
      expect(rows).toHaveLength(1);
      expect(rows[0]?.getAttribute("href")).toBe("/chat/9");
    });
    // The search never broadens the project filter: its other chat holds the words but not the project.
    expect(screen.queryByTestId("chat-entry")?.getAttribute("href")).not.toBe("/chat/4");
  });
});
