/**
 * The graph the canvas edits, and the edit operations on it.
 *
 * The graph is a workflow's parsed form as the server answers it and takes it back: nodes with their parameters and
 * positions, edges by port, and the triggers. Every edit here is a plain function over it, so the canvas holds one
 * piece of state — the graph a person is editing — and saving sends exactly what is here. The trigger is not a node of
 * the definition, but edges leave it by name, so it has a place on the graph: `TRIGGER`, the id the engine gives it.
 */
import type { WorkflowDefinition, WorkflowEdge, WorkflowNode } from "../../../api/types/workflows";

/** The id the engine goes by for the start of the graph; no node of the definition may take it. */
export const TRIGGER = "trigger";

/**
 * A position for a node added with no place of its own: on a free row under everything on the graph, in line with the
 * leftmost node. Beside the last node would do for one node, but a second one added lands on the first, and a node
 * half over another cannot have its ports held down.
 */
export function nextPosition(graph: WorkflowDefinition): { x: number; y: number } {
  if (graph.nodes.length === 0) {
    return { x: 320, y: 120 };
  }
  const left = Math.min(...graph.nodes.map((node) => node.position.x));
  const below = Math.max(...graph.nodes.map((node) => node.position.y));
  return { x: left, y: below + 180 };
}

/** An id for a new node of a kind that no node of the graph has taken yet. */
export function nextNodeId(graph: WorkflowDefinition, kind: string): string {
  const base = kind.replace(/[^a-zA-Z0-9_-]/g, "-");
  const taken = new Set(graph.nodes.map((node) => node.id));
  if (!taken.has(base)) {
    return base;
  }
  let n = 2;
  while (taken.has(`${base}-${n}`)) {
    n += 1;
  }
  return `${base}-${n}`;
}

/** A node for a kind, at a position, with an id no other node has. */
export function addNode(
  graph: WorkflowDefinition,
  kind: string,
  position: { x: number; y: number },
  params?: Record<string, unknown>,
): WorkflowDefinition {
  const node: WorkflowNode = {
    id: nextNodeId(graph, kind),
    kind,
    params: params ?? {},
    position,
  };
  return { ...graph, nodes: [...graph.nodes, node] };
}

export function moveNode(
  graph: WorkflowDefinition,
  id: string,
  position: { x: number; y: number },
): WorkflowDefinition {
  return {
    ...graph,
    nodes: graph.nodes.map((node) => (node.id === id ? { ...node, position } : node)),
  };
}

/** Replaces the workflow's one trigger with the one the form now shows. */
export function setTrigger(
  graph: WorkflowDefinition,
  trigger: WorkflowDefinition["trigger"][number],
): WorkflowDefinition {
  return { ...graph, trigger: [trigger] };
}

/** Sets a node's parameters whole; a node that keeps none is written with none. */
export function setParams(
  graph: WorkflowDefinition,
  id: string,
  params: Record<string, unknown>,
): WorkflowDefinition {
  const clean = Object.keys(params).length === 0 ? undefined : params;
  return {
    ...graph,
    nodes: graph.nodes.map((node) =>
      node.id === id
        ? { ...node, id, kind: node.kind, position: node.position, params: clean }
        : node,
    ),
  };
}

/** Renames a node, and re-points every edge that named it. False when the name is taken or is the trigger's. */
export function renameNode(
  graph: WorkflowDefinition,
  from: string,
  to: string,
): WorkflowDefinition | null {
  const id = to.trim();
  if (id === "" || id === TRIGGER || (id !== from && graph.nodes.some((node) => node.id === id))) {
    return null;
  }
  return {
    ...graph,
    nodes: graph.nodes.map((node) => (node.id === from ? { ...node, id } : node)),
    edges: graph.edges.map((edge) => ({
      ...edge,
      from: edge.from === from ? id : edge.from,
      to: edge.to === from ? id : edge.to,
    })),
  };
}

/** An edge a person drew between two ports. Refused for an edge the graph already has or one into itself. */
export function connect(graph: WorkflowDefinition, edge: WorkflowEdge): WorkflowDefinition | null {
  if (edge.from === edge.to) {
    return null;
  }
  if (edgeExists(graph, edge)) {
    return null;
  }
  return { ...graph, edges: [...graph.edges, { ...edge }] };
}

