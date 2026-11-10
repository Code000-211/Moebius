// @vitest-environment jsdom
/** The generic parameter form, built from what a kind reports rather than from anything the form knows. */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkflowDefinition, WorkflowNode } from "../../../api/types/workflows";
import { nodeKinds, type ParamField } from "../nodeKinds";
import { ParamForm } from "./ParamForm";

afterEach(cleanup);

const forEachFields = nodeKinds.find((kind) => kind.kind === "for_each")?.params ?? [];

const graph: WorkflowDefinition = {
  name: "release",
  description: "",
  trigger: [{ kind: "manual", params: {} }],
  nodes: [
    { id: "build", kind: "agent", position: { x: 0, y: 0 } },
    { id: "publish", kind: "script", position: { x: 0, y: 0 } },
  ],
  edges: [],
};

function node(over: Partial<WorkflowNode> = {}): WorkflowNode {
  return {
    id: "per-file",
    kind: "for_each",
    params: { rejoin: "publish" },
    position: { x: 0, y: 0 },
    ...over,
  };
}

function show(over: Partial<WorkflowNode> = {}, fields: ParamField[] = forEachFields) {
  const onParams = vi.fn();
  render(<ParamForm node={node(over)} graph={graph} fields={fields} known onParams={onParams} />);
  return onParams;
}

describe("a form from the metadata", () => {
  it("has a control for every parameter the kind reports, at the value the node has", () => {
    show({ params: { rejoin: "publish", max_elements: 20 } });
    expect(screen.getByLabelText("Rejoins at")).toBeTruthy();
    expect(picked()).toBe("publish");
    expect((screen.getByLabelText("Most elements") as HTMLInputElement).value).toBe("20");
  });

  it("explains a parameter under the control that asks for it", () => {
    show();
    expect(
      screen.getByText(
        "The most elements one run of the node may fan out over. Left alone, a hundred.",
      ),
    ).toBeTruthy();
  });

  it("says which parameter the kind needs and the node does not have yet", () => {
    show({ params: {} });
    expect(screen.getByText(/"rejoin" still has no value/)).toBeTruthy();
  });
});

describe("editing a parameter", () => {
  it("gives the whole set back with the one change, and keeps the rest", () => {
    const onParams = show({ params: { rejoin: "publish", max_elements: 20 } });
    fireEvent.change(screen.getByLabelText("Most elements"), { target: { value: "5" } });
    expect(onParams).toHaveBeenLastCalledWith({ rejoin: "publish", max_elements: 5 });
  });

  it("writes a number as a number, and takes it away when the field is cleared", () => {
    const onParams = show();
    fireEvent.change(screen.getByLabelText("Most elements"), { target: { value: "12" } });
    expect(onParams).toHaveBeenLastCalledWith({ rejoin: "publish", max_elements: 12 });
    fireEvent.change(screen.getByLabelText("Most elements"), { target: { value: "" } });
    expect(onParams).toHaveBeenLastCalledWith({ rejoin: "publish" });
  });

  it("keeps a parameter the registry does not describe when another one is edited", () => {
    const onParams = show({ params: { rejoin: "publish", retries: 3 } });
    expect((screen.getByLabelText("Other parameters") as HTMLTextAreaElement).value).toBe(
      '{\n  "retries": 3\n}',
    );
    fireEvent.change(screen.getByLabelText("Most elements"), { target: { value: "8" } });
    expect(onParams).toHaveBeenLastCalledWith({ rejoin: "publish", retries: 3, max_elements: 8 });
  });

  it("takes the parameter a person writes in the JSON and keeps the ones the form shows", () => {
    const onParams = show({ params: { rejoin: "publish", retries: 3 } });
    fireEvent.change(screen.getByLabelText("Other parameters"), {
      target: { value: '{\n  "retries": 4,\n  "when": "always"\n}' },
    });
    expect(onParams).toHaveBeenLastCalledWith({ rejoin: "publish", retries: 4, when: "always" });
  });

  it("says so when the JSON is not JSON yet, and gives nothing back", () => {
    const onParams = show({ params: { rejoin: "publish", retries: 3 } });
    fireEvent.change(screen.getByLabelText("Other parameters"), { target: { value: "{oops" } });
    expect(screen.getByText("This is not JSON yet.")).toBeTruthy();
    expect(onParams).not.toHaveBeenCalled();
  });
});

describe("a kind the registry does not know", () => {
  it("is edited as JSON whole", () => {
    const onParams = vi.fn();
    render(
      <ParamForm
        node={node({ kind: "agent", params: { agent: "Coder" } })}
        graph={graph}
        fields={[]}
        known={false}
        onParams={onParams}
      />,
    );
    expect(screen.getByText(/Nothing in the registry describes/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Parameters"), {
      target: { value: '{\n  "agent": "Reviewer"\n}' },
    });
    expect(onParams).toHaveBeenLastCalledWith({ agent: "Reviewer" });
  });
});

describe("the other kinds of control", () => {
  function some(fields: ParamField[], params: Record<string, unknown>) {
    const onParams = vi.fn();
    render(
      <ParamForm
        node={node({ kind: "odd", params })}
        graph={graph}
        fields={fields}
        known
        onParams={onParams}
      />,
    );
    return onParams;
  }

  it("writes a switch as a boolean", () => {
    const onParams = some([{ type: "boolean", name: "retries_ok", label: "Try again" }], {});
    fireEvent.click(screen.getByRole("switch", { name: "Try again" }));
    expect(onParams).toHaveBeenLastCalledWith({ retries_ok: true });
  });

  it("writes a list as one item per line", () => {
    const onParams = some([{ type: "list", name: "paths", label: "Paths" }], {
      paths: ["src", "docs"],
    });
    const field = screen.getByLabelText("Paths") as HTMLTextAreaElement;
    expect(field.value).toBe("src\ndocs");
    fireEvent.change(field, { target: { value: "src\nweb\n" } });
    expect(onParams).toHaveBeenLastCalledWith({ paths: ["src", "web"] });
  });

  it("writes a JSON field as the value it spells", () => {
    const onParams = some([{ type: "json", name: "when", label: "When" }], {
      when: { branch: "main" },
    });
    fireEvent.change(screen.getByLabelText("When"), { target: { value: '{ "branch": "dev" }' } });
    expect(onParams).toHaveBeenLastCalledWith({ when: { branch: "dev" } });
  });

  it("offers the other nodes of the graph where a parameter names one", () => {
    show({ params: { rejoin: "publish" } });
    expect(picked()).toBe("publish");
  });
});

/** What the control that picks a node shows as chosen. */
function picked(): string {
  return screen.getByRole("button", { name: "Rejoins at" }).textContent?.trim() ?? "";
}
