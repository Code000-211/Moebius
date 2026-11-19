/**
 * The Rules page as a list and a detail: a row for each set of rules, the chosen set in the address and in the middle,
 * and its facts in the right column.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RulesPage } from "./RulesPage";

const VIEW = {
  patterns: [{ name: "no-force-push", description: "Never force-push." }],
  prose: {
    source: "harness",
    editable: true,
    text: "## Be careful\nAsk first.\n",
    rules: [{ name: "Be careful", text: "Ask first." }],
    error: null,
  },
  repositories: [
    {
      repository: "api",
      repository_id: 7,
      text: "## Tests\nRun them.\n",
      rules: [{ name: "Tests", text: "Run them." }],
      error: null,
    },
  ],
  refusals: { "no-force-push": 2 },
};

function Where() {
  return <span data-testid="where">{useLocation().search}</span>;
}

function show(initial = "/rules") {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: true, status: 200, text: async () => JSON.stringify(VIEW) })),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={[initial]}>
        <RulesPage />
        <Where />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("the rules as a list and a detail", () => {
  it("lists a row for each set of rules and opens the first", async () => {
    show();
    const rows = await screen.findAllByTestId("list-card");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("Built-in patterns"),
      expect.stringContaining("Server rules"),
      expect.stringContaining("api"),
    ]);
    expect(rows[0]?.getAttribute("aria-current")).toBe("true");
    expect(screen.getByText("no-force-push")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Properties of Built-in patterns" })).toBeTruthy();
  });

  it("puts the chosen set in the address and shows it", async () => {
    show();
    fireEvent.click(await screen.findByRole("button", { name: /Server rules/ }));
    expect(screen.getByTestId("where").textContent).toBe("?set=server");
    expect(await screen.findByText("Be careful")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Properties of Server rules" })).toBeTruthy();
  });

  it("opens the set the address names", async () => {
    show("/rules?set=7");
    expect(await screen.findByText("Run them.")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Properties of api" })).toBeTruthy();
  });
});