export function removeEdge(graph: WorkflowDefinition, edge: WorkflowEdge): WorkflowDefinition {
  return { ...graph, edges: graph.edges.filter((other) => !sameEdge(edge, other)) };
}

/** Takes a node away, and with it every edge that left from it or came to it. The trigger stays. */
export function removeNode(graph: WorkflowDefinition, id: string): WorkflowDefinition {
  if (id === TRIGGER) {
    return graph;
  }
  return {
    ...graph,
    nodes: graph.nodes.filter((node) => node.id !== id),
    edges: graph.edges.filter((edge) => edge.from !== id && edge.to !== id),
  };
}

function sameEdge(a: WorkflowEdge, b: WorkflowEdge): boolean {
  return a.from === b.from && a.to === b.to && a.port === b.port;
}

function edgeExists(graph: WorkflowDefinition, edge: WorkflowEdge): boolean {
  return graph.edges.some((other) => sameEdge(edge, other));
}

/**
 * One string for one edge: the node it leaves, the port it leaves by, and the node it comes to. The board keys its
 * edges by it, so a change to the graph and the edge the library names are the same thing.
 */
export function edgeId(edge: WorkflowEdge): string {
  return `${edge.from}:${edge.port}->${edge.to}`;
}

/** A node of the graph by its id. The trigger is no node of the definition and is never found this way. */
export function nodeOf(graph: WorkflowDefinition, id: string): WorkflowNode | undefined {
  return graph.nodes.find((node) => node.id === id);
}

/** The port an edge leaves its source by; the trigger has the one it has always had. */
export function triggerPorts(): { inputs: string[]; outputs: string[] } {
  return { inputs: [], outputs: ["out"] };
}

/** Whether a person has changed the graph since it was saved; positions count, as they are saved too. */
export function isDirty(before: WorkflowDefinition, after: WorkflowDefinition): boolean {
  return canonical(before) !== canonical(after);
}

/** A graph as one string, with nodes, edges and parameters in a set order, so only real changes differ. */
function canonical(graph: WorkflowDefinition): string {
  const nodes = [...graph.nodes]
    .sort((a, b) => a.id.localeCompare(b.id))
    .map((node) => ({
      id: node.id,
      kind: node.kind,
      position: { x: Math.round(node.position.x), y: Math.round(node.position.y) },
      params: canonicalValue(node.params ?? {}),
    }));
  const edges = [...graph.edges]
    .sort((a, b) => edgeId(a).localeCompare(edgeId(b)))
    .map((edge) => ({ ...edge }));
  return JSON.stringify({
    description: graph.description,
    trigger: canonicalValue(graph.trigger),
    nodes,
    edges,
  });
}

function canonicalValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(canonicalValue);
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return Object.keys(record)
      .sort()
      .reduce<Record<string, unknown>>((out, key) => {
        out[key] = canonicalValue(record[key]);
        return out;
      }, {});
  }
  return value;
}

/** Where a message the server answered belongs on the graph. */
export interface Problem {
  /** The node to mark, `edge` for both ends of an edge, or the trigger. */
  on: { node: string } | { edge: WorkflowEdge } | { trigger: true } | { nowhere: true };
  /** The message without the file it was answered for, in the words the server chose. */
  text: string;
}

/**
 * Places the message of a refused save on the node it names. The server answers `workflow <file>: <message>` for the
 * first thing that is wrong, and the message names a node (`node "build": ...`), an edge
 * (`edge "build" -> "test": ...`) or the start (`trigger: ...`).
 */
export function problemFor(message: string): Problem {
  const text = message.replace(/^workflow [^:]+: /, "");
  const node = /^node "([^"]+)"/.exec(text);
  if (node?.[1]) {
    return { on: { node: node[1] }, text };
  }
  const edge = /^edge "([^"]+)" -> "([^"]+)"/.exec(text);
  if (edge?.[1] && edge[2]) {
    return { on: { edge: { from: edge[1], to: edge[2], port: "" } }, text };
  }
  if (text.startsWith("trigger:")) {
    return { on: { trigger: true }, text: text.slice("trigger: ".length) };
  }
  return { on: { nowhere: true }, text };
}
