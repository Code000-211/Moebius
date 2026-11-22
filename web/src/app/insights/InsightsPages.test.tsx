// @vitest-environment jsdom
/** `?project=` preselects a project on both pages of Insights, and the pages carry the tabs of Insights and no others. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AnalyticsPage } from "../analytics/AnalyticsPage";
import { UsagePage } from "../usage/UsagePage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function open(ui: React.ReactNode, at: string) {
  const urls: string[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      urls.push(String(input));
      return new Response("{}", { status: 500, headers: { "content-type": "application/json" } });
    }),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={[at]}>{ui}</MemoryRouter>
    </QueryClientProvider>,
  );
  return urls;
}

describe("the project of Insights", () => {
  it("narrows Analytics to the project of the address, under the Analytics and Usage tabs", async () => {
    const urls = open(<AnalyticsPage />, "/insights?project=3");
    await waitFor(() => expect(urls.some((u) => u.includes("/v1/analytics?"))).toBe(true));
    expect(urls.find((u) => u.includes("/v1/analytics?"))).toContain("repository=3");
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["Analytics", "Usage"]);
    expect(screen.getByRole("heading", { name: "Insights" })).toBeTruthy();
  });

  it("narrows Usage to the project of the address, and names the same two tabs", async () => {
    const urls = open(<UsagePage />, "/insights/usage?project=3&days=7");
    await waitFor(() => expect(urls.some((u) => u.includes("/v1/usage?"))).toBe(true));
    const read = urls.find((u) => u.includes("/v1/usage?")) ?? "";
    expect(read).toContain("repository=3");
    expect(read).toContain("days=7");
    expect(screen.getAllByRole("tab").map((t) => t.getAttribute("aria-selected"))).toEqual([
      "false",
      "true",
    ]);
  });
});
