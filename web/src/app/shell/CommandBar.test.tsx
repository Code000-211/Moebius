/**
 * The one term the page switcher uses: the dialog, the field of the query and the list of what it offers say the same
 * thing, in the words the sidebar's button uses (`docs/design-web.md`: one term per concept, sentence case).
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import { AgentProvider, useAgent } from "./AgentContext";
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
    {
      id: 42,
      number: 13,
      project_number: 21,
      project_key: "PAR",
      title: "Write the docs",
      repository_id: 3,
    },
  ],
};

function open(commands: PaletteCommand[] = [], label = "#7 Focus card", owner = false) {
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
    vi.fn(async (url: string) => {
      const body = String(url).includes("/v1/people")
        ? [
            {
              id: 1,
              handle: "tom",
              name: "Tom Baker",
              role: "member",
              disabled: false,
              created_at: 0,
            },
          ]
        : { boards: [BOARD], harness_changes: [], inbox: [] };
      return { ok: true, status: 200, text: async () => JSON.stringify(body) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (owner) {
    client.setQueryData(keys.session, { signed_in: true, person: { handle: "me", role: "owner" } });
  }
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <AgentProvider>
          <PaletteFocusProvider>
            {commands.length > 0 && <PaletteFocusFor focus={{ label, commands }} />}
            <CommandBar open onOpenChange={() => {}} />
            <AgentProbe />
          </PaletteFocusProvider>
        </AgentProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

/** Shows what the agent was handed, so a test can see where Tab and the New conversation command went. */
function AgentProbe() {
  const agent = useAgent();
  return (
    <output data-testid="agent">
      {`${agent.open ? "open" : "closed"}|${agent.pending ?? ""}|${agent.fresh}`}
    </output>
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

  it("leaves Ctrl+K and Ctrl+J to the shell, which opens and closes the palette and the agent with them", async () => {
    // The list would take them as "previous" and "next" and mark them handled, so the shell would not hear them: a
    // Ctrl+K typed while the palette is still fading out would be lost.
    open();
    const field = within(await screen.findByRole("dialog", { name: "Search" })).getByRole(
      "combobox",
    );
    for (const key of ["k", "j", "n", "p"]) {
      expect(fireEvent.keyDown(field, { key, ctrlKey: true }), `Ctrl+${key}`).toBe(true);
    }
  });

  it("lists the commands of the focus first", async () => {
    const run = vi.fn();
    open([{ id: "assign", label: "Assign to me", key: "a", run }]);
    const options = await screen.findAllByRole("option");
    expect(options[0]?.textContent).toContain("Assign to me");
    expect(screen.getByText("#7 Focus card")).toBeTruthy();
    fireEvent.click(options[0] as HTMLElement);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("draws no key on any row", async () => {
    open([{ id: "assign", label: "Assign to me", key: "a", run: () => {} }]);
    await screen.findAllByRole("option");
    expect(screen.getByRole("dialog").querySelectorAll("kbd")).toHaveLength(0);
  });

  it("gives every row an icon and a name", async () => {
    open();
    const options = await screen.findAllByRole("option");
    for (const option of options) expect(option.querySelector("svg")).toBeTruthy();
  });

  it("sends the typed text to the agent on Tab and closes", async () => {
    open();
    const field = await screen.findByRole("combobox");
    fireEvent.change(field, { target: { value: "  why is the build red?  " } });
    fireEvent.keyDown(field, { key: "Tab" });
    expect(screen.getByTestId("agent").textContent).toBe("open|why is the build red?|0");
  });

  it("keeps the palette open and the agent closed on Tab with nothing typed", async () => {
    open();
    const field = await screen.findByRole("combobox");
    const event = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
    field.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    fireEvent.change(field, { target: { value: "   " } });
    fireEvent.keyDown(field, { key: "Tab" });
    expect(screen.getByTestId("agent").textContent).toBe("closed||0");
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("starts a fresh conversation with the agent from the New conversation command", async () => {
    open();
    fireEvent.click(await screen.findByRole("option", { name: "New conversation" }));
    expect(screen.getByTestId("agent").textContent).toBe("open||1");
  });

  it("keeps the commands of the focus while they match and finds the rest by kind", async () => {
    open([
      { id: "assign", label: "Assign to me", key: "a", run: () => {} },
      { id: "block", label: "Block…", key: "b", run: () => {} },
    ]);
    const field = await screen.findByRole("combobox");
    fireEvent.change(field, { target: { value: "block" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Block…"]);
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
    expect(await screen.findByText("Nothing matches.")).toBeTruthy();
  });

  it("finds the wiki's pages by the words typed and opens the one chosen", async () => {
    open();
    const field = await screen.findByRole("combobox");
    const asked: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        asked.push(String(url));
        const body = String(url).includes("/v1/wiki/search")
          ? [{ path: "projects/PAR/Refunds", name: "Refunds", project: "PAR", snippet: "x" }]
          : { boards: [BOARD], harness_changes: [], inbox: [] };
        return { ok: true, status: 200, text: async () => JSON.stringify(body) };
      }),
    );
    fireEvent.change(field, { target: { value: "refund" } });
    const hit = await screen.findByRole("option", { name: /Refunds/ });
    expect(hit.textContent).toContain("PAR");
    expect(asked.some((url) => url.includes("/v1/wiki/search?q=refund"))).toBe(true);
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
    expect(screen.getByRole("option").textContent).toContain("PAR-21 Write the docs");
  });

  it("finds a card by its key, in any case, and by its issue number", async () => {
    open();
    const field = await screen.findByRole("combobox");
    await screen.findByRole("option", { name: "Parser" });
    for (const query of ["PAR-21", "par-21", "#13"]) {
      fireEvent.change(field, { target: { value: query } });
      await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(1));
      expect(screen.getByRole("option").textContent).toContain("PAR-21 Write the docs");
    }
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

  it("says it is searching, not that nothing matches, while the wiki has not answered", async () => {
    open();
    const field = await screen.findByRole("combobox");
    let answer: (hits: unknown[]) => void = () => {};
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        const body = String(url).includes("/v1/wiki/search")
          ? await new Promise<unknown[]>((resolve) => {
              answer = resolve;
            })
          : { boards: [BOARD], harness_changes: [], inbox: [] };
        return { ok: true, status: 200, text: async () => JSON.stringify(body) };
      }),
    );
    fireEvent.change(field, { target: { value: "gamma" } });
    // Before the pause that starts the search, and while it is out, the list is not empty-handed.
    expect(screen.getByText("Searching…")).toBeTruthy();
    expect(screen.queryByText("Nothing matches.")).toBeNull();
    await waitFor(() => expect(typeof answer).toBe("function"));
    await new Promise((r) => setTimeout(r, 300));
    expect(screen.getByText("Searching…")).toBeTruthy();
    answer([{ path: "Gamma", name: "Gamma", project: null, snippet: "x" }]);
    await screen.findByRole("option", { name: /Gamma/ });
    expect(screen.queryByText("Searching…")).toBeNull();
  });

  it("finds people in a group of their own, above a loose match of a page", async () => {
    open([], "#7 Focus card", true);
    const field = await screen.findByRole("combobox");
    fireEvent.change(field, { target: { value: "tom" } });
    const person = await screen.findByRole("option", { name: /Tom Baker/ });
    expect(person.closest("[cmdk-group]")?.querySelector("[cmdk-group-heading]")?.textContent).toBe(
      "People",
    );
    // "Automations" holds "tom" inside a word; the person is the whole word, so comes first.
    const names = screen.getAllByRole("option").map((o) => o.textContent ?? "");
    expect(names[0]).toContain("Tom Baker");
    expect(names.some((n) => n.includes("Automations"))).toBe(true);
  });
});
