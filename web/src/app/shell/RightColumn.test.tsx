// @vitest-environment jsdom
/**
 * The properties pane: it is there for a page that has properties and hidden for one that has none, it has no folded
 * rail and no row for the agent, `Ctrl+J` opens the agent over it, and the properties keep their state while the agent
 * lies over them.
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
  useWatchRun: () => {},
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

describe("the properties pane", () => {
  it("draws a page's properties in the pane", async () => {
    mount(true);
    const region = await screen.findByRole("region", { name: "Definition" });
    expect(column().contains(region)).toBe(true);
    expect(column().hidden).toBe(false);
    expect(screen.getByRole("heading", { name: "Properties" })).toBeTruthy();
  });

  it("is not shown on a page without properties", () => {
    mount(false);
    expect(column().hidden).toBe(true);
    expect(screen.queryByRole("button", { name: "Show the properties" })).toBeNull();
  });

  it("has no fold button, no folded rail and no Ask Moebius row", async () => {
    mount(true);
    await screen.findByRole("region", { name: "Definition" });
    expect(screen.queryByRole("button", { name: "Fold the properties" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Ask Moebius" })).toBeNull();
    expect(column().querySelector("kbd")).toBeNull();
    expect(column().className).not.toContain("transition");
  });

  it("tells the page whether the pane is there", () => {
    mount(true);
    expect(screen.getByRole("status", { name: "Beside" }).textContent).toBe("shown");
    cleanup();
    mount(false);
    expect(screen.getByRole("status", { name: "Beside" }).textContent).toBe("slim");
  });

  it("opens the agent with Ctrl+J, the properties kept as they were, and goes back with Properties", async () => {
    mount(true);
    fireEvent.click(await screen.findByRole("button", { name: "Pressed 0" }));
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(await screen.findByRole("dialog", { name: "Moebius agent" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Pressed 1" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Properties" }));
    expect(agent()).toBeNull();
    expect(screen.getByRole("button", { name: "Pressed 1" })).toBeTruthy();
  });

  it("toggles the agent with Ctrl+J and closes it on Escape", async () => {
    mount(false);
    expect(agent()).toBeNull();
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(await screen.findByRole("dialog", { name: "Moebius agent" })).toBeTruthy();
    // A page without properties has nothing to go back to.
    expect(screen.queryByRole("button", { name: "Properties" })).toBeNull();
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(agent()).toBeNull();
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(agent()).toBeNull();
  });
});
