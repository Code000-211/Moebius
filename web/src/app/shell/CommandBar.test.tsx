/**
 * The one term the page switcher uses: the dialog, the field of the query and the list of what it offers say the same
 * thing, in the words the sidebar's button uses (`docs/design-web.md`: one term per concept, sentence case).
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandBar } from "./CommandBar";

afterEach(() => vi.unstubAllGlobals());

// jsdom has neither, and the list of results measures itself and scrolls the choice into view.
Element.prototype.scrollIntoView = () => {};

function open() {
  // jsdom has none, and the list of results measures itself with one.
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({
      ok: true,
      status: 200,
      text: async () => JSON.stringify({ boards: [], harness_changes: [], inbox: [] }),
    })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <CommandBar open onOpenChange={() => {}} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("the page switcher", () => {
  it("is named by what it is for", async () => {
    open();
    const dialog = await screen.findByRole("dialog", { name: "Search" });
    const field = within(dialog).getByRole("combobox");
    expect(field.getAttribute("placeholder")).toBe("Search…");
    const label = field.getAttribute("aria-labelledby");
    expect(label ? document.getElementById(label)?.textContent : null).toBe("Search");
    within(dialog).getByRole("listbox", { name: "Pages" });
  });
});
