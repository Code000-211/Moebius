// @vitest-environment jsdom
/** The header's Moebius button: it shows whether the panel is open and opens and closes it, Ctrl+J included. */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AgentButton } from "./AgentButton";
import { AgentProvider, useAgent } from "./AgentContext";

afterEach(cleanup);

function Seen() {
  const agent = useAgent();
  return <output aria-label="Agent">{agent.open ? "open" : "closed"}</output>;
}

function mount() {
  render(
    <AgentProvider>
      <AgentButton />
      <Seen />
    </AgentProvider>,
  );
  return screen.getByRole("button", { name: "Moebius" });
}

const state = () => screen.getByRole("status", { name: "Agent" }).textContent;

describe("the Moebius button", () => {
  it("is not pressed while the panel is closed, and pressed with an indigo tint while it is open", () => {
    const button = mount();
    expect(button.getAttribute("aria-pressed")).toBe("false");
    expect(button.className).not.toContain("bg-accent-tint");
    fireEvent.click(button);
    expect(state()).toBe("open");
    expect(button.getAttribute("aria-pressed")).toBe("true");
    expect(button.className).toContain("bg-accent-tint");
    expect(button.className).toContain("text-accent-text");
  });

  it("opens the panel and closes it again when clicked twice", () => {
    const button = mount();
    fireEvent.click(button);
    expect(state()).toBe("open");
    fireEvent.click(button);
    expect(state()).toBe("closed");
    expect(button.getAttribute("aria-pressed")).toBe("false");
  });

  it("takes the focus back when the panel closes", () => {
    const button = mount();
    fireEvent.click(button);
    (document.activeElement as HTMLElement | null)?.blur();
    fireEvent.click(button);
    expect(document.activeElement).toBe(button);
  });

  it("follows Ctrl+J", () => {
    const button = mount();
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(button.getAttribute("aria-pressed")).toBe("true");
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(button.getAttribute("aria-pressed")).toBe("false");
  });
});
