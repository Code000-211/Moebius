// @vitest-environment jsdom
/**
 * The Views page and its built-in first view "All issues": listed before the saved views, never editable or deletable,
 * and showing every issue the person may see through the query the Issues page used to ask, filters and sort included.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Card } from "../../api/types/board";
import { mockViewport } from "../../ui/page/virtualListTest";
import { card } from "../board/model/fixtures";
import { ViewsPage } from "./ViewsPage";

let restore: () => void;
beforeEach(() => {
  restore = mockViewport({ height: 600 });
});
afterEach(() => {
  cleanup();
  restore();
  vi.unstubAllGlobals();
});

const saved = {
  id: 3,
  name: "Team board",
  shared: true,
  layout: "list",
  sort: "created",
  descending: false,
  can_change: true,
  filter: {},
};

/** The page at `address` against a server that has the saved views and answers `cards`; the paths it was asked for. */
function open(address: string, cards: Card[] = []) {
  const asked: string[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string) => {
      asked.push(path);
      const body = path.startsWith("/v1/views")
        ? [saved]
        : path.startsWith("/v1/issues")
          ? cards
          : path === "/v1/snapshot"
            ? { boards: [] }
            : [];
      return new Response(JSON.stringify(body));
    }),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={[address]}>
        <Routes>
          <Route path="/views" element={<ViewsPage />} />
          <Route path="/views/all-issues" element={<ViewsPage allIssues />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return asked;
}

describe("the built-in view All issues", () => {
  it("is listed first, before the saved views", async () => {
    open("/views");
    const list = await screen.findByRole("list", { name: "Views" });
    expect(
      within(list)
        .getAllByRole("link")
        .map((link) => link.textContent),
    ).toEqual(["All issues", "Team board" + "Shared"]);
  });

  it("shows every issue, asked for as the Issues page did, and cannot be edited or deleted", async () => {
    const asked = open("/views/all-issues", [
      card(1, { title: "Login is broken" }),
      card(2, { title: "A typo" }),
    ]);
    expect(await screen.findByText("Login is broken")).toBeTruthy();
    expect(screen.getByText("A typo")).toBeTruthy();
    expect(asked).toContain("/v1/issues");
    expect(screen.getByRole("link", { name: "All issues" }).getAttribute("aria-current")).toBe(
      "page",
    );
    expect(screen.queryByRole("button", { name: /Edit|Delete/ })).toBeNull();
  });

  it("keeps the filters and the sort of the address in the query", async () => {
    const asked = open(
      "/views/all-issues?project=2&kind=Bug&field.Severity=high&sort=asc%3AEstimate",
    );
    await screen.findByText("No issue matches");
    expect(asked).toContain("/v1/issues?project=2&kind=Bug&field.Severity=high&sort=Estimate");
  });
});
