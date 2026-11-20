// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProjectsPage } from "./ProjectsPage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const project = (id: number, name: string, key: string) => ({
  id,
  name,
  key,
  private: false,
  repositories: [{ id, name, home: true }],
  members: [{ id: 1, handle: "lea", name: "Lea" }],
  open_cards: 3,
  files: null,
  intake: false,
});

describe("the Projects page", () => {
  it("lists each project as one row that opens its settings, with no raised tile", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        status: 200,
        text: async () => JSON.stringify([project(1, "shop", "SHOP"), project(2, "web", "WEB")]),
      })),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { container } = render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ProjectsPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const rows = await screen.findAllByTestId("project");
    expect(rows).toHaveLength(2);
    const link = screen.getByRole("link", { name: "shop, SHOP" });
    expect(link.getAttribute("href")).toBe("/projects/1/settings");
    expect(link.className).toContain("h-9");
    expect(container.innerHTML).not.toContain("shadow-card");
    expect(container.querySelector(".rounded-card")).toBeNull();
  });
});
