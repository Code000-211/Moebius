// @vitest-environment jsdom
/**
 * The docked "Ask Moebius" field: Enter or the arrow hands the message to the agent and clears the field, an empty
 * field sends nothing, and the field shows no key hint.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AgentProvider, useAgent } from "./AgentContext";
import { AgentDock } from "./AgentDock";

afterEach(cleanup);

function Seen() {
  const agent = useAgent();
  return (
    <output aria-label="Agent">{`${agent.open ? "open" : "closed"}|${agent.pending ?? ""}`}</output>
  );
}

function mount() {
  return render(
    <AgentProvider>
      <AgentDock />
      <Seen />
    </AgentProvider>,
  );
}

const state = () => screen.getByRole("status", { name: "Agent" }).textContent;

describe("the Ask Moebius dock", () => {
  it("sends on Enter, opens the agent with the message and clears the field", () => {
    mount();
    const field = screen.getByRole("textbox", { name: "Ask Moebius" }) as HTMLInputElement;
    fireEvent.change(field, { target: { value: "Why did MOE-209 stop?" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(state()).toBe("open|Why did MOE-209 stop?");
    expect(field.value).toBe("");
  });

  it("sends with the arrow, and nothing when the field is empty", () => {
    mount();
    const send = screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Ask Moebius" }), { key: "Enter" });
    expect(state()).toBe("closed|");
    fireEvent.change(screen.getByRole("textbox", { name: "Ask Moebius" }), {
      target: { value: "Hello" },
    });
    fireEvent.click(send);
    expect(state()).toBe("open|Hello");
  });

  it("draws the send arrow in the text colour on the control fill, muted while it is disabled", () => {
    mount();
    const send = screen.getByRole("button", { name: "Send" });
    for (const name of [
      "bg-control-fill!",
      "text-text",
      "disabled:text-secondary",
      "disabled:opacity-100",
    ])
      expect(send.className).toContain(name);
  });

  it("shows no key hint", () => {
    const { container } = mount();
    expect(container.querySelector("kbd")).toBeNull();
  });
});
