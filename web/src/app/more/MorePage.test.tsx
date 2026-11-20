import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { SETTINGS_ENTRIES } from "../settings/settingsEntries";
import { MORE_PATHS, MorePage, morePages } from "./MorePage";

const OWNER = MORE_PATHS.map((path) => path);

describe("the More page", () => {
  it("lists exactly the decided pages for an owner, in order", () => {
    expect(morePages("owner").map((page) => page.path)).toEqual(OWNER);
    expect(morePages("owner").map((page) => page.title)).toEqual([
      "Milestones",
      "Intake",
      "Chat",
      "Terminal",
      "Usage",
      "Automations",
      "Experiments",
      "Decisions",
      "Kinds of issues",
      "My tokens",
      "Ports",
      "Plugin settings",
      "Get started",
    ]);
  });

  it("leaves the owner-only pages out for a member", () => {
    const paths = morePages("member").map((page) => page.path);
    expect(paths).not.toContain("kinds");
    expect(paths).toHaveLength(OWNER.length - 1);
  });

  it("names no page that Settings has an entry for", () => {
    const entries = new Set(SETTINGS_ENTRIES.map((entry) => entry.path));
    for (const path of MORE_PATHS) expect(entries.has(path)).toBe(false);
  });

  it("draws one link per page to its path", () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <MemoryRouter>
          <MorePage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const link = screen.getByRole("link", { name: "Ports" });
    expect(link.getAttribute("href")).toBe("/ports");
  });
});
