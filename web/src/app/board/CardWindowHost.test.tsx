// @vitest-environment jsdom
/**
 * The card window for the whole app: `?card=` opens the card as a dialog over any page, which stays as it is behind it;
 * a page opens a card by setting the parameter on itself; Escape and the close control drop the parameter and hand the
 * focus back to what opened the card; a reload with the parameter opens the same card again; and a menu of the window
 * opens its own dialog over it, whose Escape leaves the window open.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Board, Card } from "../../api/types/board";
import { CardWindowHost, useOpenCard } from "./CardWindowHost";
import { card } from "./cardPropertiesFixtures";
import { NewIssueProvider } from "./NewIssueHost";

const board = (cards: Card[]): Board => ({
  repository: {
    id: 1,
    name: "demo",
    path: "/r",
    forge: "none",
    default_branch: "main",
    home_project_id: 1,
    github: null,
    trusted: [],
  },
  cards,
  round_limit: 5,
  waiting: {},
  changes: [],
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function Where() {
  const { pathname, search } = useLocation();
  return <p data-testid="where">{pathname + search}</p>;
}

/** A page with a card to open on it, as the Inbox or Fleet has one, and the host the shell mounts. */
function Page() {
  const open = useOpenCard();
  return (
    <main>
      <p>The page behind</p>
      <button type="button" onClick={() => open(5)}>
        Open card five
      </button>
    </main>
  );
}

function shell(path: string) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => ({
      ok: true,
      status: 200,
      text: async () =>
        JSON.stringify(
          String(url).endsWith("/snapshot")
            ? { boards: [board([card(5), card(6)])], inbox: [] }
            : [],
        ),
    })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[path]}>
        <NewIssueProvider>
          <Routes>
            <Route path="*" element={<Page />} />
          </Routes>
          <CardWindowHost />
          <Where />
        </NewIssueProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const where = () => screen.getByTestId("where").textContent;

describe("the card window", () => {
  it("opens over the page it was opened from and keeps the page and its address", async () => {
    shell("/inbox?tab=all");
    expect(screen.queryByRole("dialog", { name: "Card inspector" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Open card five" }));
    const dialog = await screen.findByRole("dialog", { name: "Card inspector" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(where()).toBe("/inbox?tab=all&card=5");
    // The page is still drawn behind it, and the window says whose it is.
    expect(screen.getByText("The page behind")).toBeTruthy();
    expect(dialog.textContent).toContain("Issue 5");
    expect(screen.getByRole("link", { name: "Open as page" }).getAttribute("href")).toBe(
      "/board/1?card=5",
    );
  });

  it("opens again on reload, and shows nothing for a card that is not there", async () => {
    shell("/fleet?card=6");
    expect(await screen.findByRole("dialog", { name: "Card inspector" })).toBeTruthy();
    cleanup();
    shell("/fleet?card=99");
    await waitFor(() => expect(screen.getByText("The page behind")).toBeTruthy());
    await new Promise((done) => setTimeout(done, 50));
    expect(screen.queryByRole("dialog", { name: "Card inspector" })).toBeNull();
  });

  it("drops the parameter on Escape and on the close control, and hands the focus back", async () => {
    shell("/inbox");
    const opener = screen.getByRole("button", { name: "Open card five" });
    opener.focus();
    fireEvent.click(opener);
    const dialog = await screen.findByRole("dialog", { name: "Card inspector" });
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Card inspector" })).toBeNull(),
    );
    expect(where()).toBe("/inbox");
    await waitFor(() => expect(document.activeElement).toBe(opener));

    fireEvent.click(opener);
    await screen.findByRole("dialog", { name: "Card inspector" });
    fireEvent.click(screen.getByRole("button", { name: "Close the inspector" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Card inspector" })).toBeNull(),
    );
    expect(where()).toBe("/inbox");
  });

  it("leaves the window open when Escape closes the dialog over it", async () => {
    shell("/inbox?card=5");
    await screen.findByRole("dialog", { name: "Card inspector" });
    fireEvent.keyDown(screen.getByRole("button", { name: "More actions for #5" }), {
      key: "Enter",
    });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Edit issue…" }));
    const edit = await screen.findByRole("dialog", { name: "Edit #5" });
    fireEvent.keyDown(edit, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Edit #5" })).toBeNull());
    expect(screen.getByRole("dialog", { name: "Card inspector" })).toBeTruthy();
    expect(where()).toBe("/inbox?card=5");
  });

  it("keeps every property out of the window, in a window of their own that the menu opens", async () => {
    shell("/inbox?card=5");
    const dialog = await screen.findByRole("dialog", { name: "Card inspector" });
    expect(dialog.querySelector('section[aria-label="Properties of #5"]')).toBeNull();
    expect(screen.queryByText("Priority")).toBeNull();
    fireEvent.keyDown(screen.getByRole("button", { name: "More actions for #5" }), {
      key: "Enter",
    });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Properties…" }));
    const properties = await screen.findByRole("dialog", { name: "Properties of #5" });
    const block = properties.querySelector('section[aria-label="Properties of #5"]');
    expect(block?.textContent).toContain("Priority");
    expect(block?.querySelector("details")?.hasAttribute("open")).toBe(false);
  });

  it("has no captions: no Its run, Activity, Write a comment or Edit issue button in the window", async () => {
    shell("/inbox?card=5");
    const dialog = await screen.findByRole("dialog", { name: "Card inspector" });
    for (const caption of ["Its run", "Activity", "Your turn", "The issue", "More properties"]) {
      expect(within(dialog).queryByText(caption)).toBeNull();
    }
    expect(within(dialog).queryByRole("button", { name: "Edit issue" })).toBeNull();
  });
});
