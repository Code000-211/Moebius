// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import { ProfilePage } from "./ProfilePage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const lea = { id: 2, handle: "lea", name: "Lea", role: "member", disabled: false, created_at: 1 };

function show(ui: React.ReactNode, seed: Array<[readonly unknown[], unknown]> = [], routes = {}) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input), "http://x").pathname;
      if (init?.method === "DELETE") return new Response(null, { status: 204 });
      return new Response(JSON.stringify((routes as Record<string, unknown>)[path] ?? []), {
        headers: { "content-type": "application/json" },
      });
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  for (const [key, data] of seed) client.setQueryData(key, data);
  return {
    client,
    ...render(
      <QueryClientProvider client={client}>
        <MemoryRouter>{ui}</MemoryRouter>
      </QueryClientProvider>,
    ),
  };
}

describe("Profile", () => {
  it("shows the signed-in person and signs out from the page", async () => {
    show(<ProfilePage />, [[keys.session, { signed_in: true, person: lea }]]);
    expect(screen.getByText("Lea")).toBeTruthy();
    expect(screen.getByText("@lea")).toBeTruthy();
    expect(screen.getByText("member")).toBeTruthy();
    screen.getByRole("button", { name: "Sign out" }).click();
    await screen.findByRole("button", { name: "Signing out" });
    const calls = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
    expect(
      calls.some(([url, init]) => String(url).includes("/v1/session") && init?.method === "DELETE"),
    ).toBe(true);
  });
});
