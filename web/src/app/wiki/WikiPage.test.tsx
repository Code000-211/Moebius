// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import { WikiPage } from "./WikiPage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const TEXT = "# Home\n\nWelcome.\n\n## Rotation\n\nWeekly.\n\n### Numbers\n\nIn the vault.\n";

function show(role: string, path: string) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const version = {
        commit: "abc1234",
        author: "Grace",
        at: "2026-11-01T10:00:00Z",
        message: "Add",
      };
      const body = url.includes("/v1/wiki/links/")
        ? { links: { pages: [], cards: [] }, backlinks: { pages: [], cards: [] } }
        : /\/v1\/wiki\/pages\/Home/.test(url)
          ? { path: "Home", name: "Home", text: TEXT, version }
          : url.includes("/v1/wiki/pages")
            ? [{ path: "Home", name: "Home" }]
            : [];
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

describe("the outline of a page", () => {
  it("lists the headings of the open page inside the page, without its title, and no properties pane", async () => {
    show("member", "/wiki?page=Home");
    const outline = await screen.findByRole("navigation", { name: "On this page" });
    const names = within(outline)
      .getAllByRole("button")
      .map((button) => button.textContent);
    expect(names).toEqual(["Rotation", "Numbers"]);
    expect(screen.queryByRole("region", { name: /^Properties of/ })).toBeNull();
    expect(screen.queryByText("Edited by")).toBeNull();
  });
});
