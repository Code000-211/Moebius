// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MilestonesPage } from "./MilestonesPage";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const milestone = {
  id: 4,
  name: "Launch",
  target_date: "2030-01-01",
  description: "Ship **everything** soon",
  created_at: 0,
  cards: 2,
  done: 1,
  projects: [],
};
const detail = {
  ...milestone,
  cards: [
    {
      id: 1,
      project_id: 1,
      repository_id: 1,
      number: 1,
      title: "One",
      column: "Backlog",
      done: false,
    },
    { id: 2, project_id: 1, repository_id: 1, number: 2, title: "Two", column: "Done", done: true },
  ],
};

function Where() {
  return <output data-testid="where">{useLocation().search}</output>;
}

function show(path = "/milestones") {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const body =
        url === "/v1/milestones" ? [milestone] : url.startsWith("/v1/milestones/") ? detail : [];
      return { ok: true, status: 200, text: async () => JSON.stringify(body) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[path]}>
        <MilestonesPage />
        <Where />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the Milestones page", () => {
  it("keeps the opened milestone in the address, and opens it again from there", async () => {
    show();
    fireEvent.click(await screen.findByRole("button", { name: /Launch/ }));
    await screen.findByText("everything");
    expect(screen.getByTestId("where").textContent).toBe("?milestone=4");
    cleanup();
    show("/milestones?milestone=4");
    await screen.findByText("everything");
  });

  it("draws the description as Markdown and puts Edit and Delete above the cards", async () => {
    show("/milestones?milestone=4");
    const strong = await screen.findByText("everything");
    expect(strong.tagName).toBe("STRONG");
    expect(screen.queryByText(/\*\*everything\*\*/)).toBeNull();
    const edit = screen.getByRole("button", { name: "Edit" });
    const cards = await screen.findByRole("list", { name: "Cards of Launch" });
    await waitFor(() => expect(within(cards).getByText("One")).toBeTruthy());
    // Edit comes before the list of cards in the page.
    expect(edit.compareDocumentPosition(cards) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
