// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import { WikiPage } from "./WikiPage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function show(role: string, path: string) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const body = url.includes("/v1/wiki/pages") ? [{ path: "Home", name: "Home" }] : [];
      return { ok: true, status: 200, text: async () => JSON.stringify(body) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(keys.session, { signed_in: true, person: { handle: "me", role } });
  render(
    <QueryClientProvider client={client}>
      <RouterProvider
        router={createMemoryRouter([{ path: "*", element: <WikiPage /> }], {
          initialEntries: [path],
        })}
      />
    </QueryClientProvider>,
  );
}

describe("a new page of the wiki", () => {
  it("has no form for a viewer who opens the address of one", async () => {
    show("viewer", "/wiki?new=");
    await screen.findByRole("heading", { name: "Wiki" });
    await screen.findAllByText("Home");
    expect(screen.queryByRole("form", { name: /new page/i })).toBeNull();
    expect(screen.queryByLabelText(/name of the page|page name|title/i)).toBeNull();
  });

  it("has the form for a member", async () => {
    show("member", "/wiki?new=");
    expect(await screen.findByLabelText(/name|title/i)).toBeTruthy();
  });
});
