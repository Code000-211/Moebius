// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../../api/types/projects";
import { MilestonesTab } from "./MilestonesTab";

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
  cards: 3,
  done: 2,
  projects: [
    { id: 1, name: "Shop", key: "SHOP", cards: 2, done: 1 },
    { id: 2, name: "Blog", key: "BLOG", cards: 1, done: 1 },
  ],
};
// A goal that only the other project's cards are in, and one nobody put a card in.
const elsewhere = {
  ...milestone,
  id: 5,
  name: "Redesign",
  cards: 1,
  done: 0,
  projects: [{ id: 2, name: "Blog", key: "BLOG", cards: 1, done: 0 }],
};
const empty = { ...milestone, id: 6, name: "Unused", cards: 0, done: 0, projects: [] };

const project = (id: number): Project =>
  ({
    id,
    name: id === 1 ? "Shop" : "Blog",
    key: id === 1 ? "SHOP" : "BLOG",
    created_at: 0,
    private: false,
    repositories: [{ id: 1, name: "core", home: true }],
    members: [],
    open_cards: 0,
    files: null,
    kinds: [],
    intake: false,
    member: true,
  }) as Project;
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
    {
      id: 3,
      project_id: 2,
      repository_id: 2,
      number: 1,
      title: "Other",
      column: "Done",
      done: true,
    },
  ],
};

function Where() {
  return <output data-testid="where">{useLocation().search}</output>;
}

function show(path = "/projects/1/milestones", of = 1) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const body =
        url === "/v1/milestones"
          ? [milestone, elsewhere, empty]
          : url.startsWith("/v1/milestones/")
            ? detail
            : [];
      return { ok: true, status: 200, text: async () => JSON.stringify(body) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[path]}>
        <MilestonesTab project={project(of)} onTerminal={() => {}} />
        <Where />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the Milestones tab", () => {
  it("shows each milestone as a bar with its progress on one axis, with no raised card", async () => {
    show();
    const row = (await screen.findByText("Launch")).closest("li") as HTMLElement;
    const bar = within(row).getByRole("progressbar");
    expect(bar.getAttribute("aria-valuenow")).toBe("50");
    expect(bar.getAttribute("aria-label")).toBe("1 of 2 cards");
    expect(row.className).not.toMatch(/shadow|rounded/);
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("keeps the opened milestone in the address, and opens it again from there", async () => {
    show();
    fireEvent.click(await screen.findByRole("button", { name: /Launch/ }));
    await screen.findByText("everything");
    expect(screen.getByTestId("where").textContent).toBe("?milestone=4");
    cleanup();
    show("/projects/1/milestones?milestone=4");
    await screen.findByText("everything");
  });

  it("draws the description as Markdown and puts Edit and Delete above the cards", async () => {
    show("/projects/1/milestones?milestone=4");
    const strong = await screen.findByText("everything");
    expect(strong.tagName).toBe("STRONG");
    expect(screen.queryByText(/\*\*everything\*\*/)).toBeNull();
    const edit = screen.getByRole("button", { name: "Edit" });
    const cards = await screen.findByRole("list", { name: "Cards of Launch" });
    await waitFor(() => expect(within(cards).getByText("One")).toBeTruthy());
    // Edit comes before the list of cards in the page.
    expect(edit.compareDocumentPosition(cards) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("lists only the milestones that hold cards of this project, with this project's progress", async () => {
    show();
    await screen.findByText("Launch");
    expect(screen.queryByText("Redesign")).toBeNull();
    expect(screen.queryByText("Unused")).toBeNull();
    cleanup();
    show("/projects/2/milestones", 2);
    await screen.findByText("Redesign");
    const launch = screen.getByText("Launch").closest("li") as HTMLElement;
    // Of its three cards, one is in Blog, and it is done.
    expect(within(launch).getByRole("progressbar").getAttribute("aria-label")).toBe("1 of 1 card");
    expect(screen.queryByText("Unused")).toBeNull();
  });

  it("lists only this project's cards under an opened milestone", async () => {
    show("/projects/1/milestones?milestone=4");
    const cards = await screen.findByRole("list", { name: "Cards of Launch" });
    await waitFor(() => expect(within(cards).getByText("One")).toBeTruthy());
    expect(within(cards).queryByText("Other")).toBeNull();
  });

  it("has no project filter any more", async () => {
    show();
    await screen.findByText("Launch");
    expect(screen.queryByRole("button", { name: /Filter/ })).toBeNull();
  });
});
