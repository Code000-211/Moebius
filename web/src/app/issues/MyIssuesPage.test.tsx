// @vitest-environment jsdom
/**
 * My issues: it asks the server for the signed-in person's issues only, groups them by status, opens a row in the
 * card window and draws a few rows of two thousand.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Card } from "../../api/types/board";
import { mockViewport } from "../../ui/page/virtualListTest";
import { card } from "../board/model/fixtures";
import { MyIssuesPage } from "./MyIssuesPage";

let restore: () => void;
beforeEach(() => {
  restore = mockViewport({ height: 600 });
});
afterEach(() => {
  cleanup();
  restore();
  vi.unstubAllGlobals();
});

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.pathname + location.search}</p>;
}

/** Shows the page for `person` against a server that answers `cards`, and returns the addresses it was asked for. */
function open(cards: Card[], person: { handle: string } | null = { handle: "lea" }) {
  const asked: string[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string) => {
      asked.push(path);
      return new Response(
        JSON.stringify(path === "/v1/session" ? { signed_in: true, person } : cards),
      );
    }),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={["/my-issues"]}>
        <Where />
        <Routes>
          <Route path="/my-issues" element={<MyIssuesPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return asked;
}

describe("the My issues page", () => {
  it("asks only for the issues given to the signed-in person and shows them grouped by status", async () => {
    const asked = open([
      card(1, { title: "Fix login", column: "In progress" }),
      card(2, { title: "Write docs", column: "Backlog" }),
    ]);
    expect(await screen.findByText("Fix login")).toBeTruthy();
    expect(asked).toContain("/v1/issues?assignee=lea");
    expect(screen.getAllByTestId("group-row").map((row) => row.textContent)).toEqual([
      "In progress",
      "Not started",
    ]);
    expect(screen.getAllByTestId("list-row")).toHaveLength(2);
  });

  it("opens the card in its window when a row is chosen", async () => {
    open([card(7, { title: "Fix login", column: "In progress" })]);
    fireEvent.click(await screen.findByText("Fix login"));
    expect(screen.getByTestId("where").textContent).toBe("/my-issues?card=7");
  });

  it("says in one line that nothing is given to the person", async () => {
    open([]);
    expect(await screen.findByText("Nothing is assigned to you")).toBeTruthy();
  });

  it("asks for nothing, and says so, when no person is signed in", async () => {
    const asked = open([], null);
    expect(await screen.findByText("Sign in as a person to see yours")).toBeTruthy();
    expect(asked.some((path) => path.startsWith("/v1/issues"))).toBe(false);
  });

  it("draws fewer than 100 rows of 2000 issues", async () => {
    open(
      Array.from({ length: 2000 }, (_, n) =>
        card(n + 1, { title: `Issue ${n + 1}`, column: n % 2 === 0 ? "Backlog" : "In review" }),
      ),
    );
    await waitFor(() => expect(screen.getAllByTestId("list-row").length).toBeGreaterThan(0));
    expect(screen.getAllByTestId("list-row").length).toBeLessThan(100);
    expect(screen.getByRole("list", { name: "My issues" }).getAttribute("data-testid")).toBe(
      "virtual-list",
    );
  });
});
