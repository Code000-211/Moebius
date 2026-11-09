// @vitest-environment jsdom
/** The panel beside the graph: renaming a node, and the names it refuses before asking the graph to take them. */
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
  const onRename = vi.fn();
  render(
    <Inspector
      graph={graph}
      selection={{ kind: "node", id: "test" }}
      onRename={onRename}
      onParams={vi.fn()}
      onDeleteNode={vi.fn()}
      onDeleteEdge={vi.fn()}
    />,
  );
  return onRename;
}

describe("renaming a node from the panel", () => {
  it("gives the new name back when no other node has it", () => {
    const onRename = show();
    fireEvent.change(screen.getByLabelText("Id"), { target: { value: "check" } });
    fireEvent.blur(screen.getByLabelText("Id"));
    expect(onRename).toHaveBeenCalledWith("test", "check");
  });

  it("refuses the trigger's reserved id, and leaves the node's name alone", () => {
    const onRename = show();
    fireEvent.change(screen.getByLabelText("Id"), { target: { value: "trigger" } });
    fireEvent.blur(screen.getByLabelText("Id"));
    expect(screen.getByTestId("id-problem").textContent).toBe("The trigger's id is reserved.");
    expect(onRename).not.toHaveBeenCalled();
  });
});
