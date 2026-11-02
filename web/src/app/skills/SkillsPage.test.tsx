/**
 * The Skills page: the rows are a list a screen reader can count and nothing sits above them to push them out of step
 * with the card beside them, and the buttons that need a harness configuration repository say so where a keyboard
 * reaches them.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SkillsPage } from "./SkillsPage";

/** Shows the page with the skills the server lists. */
function show(skills: unknown[]) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: true, status: 200, text: async () => JSON.stringify(skills) })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/skills"]}>
        <SkillsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("the list of skills", () => {
  it("is a list a screen reader can count, with no count line above it", async () => {
    show([
      { name: "moebius", description: "Operate Moebius.", built_in: true },
      { name: "house-style", description: "How this repository writes code." },
    ]);
    const list = await screen.findByRole("list");
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    expect(within(list).getByRole("button", { name: /house-style/ })).toBeTruthy();
    // A line that counted the rows would start the column above the card beside it, so the list does the counting.
    expect(screen.queryByText(/^\d+ skills?$/)).toBeNull();
  });
});

describe("a server without a harness configuration repository", () => {
  it("says beside the buttons why they are quiet", async () => {
    show([]);
    await screen.findByTestId("harness-setup");
    expect(screen.getByRole("button", { name: "New skill" }).hasAttribute("disabled")).toBe(true);
    expect(
      screen.getByText("Needs a harness configuration repository: set it up below."),
    ).toBeTruthy();
  });
});
