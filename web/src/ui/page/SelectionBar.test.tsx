import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ContextMenu } from "../components/ContextMenu";
import { SelectionBar, type SelectionCommand } from "./SelectionBar";

afterEach(cleanup);

function commands(run = vi.fn()): SelectionCommand[] {
  return [
    { id: "assign", label: "Assign to me", key: "a", run },
    { id: "stop", label: "Stop run", key: "x", run: vi.fn() },
  ];
}

describe("SelectionBar", () => {
  it("says how many are chosen and shows nothing when none are", () => {
    const { rerender } = render(
      <SelectionBar noun="card" count={3} commands={commands()} onClear={() => {}} />,
    );
    expect(screen.getByRole("status").textContent).toBe("3 cards chosen");
    rerender(<SelectionBar noun="card" count={1} commands={commands()} onClear={() => {}} />);
    expect(screen.getByRole("status").textContent).toBe("1 card chosen");
    rerender(<SelectionBar noun="card" count={0} commands={commands()} onClear={() => {}} />);
    expect(screen.queryByRole("toolbar")).toBeNull();
  });

  it("runs a command by its button and by its key, but not while a field has the keys", () => {
    const run = vi.fn();
    render(<SelectionBar noun="card" count={2} commands={commands(run)} onClear={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /Assign to me/ }));
    expect(run).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(document.body, { key: "a" });
    expect(run).toHaveBeenCalledTimes(2);
    const input = document.createElement("input");
    document.body.append(input);
    fireEvent.keyDown(input, { key: "a" });
    expect(run).toHaveBeenCalledTimes(2);
    input.remove();
  });

  it("lets go of the choice from its button", () => {
    const clear = vi.fn();
    render(<SelectionBar noun="run" count={2} commands={[]} onClear={clear} />);
    fireEvent.click(screen.getByRole("button", { name: /Clear the choice/ }));
    expect(clear).toHaveBeenCalled();
  });
});

describe("ContextMenu", () => {
  it("lists the commands, keeps their keys for a screen reader, draws none, runs one and closes", () => {
    const run = vi.fn();
    const close = vi.fn();
    render(
      <ContextMenu
        point={{ x: 10, y: 10 }}
        title="2 cards"
        commands={commands(run)}
        onClose={close}
      />,
    );
    expect(screen.getByRole("menu", { name: "2 cards" })).toBeTruthy();
    const item = screen.getByRole("menuitem", { name: /Assign to me/ });
    expect(item.getAttribute("aria-keyshortcuts")).toBe("a");
    expect(document.querySelector("kbd")).toBeNull();
    fireEvent.click(item);
    expect(run).toHaveBeenCalled();
    expect(close).toHaveBeenCalled();
  });

  it("closes on Escape and draws nothing without a point", () => {
    const close = vi.fn();
    const { rerender } = render(
      <ContextMenu point={{ x: 1, y: 1 }} title="1 card" commands={commands()} onClose={close} />,
    );
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(close).toHaveBeenCalled();
    rerender(<ContextMenu point={null} title="1 card" commands={commands()} onClose={close} />);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
