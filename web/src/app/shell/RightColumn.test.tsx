// @vitest-environment jsdom
/**
 * The page's right column: it unfolds for a page that has properties and stays the slim column for one that has none,
 * folding is kept for this browser, "Ask Moebius" and `Ctrl+J` open the agent over the column, and the properties keep
 * their state while the agent lies over them and while they are folded.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PageProperties, PropertiesProvider, usePropertiesShown } from "../../ui/page/Properties";
import { SheetSlotProvider } from "../../ui/page/SheetSlot";
import { RightColumn } from "./RightColumn";

// The agent's conversation follows the event stream, which these tests have none of.
vi.mock("../../live/LiveProvider", () => ({
  useServerEvents: () => {},
  useConnection: () => "online",
}));

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string) => {
      if (path === "/v1/snapshot") {
        return new Response(JSON.stringify({ boards: [], harness_changes: [], inbox: [] }));
      }
      if (path.startsWith("/v1/chats")) return new Response("[]");
      return new Response(`no stub for ${path}`, { status: 404 });
    }),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  localStorage.clear();
});

/** A property with state of its own: a count that only a remount would lose. */
function Counter() {
  const [count, setCount] = useState(0);
  return (
    <button type="button" onClick={() => setCount(count + 1)}>
      {`Pressed ${count}`}
    </button>
  );
}

/** What a page that lays something beside the column reads: whether the column stands unfolded. */
function Beside() {
  return <output aria-label="Beside">{usePropertiesShown() ? "shown" : "slim"}</output>;
}

function mount(withProperties: boolean) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={["/workflows/release"]}>
        <SheetSlotProvider>
          <PropertiesProvider>
            {withProperties && (
              <PageProperties label="Definition">
                <Counter />
              </PageProperties>
            )}
            <RightColumn />
            <Beside />
          </PropertiesProvider>
        </SheetSlotProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const column = () => document.querySelector('[data-surface="right-column"]') as HTMLElement;
const agent = () => screen.queryByRole("dialog", { name: "Moebius agent" });
const ask = () => screen.getByRole("button", { name: "Ask Moebius" });

describe("the right column", () => {
  it("draws a page's properties in the column, unfolded", async () => {
    mount(true);
    const region = await screen.findByRole("region", { name: "Definition" });
    expect(column().contains(region)).toBe(true);
    expect(column().hasAttribute("data-folded")).toBe(false);
    expect(screen.getByRole("heading", { name: "Properties" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Fold the properties" })).toBeTruthy();
    expect(ask().textContent).toContain("Ask Moebius");
  });

  it("stays the slim column on a page without properties, with both buttons", () => {
    mount(false);
    expect(column().hasAttribute("data-folded")).toBe(true);
    const unfold = screen.getByRole("button", { name: "Show the properties" }) as HTMLButtonElement;
    expect(unfold.disabled).toBe(true);
    expect(ask()).toBeTruthy();
  });

  it("folds to the slim column with both buttons, keeps the choice, and keeps the properties' state", async () => {
    mount(true);
    fireEvent.click(await screen.findByRole("button", { name: "Pressed 0" }));
    fireEvent.click(screen.getByRole("button", { name: "Fold the properties" }));
    expect(column().hasAttribute("data-folded")).toBe(true);
    expect(screen.getByRole("button", { name: "Show the properties" })).toBeTruthy();
    expect(ask()).toBeTruthy();
    expect(localStorage.getItem("moebius.properties.folded")).toBe("1");

    // Another page, or a reload, finds it folded.
    cleanup();
    mount(true);
    expect(column().hasAttribute("data-folded")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Show the properties" }));
    expect(column().hasAttribute("data-folded")).toBe(false);
    expect(localStorage.getItem("moebius.properties.folded")).toBe("0");
  });

  it("opens the agent over the column and gives it back, the properties kept as they were", async () => {
    mount(true);
    fireEvent.click(await screen.findByRole("button", { name: "Pressed 0" }));
    fireEvent.click(ask());
    expect(agent()).toBeTruthy();
    // The column is still there under the sheet, with its properties as they were.
    expect(screen.getByRole("button", { name: "Pressed 1" })).toBeTruthy();
    // "Properties" goes back to them.
    fireEvent.click(screen.getByRole("button", { name: "Properties" }));
    expect(agent()).toBeNull();
    expect(screen.getByRole("button", { name: "Pressed 1" })).toBeTruthy();
    expect(document.activeElement).toBe(ask());
  });

  it("tells the page whether it stands unfolded with the properties", () => {
    mount(true);
    const beside = () => screen.getByRole("status", { name: "Beside" }).textContent;
    expect(beside()).toBe("shown");
    fireEvent.click(screen.getByRole("button", { name: "Fold the properties" }));
    expect(beside()).toBe("slim");
    cleanup();
    localStorage.clear();
    mount(false);
    expect(beside()).toBe("slim");
  });

  it("toggles the agent with Ctrl+J and closes it on Escape", () => {
    mount(false);
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(agent()).toBeTruthy();
    // A page without properties has nothing to go back to.
    expect(screen.queryByRole("button", { name: "Properties" })).toBeNull();
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(agent()).toBeNull();
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(agent()).toBeNull();
  });
});
