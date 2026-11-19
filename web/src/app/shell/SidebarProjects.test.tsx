/**
 * The collapsible groups and the project switcher of the sidebar. Automate holds Automations and Workflows, with Improve
 * folded over Retro, Experiments and Decisions; the choice is kept per browser, and the rail never folds a group. The
 * switcher shows the places of the chosen project, which the address and this browser decide.
 */
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GROUP_KEYS, show } from "./SidebarTestHelpers";

/** The pages of Automate, and those inside Improve, in the order the sidebar lists them. */
const AUTOMATION_PAGES: readonly string[] = ["Automations", "Workflows"];
const IMPROVE_PAGES: readonly string[] = ["Retro", "Experiments", "Decisions"];

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe("Improve and the places of Automate", () => {
  it("lists Automate with Improve folded open over its pages before anybody has chosen anything", async () => {
    show();
    for (const page of AUTOMATION_PAGES) {
      expect(await screen.findByRole("link", { name: new RegExp(`^${page}`) })).not.toBeNull();
    }
    const improve = screen.getByRole("button", { name: "Collapse Improve" });
    expect(improve.getAttribute("aria-expanded")).toBe("true");
    for (const page of IMPROVE_PAGES) {
      expect(screen.getByRole("link", { name: new RegExp(`^${page}`) })).not.toBeNull();
    }
    expect(screen.getByRole("link", { name: /^Inbox/ })).not.toBeNull();
    expect(screen.getByRole("link", { name: /^Fleet/ })).not.toBeNull();
  });

  it("stands in the order of the drafts", async () => {
    show();
    await screen.findByRole("link", { name: /^Board/ });
    const order = [
      screen.getByRole("link", { name: /^Inbox/ }),
      screen.getByRole("link", { name: /^Fleet/ }),
      screen.getByRole("button", { name: "Change project" }),
      screen.getByRole("link", { name: /^Board/ }),
      screen.getByRole("link", { name: /^Files/ }),
      screen.getByRole("link", { name: /^Automations/ }),
      screen.getByRole("link", { name: /^Workflows/ }),
      screen.getByRole("button", { name: "Collapse Improve" }),
      screen.getByRole("link", { name: /^Retro/ }),
    ];
    for (let each = 1; each < order.length; each += 1) {
      const before = order[each - 1] as HTMLElement;
      const after = order[each] as HTMLElement;
      expect(before.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
  });

  it("closes Improve on its caret, and leaves the rows beside it standing", async () => {
    show();
    await screen.findByRole("link", { name: /^Retro/ });
    fireEvent.click(screen.getByRole("button", { name: "Collapse Improve" }));
    for (const page of IMPROVE_PAGES) {
      expect(screen.queryByRole("link", { name: new RegExp(`^${page}`) })).toBeNull();
    }
    expect(screen.getByRole("link", { name: /^Automations/ })).not.toBeNull();
    expect(screen.getByRole("link", { name: /^Inbox/ })).not.toBeNull();
    expect(
      screen.getByRole("button", { name: "Expand Improve" }).getAttribute("aria-expanded"),
    ).toBe("false");
    // The choice is kept in this browser the way the sidebar's shape is.
    expect(localStorage.getItem(GROUP_KEYS.Improve)).toBe("0");
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

  it("keeps a closed Improve closed the next time the sidebar is built", async () => {
    show();
    await screen.findByRole("link", { name: /^Retro/ });
    fireEvent.click(screen.getByRole("button", { name: "Collapse Improve" }));
    cleanup();
    show();
    await screen.findByRole("link", { name: /^Inbox/ });
    expect(screen.queryByRole("link", { name: /^Retro/ })).toBeNull();
    expect(screen.queryByRole("link", { name: /^Experiments/ })).toBeNull();
    expect(screen.getByRole("link", { name: /^Automations/ })).not.toBeNull();
  });

  it("keeps every row in the rail, whatever Improve was left as", async () => {
    localStorage.setItem(GROUP_KEYS.Improve, "0");
    show({ collapsed: true });
    await screen.findByRole("link", { name: /^Inbox/ });
    expect(screen.queryByRole("button", { name: "Collapse Improve" })).toBeNull();
    for (const page of [...AUTOMATION_PAGES, ...IMPROVE_PAGES]) {
      expect(screen.getByRole("link", { name: new RegExp(`^${page}`) })).not.toBeNull();
    }
    expect(await screen.findByRole("link", { name: /^Board/ })).not.toBeNull();
    expect(screen.getByRole("link", { name: /^Files/ })).not.toBeNull();
  });
});

describe("the project switcher", () => {
  it("shows the places of the first project until one is chosen", async () => {
    show();
    const board = await screen.findByRole("link", { name: /^Board/ });
    expect(board.getAttribute("href")).toBe("/board/1");
    expect(screen.getByRole("link", { name: "Files" }).getAttribute("href")).toBe("/projects/1");
  });

  it("follows the project the address names, and keeps it for the pages after it", async () => {
    show({ at: "/board/2" });
    const board = await screen.findByRole("link", { name: /^Board/ });
    expect(board.getAttribute("href")).toBe("/board/2");
    expect(screen.getByRole("link", { name: "Files" }).getAttribute("href")).toBe("/projects/2");
    await waitFor(() => expect(localStorage.getItem(GROUP_KEYS.Project)).toBe("2"));
    // A page that is not of a project, in a new browser load, still shows the project that was kept.
    cleanup();
    show({ at: "/fleet" });
    const again = await screen.findByRole("link", { name: /^Board/ });
    expect(again.getAttribute("href")).toBe("/board/2");
  });

  it("falls back to the first project when the one kept is gone", async () => {
    localStorage.setItem(GROUP_KEYS.Project, "99");
    show();
    const board = await screen.findByRole("link", { name: /^Board/ });
    expect(board.getAttribute("href")).toBe("/board/1");
  });

  it("marks Files on the project's page, and Board on a column's script", async () => {
    show({ at: "/projects/1" });
    const files = await screen.findByRole("link", { name: "Files" });
    expect(files.getAttribute("aria-current")).toBe("page");
    cleanup();
    show({ at: "/board/1/scripts/Todo" });
    const board = await screen.findByRole("link", { name: /^Board/ });
    expect(board.getAttribute("aria-current")).toBe("page");
  });

  it("names the switcher in the rail with the project it holds", async () => {
    show({ collapsed: true });
    expect(
      await screen.findByRole("button", { name: "Project moebius-platform. Change project" }),
    ).not.toBeNull();
  });
});
