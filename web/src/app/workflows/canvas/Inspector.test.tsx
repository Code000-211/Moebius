// @vitest-environment jsdom
/** The window a node opens in: its draft, Save, and the names it refuses. */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkflowDefinition } from "../../../api/types/workflows";
import { Inspector } from "./Inspector";

afterEach(cleanup);

const graph: WorkflowDefinition = {
  name: "release",
  description: "",
  trigger: [{ kind: "manual", params: {} }],
  nodes: [
    { id: "build", kind: "agent", position: { x: 0, y: 0 } },
    { id: "test", kind: "script", params: { script: "cargo test" }, position: { x: 0, y: 0 } },
  ],
  edges: [],
};

function show() {
  const onSaveNode = vi.fn();
  const onClose = vi.fn();
  render(
    <Inspector
      graph={graph}
      selection={{ kind: "node", id: "test" }}
      onSaveNode={onSaveNode}
      onSaveTrigger={vi.fn()}
      onDeleteNode={vi.fn()}
      onDeleteEdge={vi.fn()}
      onClose={onClose}
    />,
  );
  return { onSaveNode, onClose };
}

const id = () => screen.getByLabelText("Id");
const save = () => screen.getByRole("button", { name: "Save" });

describe("editing a node in the window", () => {
  it("gives the new name back only when Save is pressed", () => {
    const { onSaveNode, onClose } = show();
    expect(save().hasAttribute("disabled")).toBe(true);
    fireEvent.change(id(), { target: { value: "check" } });
    expect(onSaveNode).not.toHaveBeenCalled();
    fireEvent.click(save());
    expect(onSaveNode).toHaveBeenCalledWith("test", "check", { script: "cargo test" });
    expect(onClose).toHaveBeenCalled();
  });

  it("refuses the trigger's reserved id and cannot be saved with it", () => {
    const { onSaveNode } = show();
    fireEvent.change(id(), { target: { value: "trigger" } });
    expect(screen.getByTestId("id-problem").textContent).toBe("The trigger's id is reserved.");
    expect(save().hasAttribute("disabled")).toBe(true);
    expect(onSaveNode).not.toHaveBeenCalled();
  });

  it("asks before closing with edits, and closes a clean window at once", () => {
    const { onClose } = show();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.change(id(), { target: { value: "check" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("alertdialog", { name: "Discard changes?" })).toBeTruthy();
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
