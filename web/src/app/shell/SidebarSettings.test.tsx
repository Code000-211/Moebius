/**
 * The sidebar's list of settings pages and what stands at its foot: the gear to Settings on every page, the way back
 * to the page the person was on.
 */
import { fireEvent, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SETTINGS_PAGES, settingsLabel } from "../pages";
import { show } from "./SidebarTestHelpers";

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
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

  it("keeps the gear at the foot on a settings page too", async () => {
    show({ at: "/models" });
    await screen.findByRole("link", { name: "General" });
    // The everyday list is exchanged for the settings' own; the foot under it is not.
    expect(screen.getByRole("link", { name: "Settings" })).not.toBeNull();
  });

  it("draws the foot as the rail draws a row: the gear alone, its words in the name", async () => {
    show();
    const foot = await screen.findByRole("link", { name: "Settings" });
    expect(foot.textContent).toBe("");
    expect(foot.querySelector("svg")).not.toBeNull();
    // The foot says nothing about the page on screen; the list above it does the marking.
    expect(foot.getAttribute("aria-current")).toBeNull();
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
    for (const page of SETTINGS_PAGES.filter((each) => !each.ownerOnly)) {
      const row = screen.getByRole("link", { name: settingsLabel(page) });
      expect(row.querySelector("svg"), page.path).not.toBeNull();
    }
  });

  it("leaves the way out of the session to Profile, on no list of the sidebar", async () => {
    show({ at: "/models" });
    await screen.findByRole("navigation", { name: "Pages" });
    expect(screen.queryByRole("button", { name: /Sign out/ })).toBeNull();
    expect(screen.queryByText(/Signed in as/)).toBeNull();
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
