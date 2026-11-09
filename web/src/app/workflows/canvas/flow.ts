/** The graph as the canvas draws it: cards for nodes, edges by port, and the start where the nodes begin. */
import type { Edge, Node } from "@xyflow/react";
import type { WorkflowDefinition, WorkflowEdge } from "../../../api/types/workflows";
import type { WorkflowNodeRun } from "../../../api/workflows";
import { portsOf } from "../nodeKinds";
import type { CardData } from "./cards";
import { edgeId, nodeOf, type Problem, TRIGGER, triggerPorts } from "./model";

/** A card of the board: a node of the graph, or its start. */
export type CardNode = Node<CardData, "kind" | "trigger">;

/** How far the start sits to the left of the nodes it leads into. */
const TRIGGER_GAP = 300;

/** What the cards are marked with besides their own parameters. */
export interface Marks {
  /** The node a person has chosen, and the edge they have chosen. */
  node?: string | null;
  edge?: string | null;
  /** The messages of a refused save, by the node they belong to. */
  problems?: Record<string, string[]>;
  /** A message about the start of the graph. */
  triggerProblem?: string;
  /** The run a person is looking at, by the node it reached. */
  runs?: Record<string, WorkflowNodeRun>;
  /** Where the start was dragged to. It is not a node of the definition, so its place is the canvas's own. */
  triggerPosition?: { x: number; y: number };
}

/** The cards: every node of the definition, and the start of the graph before them. */
export function flowNodes(graph: WorkflowDefinition, marks: Marks = {}): CardNode[] {
  const trigger = graph.trigger[0];
  const nodes: CardNode[] = [
    {
      id: TRIGGER,
      type: "trigger",
      position: marks.triggerPosition ?? triggerPosition(graph),
      draggable: true,
      // The start of the graph belongs to the file, not to the canvas: it cannot be deleted here.
      deletable: false,
      data: {
        id: TRIGGER,
        kind: trigger?.kind ?? "none",
        title: trigger ? `${trigger.kind}` : "No trigger",
        summary: "",
        inputs: triggerPorts().inputs,
        outputs: triggerPorts().outputs,
        trigger,
        problems: marks.triggerProblem ? [marks.triggerProblem] : [],
        run: marks.runs?.[TRIGGER],
      } satisfies CardData,
      selected: marks.node === TRIGGER,
    },
  ];
  for (const node of graph.nodes) {
    const ports = portsOf(node.kind, node.params);
    const data: CardData = {
      id: node.id,
      kind: node.kind,
      title: node.id,
      summary: summaryOf(node.params),
      inputs: ports.inputs,
      outputs: ports.outputs,
      problems: marks.problems?.[node.id] ?? [],
      run: marks.runs?.[node.id],
    };
    nodes.push({
      id: node.id,
      type: "kind",
      position: node.position,
      data,
      selected: marks.node === node.id,
    });
  }
  return nodes;
}

/** The edges, each one labelled with the port it leaves by and marked when a refused save names it. */
export function flowEdges(graph: WorkflowDefinition, marks: Marks = {}): Edge[] {
  return graph.edges.map((edge) => {
    const id = edgeId(edge);
    const problem = problemForEdge(marks.problems, edge);
    return {
      id,
      source: edge.from,
      target: edge.to,
      sourceHandle: edge.port,
      targetHandle: targetPort(graph, edge),
      label: edge.port,
      labelShowBg: true,
      selected: marks.edge === id,
      data: { problem },
      style: problem ? { stroke: "var(--error)" } : undefined,
    } satisfies Edge;
  });
}

/** Where the start goes when nobody has moved it: to the left of the nodes it leads into. */
export function triggerPosition(graph: WorkflowDefinition): { x: number; y: number } {
  if (graph.nodes.length === 0) {
    return { x: 0, y: 120 };
  }
  const left = Math.min(...graph.nodes.map((node) => node.position.x));
  const middle = graph.nodes.reduce((sum, node) => sum + node.position.y, 0) / graph.nodes.length;
  return { x: left - TRIGGER_GAP, y: Math.round(middle) };
}

/**
 * The port an edge comes in by. A definition names only the port it leaves by, and a node of the registry takes its
 * one input; an edge that comes to the start, which cannot be drawn, is left for the server to refuse.
 */
function targetPort(graph: WorkflowDefinition, edge: WorkflowEdge): string | null {
  if (edge.to === TRIGGER) {
    return null;
  }
  const node = nodeOf(graph, edge.to);
  if (!node) {
    return null;
  }
  return portsOf(node.kind, node.params).inputs[0] ?? "in";
}

function problemForEdge(
  problems: Record<string, string[]> | undefined,
  edge: WorkflowEdge,
): string | undefined {
  for (const message of problems?.[edge.from] ?? []) {
    if (message.includes(`edge "${edge.from}" -> "${edge.to}"`)) {
      return message;
    }
  }
  return undefined;
}

/** One line of a node's parameters, so a person recognises it without opening the panel. */
export function summaryOf(params: unknown): string {
  if (!params || typeof params !== "object") {
    return "";
  }
  return Object.entries(params as Record<string, unknown>)
    .map(([name, value]) => `${name}: ${typeof value === "string" ? value : JSON.stringify(value)}`)
    .join(" · ");
}

/**
 * The messages of a refused save, by the node that carries them. An edge's message goes on the node it leaves from,
 * which is the one a person can move; a message about the start is not carried by a node at all.
 */
export function problemsOf(problem: Problem | null): Record<string, string[]> {
  if (!problem) {
    return {};
  }
  if ("node" in problem.on) {
    return { [problem.on.node]: [problem.text] };
  }
  if ("edge" in problem.on) {
    return { [problem.on.edge.from]: [problem.text] };
  }
  return {};
}
