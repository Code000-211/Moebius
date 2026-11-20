// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { KindsPage } from "./KindsPage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const KIND = { id: 1, name: "Bug", fields: [], projects: [] };
const PROJECT = {
  id: 5,
  name: "Storefront",
  key: "SF",
  created_at: 0,
  repositories: [],
  members: [],
  open_cards: 0,
  files: null,
  kinds: [],
  intake: false,
  private: false,
  member: true,
};

function show() {
  const sent: string[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      if (init?.method === "PUT") sent.push(`${url} ${init.body}`);
      // The answer to the save comes late, as a server's does.
      if (init?.method === "PUT") await new Promise((r) => setTimeout(r, 100));
      const body =
        url === "/v1/kinds"
          ? [KIND]
          : url === "/v1/projects"
            ? [PROJECT]
            : url === "/v1/templates"
              ? { editable: false, issues: [], pages: [] }
              : PROJECT;
      return { ok: true, status: 200, text: async () => JSON.stringify(body) };
    }),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter>
        <KindsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return sent;
}

describe("the Kinds page", () => {
  it("shows a ticked project at once while it is saved", async () => {
    const sent = show();
    const box = (await screen.findByLabelText("Storefront")) as HTMLInputElement;
    expect(box.checked).toBe(false);
    fireEvent.click(box);
    await waitFor(() => expect(box.checked).toBe(true));
    await waitFor(() => expect(sent).toEqual(['/v1/projects/5/kinds {"kinds":["Bug"]}']));
  });

  it("says a kind has unsaved changes once its form differs from what is stored", async () => {
    show();
    await screen.findByText("Bug");
    const form = (await screen.findByRole("button", { name: "Save kind" })).closest("form");
    expect(form?.textContent).not.toContain("Unsaved changes");
    fireEvent.click(screen.getAllByRole("button", { name: "Add a field" })[1] as HTMLElement);
    await waitFor(() => expect(form?.textContent).toContain("Unsaved changes"));
  });
});
