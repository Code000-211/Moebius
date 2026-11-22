import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { WorkflowDefinition } from "../../../api/types/workflows";
import { Palette } from "./Palette";

const graph: WorkflowDefinition = {
  name: "w",
  description: "",
  trigger: [{ kind: "manual", params: {} }],
  nodes: [],
  edges: [],
};

describe("the palette", () => {
  it("has one row per kind, each named once, in the order the owner chose", () => {
    render(<Palette graph={graph} onAdd={vi.fn()} />);
    const rows = screen.getAllByTestId("palette-kind");
    expect(rows.map((row) => row.textContent?.trim())).toEqual([
      "Agent",
      "Code",
      "Condition",
      "For each",
      "HTTP",
      "Approval",
      "Merge",
      "Chat",
      "Publish",
    ]);
    // The kind's own name is not written a second time.
    for (const row of rows) {
      expect(within(row).getAllByText(/./)).toHaveLength(1);
    }
  });

  it("is titled and ends with what to do", () => {
    render(<Palette graph={graph} onAdd={vi.fn()} />);
    expect(screen.getByText("Add a step")).toBeTruthy();
    expect(screen.getByText("Drag onto the canvas")).toBeTruthy();
  });

  it("adds a step when a row is pressed, and lets it be dragged", () => {
    const onAdd = vi.fn();
    render(<Palette graph={graph} onAdd={onAdd} />);
    const row = screen.getByRole("button", { name: "Code" });
    expect(row.getAttribute("draggable")).toBe("true");
    row.click();
    expect(onAdd.mock.calls[0]?.[0].kind).toBe("code");
  });
});
