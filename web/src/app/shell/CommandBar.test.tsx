/**
 * The one term the page switcher uses: the dialog, the field of the query and the list of what it offers say the same
 * thing, in the words the sidebar's button uses (`docs/design-web.md`: one term per concept, sentence case).
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandBar } from "./CommandBar";
import { PaletteFocusFor, PaletteFocusProvider } from "./palette/focus";
import type { PaletteCommand } from "./palette/model";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

// jsdom has neither, and the list of results measures itself and scrolls the choice into view.
Element.prototype.scrollIntoView = () => {};

const BOARD = {
  repository: {
    id: 3,
    name: "Parser",
    path: "",
    forge: "none",
    default_branch: "main",
    github: null,
    trusted: [],
  },
  cards: [
    { id: 41, number: 12, title: "Fix the lexer", repository_id: 3 },
    { id: 42, number: 13, title: "Write the docs", repository_id: 3 },
  ],
};

function open(commands: PaletteCommand[] = [], label = "#7 Focus card") {
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
      text: async () => JSON.stringify({ boards: [BOARD], harness_changes: [], inbox: [] }),
    })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <PaletteFocusProvider>
          {commands.length > 0 && <PaletteFocusFor focus={{ label, commands }} />}
          <CommandBar open onOpenChange={() => {}} />
        </PaletteFocusProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function Host() {
  const [shown, setShown] = useState(false);
  return (
    <MemoryRouter>
      <button type="button" onClick={() => setShown(true)}>
        Start
      </button>
      <CommandBar open={shown} onOpenChange={setShown} />
    </MemoryRouter>
  );
}

describe("the palette", () => {
  it("is named by what it is for", async () => {
    open();
    const dialog = await screen.findByRole("dialog", { name: "Search" });
    const field = within(dialog).getByRole("combobox");
    expect(field.getAttribute("placeholder")).toBe("Search…");
    const label = field.getAttribute("aria-labelledby");
    expect(label ? document.getElementById(label)?.textContent : null).toBe("Search");
    within(dialog).getByRole("listbox", { name: "Commands and places" });
  });

  it("lists the commands of the focus first, with their keys", async () => {
    const run = vi.fn();
    open([{ id: "assign", label: "Assign to me", key: "a", run }]);
    const options = await screen.findAllByRole("option");
    expect(options[0]?.textContent).toContain("Assign to me");
    expect(options[0]?.querySelector("kbd")?.textContent).toBe("a");
    expect(screen.getByText("#7 Focus card")).toBeTruthy();
    fireEvent.click(options[0] as HTMLElement);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("keeps the commands of the focus while they match and finds the rest by kind", async () => {
    open([
      { id: "assign", label: "Assign to me", key: "a", run: () => {} },
      { id: "block", label: "Block…", key: "b", run: () => {} },
    ]);
    const field = await screen.findByRole("combobox");
    fireEvent.change(field, { target: { value: "block" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Block…b"]);
    fireEvent.change(field, { target: { value: "a" } });
    const names = screen.getAllByRole("option").map((o) => o.textContent ?? "");
    expect(names[0]).toContain("Assign to me");
    expect(names.some((n) => n.includes("Agents"))).toBe(true);
    fireEvent.change(field, { target: { value: "lexer" } });
    await screen.findByRole("option", { name: /#12 Fix the lexer/ });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    fireEvent.change(field, { target: { value: "card" } });
    expect(screen.getAllByRole("option")).toHaveLength(4);
    fireEvent.change(field, { target: { value: "place" } });
    await screen.findByRole("option", { name: "Parser" });
    fireEvent.change(field, { target: { value: "page retro" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Retro"]);
    fireEvent.change(field, { target: { value: "zzzz" } });
    expect(screen.getByText("Nothing matches.")).toBeTruthy();
  });

  it("moves with the arrows and runs the chosen command on Enter", async () => {
    const first = vi.fn();
    const second = vi.fn();
    open([
      { id: "one", label: "First thing", run: first },
      { id: "two", label: "Second thing", run: second },
    ]);
    const field = await screen.findByRole("combobox");
    fireEvent.keyDown(field, { key: "ArrowDown" });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(second).toHaveBeenCalledTimes(1);
    expect(first).not.toHaveBeenCalled();
  });

  it("offers a card and a place of the board", async () => {
    open();
    const field = await screen.findByRole("combobox");
    await screen.findByRole("option", { name: "Parser" });
    fireEvent.change(field, { target: { value: "docs" } });
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(1));
    expect(screen.getByRole("option").textContent).toContain("#13 Write the docs");
  });

  it("closes on Escape and gives the focus back to where it was", async () => {
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
        <Host />
      </QueryClientProvider>,
    );
    const start = screen.getByRole("button", { name: "Start" });
    start.focus();
    fireEvent.click(start);
    const field = await screen.findByRole("combobox");
    fireEvent.keyDown(field, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement?.textContent).toBe("Start"));
  });
});
