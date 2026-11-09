/** The graph's edit operations, and where a refused save's message belongs. */
import { describe, expect, it } from "vitest";
import type { WorkflowDefinition } from "../../../api/types/workflows";
import {
  addNode,
  connect,
  edgeId,
  isDirty,
  moveNode,
  nextNodeId,
  nextPosition,
  nodeOf,
  problemFor,
  removeEdge,
  removeNode,
  renameNode,
  setParams,
  setTrigger,
  TRIGGER,
} from "./model";

function graph(over: Partial<WorkflowDefinition> = {}): WorkflowDefinition {
  return {
    name: "release",
    description: "Build, test and publish a release.",
    trigger: [{ kind: "schedule", params: { detail: "0 6 * * 1" } }],
    nodes: [
      { id: "build", kind: "agent", params: { agent: "Coder" }, position: { x: 200, y: 120 } },
      {
        id: "test",
        kind: "script",
        params: { script: "cargo test" },
        position: { x: 480, y: 120 },
      },
    ],
    edges: [{ from: TRIGGER, to: "build", port: "out" }],
    ...over,
  };
}

describe("adding a node", () => {
  it("takes an id no other node has", () => {
    expect(nextNodeId(graph(), "for_each")).toBe("for_each");
    expect(
      nextNodeId(
        graph({
          nodes: [...graph().nodes, { id: "for_each", kind: "for_each", position: { x: 0, y: 0 } }],
        }),
        "for_each",
      ),
    ).toBe("for_each-2");
  });

  it("keeps the nodes it is given and places the new one where it was asked", () => {
    const withNode = addNode(graph(), "for_each", { x: 700, y: 240 }, { rejoin: "test" });
    expect(withNode.nodes).toHaveLength(3);
    expect(withNode.nodes[2]).toEqual({
      id: "for_each",
      kind: "for_each",
      params: { rejoin: "test" },
      position: { x: 700, y: 240 },
    });
    // The graph it was given is untouched.
    expect(graph().nodes).toHaveLength(2);
  });

  it("drops a node on a free row under the others", () => {
    expect(nextPosition(graph({ nodes: [] }))).toEqual({ x: 320, y: 120 });
    // Under the lowest node rather than beside it: the next one added must not land on this one.
    expect(nextPosition(graph())).toEqual({ x: 200, y: 300 });
  });
});

describe("moving and editing a node", () => {
  it("moves one node by its id and leaves the others", () => {
    const moved = moveNode(graph(), "test", { x: 10, y: 20 });
    expect(moved.nodes.map((node) => node.position)).toEqual([
      { x: 200, y: 120 },
      { x: 10, y: 20 },
    ]);
  });

  it("writes parameters whole, and writes none at all when nothing is left", () => {
    const edited = setParams(graph(), "test", { script: "pnpm test", timeout: 20 });
    expect(nodeOf(edited, "test")?.params).toEqual({ script: "pnpm test", timeout: 20 });
    expect(setParams(edited, "test", {}).nodes[1]?.params).toBeUndefined();
  });

  it("replaces the one trigger with the one the form shows", () => {
    const edited = setTrigger(graph(), { kind: "schedule", params: { cron: "0 7 * * 1" } });
    expect(edited.trigger).toEqual([{ kind: "schedule", params: { cron: "0 7 * * 1" } }]);
    expect(edited.nodes).toEqual(graph().nodes);
  });

  it("renames a node and re-points the edges that named it", () => {
    const withEdge = connect(graph(), { from: "build", to: "test", port: "out" }) ?? graph();
    const renamed = renameNode(withEdge, "test", "check");
    expect(renamed?.nodes.map((node) => node.id)).toEqual(["build", "check"]);
    expect(renamed?.edges).toEqual([
      { from: TRIGGER, to: "build", port: "out" },
      { from: "build", to: "check", port: "out" },
    ]);
  });

  it("refuses a name another node has, the trigger's, or none at all", () => {
    expect(renameNode(graph(), "test", "build")).toBeNull();
    expect(renameNode(graph(), "test", TRIGGER)).toBeNull();
    expect(renameNode(graph(), "test", "   ")).toBeNull();
  });
});

