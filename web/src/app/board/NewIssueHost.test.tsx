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

function page() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/agents"]}>
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
