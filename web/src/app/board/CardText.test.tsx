// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CardText } from "./CardText";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function show(content: string, projectKey = "STOREFRONT") {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({
      ok: true,
      status: 200,
      text: async () =>
        JSON.stringify([
          { path: "Engineering/Page 12", name: "Page 12" },
          { path: "Other/Notes", name: "Notes" },
          { path: "projects/STOREFRONT/Notes", name: "Notes" },
        ]),
    })),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter>
        <CardText content={content} projectKey={projectKey} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the text of a card", () => {
  it("links [[Page]] and [[Page|label]] to the wiki, and leaves a page that is missing as written", async () => {
    show("See [[Engineering/Page 12]], [[page 12|the page]] and [[No such page]].");
    const first = await screen.findByRole("link", { name: "Engineering/Page 12" });
    expect(first.getAttribute("href")).toBe("/wiki?page=Engineering%2FPage%2012");
    expect(screen.getByRole("link", { name: "the page" }).getAttribute("href")).toBe(
      "/wiki?page=Engineering%2FPage%2012",
    );
    expect(screen.queryByRole("link", { name: /No such page/ })).toBeNull();
    expect(document.body.textContent).toContain("[[No such page]]");
  });

  it("reads a name as the card's project does, its own folder first", async () => {
    show("[[Notes]]");
    const link = await screen.findByRole("link", { name: "Notes" });
    expect(link.getAttribute("href")).toBe("/wiki?page=projects%2FSTOREFRONT%2FNotes");
  });

  it("leaves code alone", async () => {
    show("`[[Engineering/Page 12]]`");
    await screen.findByText("[[Engineering/Page 12]]");
    expect(screen.queryByRole("link")).toBeNull();
  });
});