describe("joining and parting nodes", () => {
  it("joins two ports and keeps the port the edge leaves by", () => {
    const joined = connect(graph(), { from: "build", to: "test", port: "out" });
    expect(joined?.edges[1]).toEqual({ from: "build", to: "test", port: "out" });
    expect(edgeId(joined?.edges[1] ?? { from: "", to: "", port: "" })).toBe("build:out->test");
  });

  it("refuses an edge twice over and one into itself", () => {
    const joined = connect(graph(), { from: "build", to: "test", port: "out" }) ?? graph();
    expect(connect(joined, { from: "build", to: "test", port: "out" })).toBeNull();
    expect(connect(joined, { from: "build", to: "build", port: "out" })).toBeNull();
    // The same two nodes by another port are another edge.
    expect(connect(joined, { from: "build", to: "test", port: "on_fail" })).not.toBeNull();
    const backwards = joined.edges.filter((edge) => edge.from === "test" && edge.to === "build");
    expect(backwards).toEqual([]);
  });

  it("takes one edge away and leaves the rest", () => {
    const joined = connect(graph(), { from: "build", to: "test", port: "out" }) ?? graph();
    const parted = removeEdge(joined, { from: TRIGGER, to: "build", port: "out" });
    expect(parted.edges).toEqual([{ from: "build", to: "test", port: "out" }]);
  });

  it("takes a node away with every edge that named it", () => {
    const joined = connect(graph(), { from: "build", to: "test", port: "out" }) ?? graph();
    const gone = removeNode(joined, "build");
    expect(gone.nodes.map((node) => node.id)).toEqual(["test"]);
    expect(gone.edges).toEqual([]);
  });

  it("keeps the start of the graph", () => {
    expect(removeNode(graph(), TRIGGER)).toEqual(graph());
  });
});

describe("unsaved changes", () => {
  it("notices a moved node, since positions are saved too", () => {
    const saved = graph();
    expect(isDirty(saved, graph())).toBe(false);
    expect(isDirty(saved, moveNode(saved, "test", { x: 1, y: 1 }))).toBe(true);
  });

  it("notices an edited parameter and a new edge, and ignores the order things are written in", () => {
    const saved = graph();
    expect(isDirty(saved, setParams(saved, "test", { script: "pnpm test" }))).toBe(true);
    const joined = connect(saved, { from: "build", to: "test", port: "out" }) ?? saved;
    expect(isDirty(saved, joined)).toBe(true);
    const shuffled: WorkflowDefinition = {
      ...saved,
      nodes: [...saved.nodes].reverse(),
      edges: [...saved.edges],
    };
    expect(isDirty(saved, shuffled)).toBe(false);
  });

  it("ignores the order the keys of a parameter are written in", () => {
    const one = graph({
      nodes: [{ id: "a", kind: "script", params: { x: 1, y: 2 }, position: { x: 0, y: 0 } }],
    });
    const other = graph({
      nodes: [{ id: "a", kind: "script", params: { y: 2, x: 1 }, position: { x: 0, y: 0 } }],
    });
    expect(isDirty(one, other)).toBe(false);
  });
});

describe("a message the server refused a save with", () => {
  it("lands on the node it names", () => {
    const problem = problemFor(
      'workflow workflows/release.toml: node "test": unknown node kind "agent"',
    );
    expect(problem.on).toEqual({ node: "test" });
    expect(problem.text).toBe('node "test": unknown node kind "agent"');
  });

  it("lands on the edge it names", () => {
    const problem = problemFor(
      'workflow workflows/release.toml: edge "build" -> "test": no node "test"',
    );
    expect(problem.on).toEqual({ edge: { from: "build", to: "test", port: "" } });
  });

  it("lands on the start when it names the trigger, and nowhere else when it names nothing", () => {
    expect(
      problemFor('workflow workflows/release.toml: trigger: unknown node kind "missing"'),
    ).toEqual({
      on: { trigger: true },
      text: 'unknown node kind "missing"',
    });
    expect(
      problemFor("workflow workflows/release.toml: exactly one trigger is required, found none").on,
    ).toEqual({ nowhere: true });
  });
});
