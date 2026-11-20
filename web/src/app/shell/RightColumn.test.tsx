// @vitest-environment jsdom
/**
 * The properties pane: it is there for a page that has properties and hidden for one that has none, it has no folded
 * rail and no row for the agent, it is hidden while the agent's panel is open, and the properties keep their state
 * while it is.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { PageProperties, PropertiesProvider, usePropertiesShown } from "../../ui/page/Properties";
import { AgentProvider, useAgent } from "./AgentContext";
import { RightColumn } from "./RightColumn";

afterEach(() => {
  cleanup();
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

/** Stands for the dock and the panel's close control. */
function Toggle() {
  const agent = useAgent();
  return (
    <button type="button" onClick={() => (agent.open ? agent.hide() : agent.show())}>
      {agent.open ? "agent open" : "agent closed"}
    </button>
  );
}

function mount(withProperties: boolean) {
  return render(
    <MemoryRouter initialEntries={["/workflows/release"]}>
      <AgentProvider>
        <PropertiesProvider>
          {withProperties && (
            <PageProperties label="Definition">
              <Counter />
            </PageProperties>
          )}
          <RightColumn />
          <Beside />
          <Toggle />
        </PropertiesProvider>
      </AgentProvider>
    </MemoryRouter>,
  );
}

const column = () => document.querySelector('[data-surface="right-column"]') as HTMLElement;

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

  it("hides the pane while the agent is open, the properties kept as they were", async () => {
    mount(true);
    fireEvent.click(await screen.findByRole("button", { name: "Pressed 0" }));
    fireEvent.click(screen.getByRole("button", { name: "agent closed" }));
    expect(column().hidden).toBe(true);
    expect(screen.getByRole("status", { name: "Beside" }).textContent).toBe("slim");
    fireEvent.click(screen.getByRole("button", { name: "agent open" }));
    expect(column().hidden).toBe(false);
    expect(screen.getByRole("button", { name: "Pressed 1" })).toBeTruthy();
  });

  it("toggles the agent with Ctrl+J", () => {
    mount(false);
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(screen.getByRole("button", { name: "agent open" })).toBeTruthy();
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(screen.getByRole("button", { name: "agent closed" })).toBeTruthy();
  });
});
