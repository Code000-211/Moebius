// @vitest-environment jsdom
/**
 * What the sheet says when the server will not make the card: the refusal as it came, and — when the refusal is a
 * forge the server was never set up for — the way to set it up. A person told a forge is missing should not have to
 * work out on their own which page sets it.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NewIssueDialog } from "./NewIssueDialog";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** The sheet, opened, with the title typed and Create pressed against a server that answers `message`. */
async function refused(message: string) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: false, status: 503, text: async () => message })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <NewIssueDialog
          repository={1}
          name="travel-timeline"
          open
          onOpenChange={() => {}}
          onCreated={() => {}}
        />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  fireEvent.change(screen.getByLabelText("Title"), { target: { value: "security" } });
  fireEvent.click(screen.getByRole("button", { name: "Create" }));
  return screen.findByRole("alert");
}

describe("the refusal of a new issue", () => {
  it("carries the way to set up a forge the server was never told about", async () => {
    const line = await refused("travel-timeline needs the forge github, which is not set up");
    expect(line.textContent).toContain("needs the forge github, which is not set up");
    const link = within(line).getByRole("link", { name: "Set up GitHub" });
    expect(link.getAttribute("href")).toBe("/settings");
  });

  it("offers no way to set anything up when the refusal is about something else", async () => {
    const line = await refused("the title is already taken by issue 12");
    expect(line.textContent).toContain("already taken by issue 12");
    expect(within(line).queryByRole("link")).toBeNull();
    expect(within(line).queryByText(/Set up/)).toBeNull();
  });
});
