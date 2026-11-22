// @vitest-environment jsdom
/**
 * The form of a new issue for the whole app: `C` opens it from any page but not while a person types in a field, and
 * "Create more" keeps it open for the next card.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import { NewIssueProvider } from "./NewIssueHost";

const snapshot = {
  boards: [
    {
      repository: { id: 1, name: "travel-timeline", forge: "github" },
      cards: [],
      round_limit: 5,
      waiting: {},
      changes: [],
    },
    {
      repository: { id: 2, name: "storefront", forge: "github" },
      cards: [],
      round_limit: 5,
      waiting: {},
      changes: [],
    },
  ],
  inbox: [],
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function stub() {
  const fetch = vi.fn(async (_url: string, init?: { method?: string }) => {
    const made = init?.method === "POST";
    if (_url === "/v1/projects") {
      const project = (id: number, repository: number) => ({
        id,
        name: `P${id}`,
        key: `P${id}`,
        repositories: [{ id: repository, name: "r", home: true }],
        files: null,
      });
      return { ok: true, status: 200, text: async () => JSON.stringify([project(5, 2)]) };
    }
    return {
      ok: true,
      status: 200,
      text: async () =>
        JSON.stringify(made ? { id: 9, number: 4, title: "A step", body: "" } : snapshot),
    };
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

function page(path = "/agents") {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[path]}>
        <NewIssueProvider>
          <input aria-label="Elsewhere" />
          <p>The page</p>
        </NewIssueProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  // The boards are read before the form has a repository to open for.
  return waitFor(() => expect(client.getQueryData(keys.snapshot)).toBeDefined());
}

describe("C", () => {
  it("opens the form from any page", async () => {
    stub();
    await page();
    fireEvent.keyDown(document.body, { key: "c" });
    expect(await screen.findByRole("dialog", { name: "New issue" })).toBeTruthy();
  });

  it("hands the focus back to where it was when Escape closes the form", async () => {
    stub();
    await page();
    const elsewhere = screen.getByLabelText("Elsewhere");
    elsewhere.focus();
    // The key is for the page: a field in focus would keep it, so the opener is a button.
    const opener = document.createElement("button");
    document.body.append(opener);
    opener.focus();
    fireEvent.keyDown(opener, { key: "c" });
    const title = await screen.findByLabelText("Title");
    fireEvent.keyDown(title, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(opener));
    opener.remove();
  });

  it("is left alone while a person types in a field", async () => {
    stub();
    await page();
    fireEvent.keyDown(screen.getByLabelText("Elsewhere"), { key: "c" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("is left alone with a modifier held", async () => {
    stub();
    await page();
    fireEvent.keyDown(document.body, { key: "c", ctrlKey: true });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("C on the All issues view filtered to a project", () => {
  it("starts with that project's repository, not the first one", async () => {
    stub();
    await page("/views/all-issues?project=5");
    // The project's repositories are read once the page is filtered.
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await new Promise((r) => setTimeout(r, 50));
    fireEvent.keyDown(document.body, { key: "c" });
    const dialog = await screen.findByRole("dialog", { name: "New issue" });
    expect(dialog.textContent).toContain("Backlog of storefront");
  });

  it("starts with the first repository when no project is filtered", async () => {
    stub();
    await page("/views/all-issues");
    fireEvent.keyDown(document.body, { key: "c" });
    const dialog = await screen.findByRole("dialog", { name: "New issue" });
    expect(dialog.textContent).toContain("Backlog of travel-timeline");
  });
});

describe("Create more", () => {
  it("keeps the form open, empty, for the next card", async () => {
    const fetch = stub();
    await page();
    fireEvent.keyDown(document.body, { key: "c" });
    const dialog = await screen.findByRole("dialog", { name: "New issue" });
    fireEvent.click(screen.getByLabelText("Create more"));
    const title = screen.getByLabelText("Title") as HTMLInputElement;
    fireEvent.change(title, { target: { value: "A step" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(screen.getByText(/Created #4/)).toBeTruthy());
    expect(screen.getByRole("dialog", { name: "New issue" })).toBe(dialog);
    expect((screen.getByLabelText("Title") as HTMLInputElement).value).toBe("");
    expect(fetch.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(1);
  });

  it("closes the form after the card when it is not chosen", async () => {
    stub();
    await page();
    fireEvent.keyDown(document.body, { key: "c" });
    await screen.findByRole("dialog", { name: "New issue" });
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "A step" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
