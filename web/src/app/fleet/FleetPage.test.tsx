/**
 * The Fleet page when its reads fail: it says what happened and offers to try again. The empty state would say that no
 * agent is working, which is the opposite of what the person needs to know.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FleetPage } from "./FleetPage";

afterEach(() => vi.unstubAllGlobals());

describe("a fleet that could not be read", () => {
  it("says what failed and offers to try again, instead of saying nothing is working", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: false,
        status: 503,
        text: async () => "the server is starting",
      })),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <FleetPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("the server is starting");
    expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
    expect(screen.queryByText("No agent is working")).toBeNull();
  });
});
