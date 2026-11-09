/**
 * The definition registry, as the web app knows it: the node kinds a graph may be built from, what each asks for, and
 * the form it draws itself.
 *
 * The server's registry (`crates/moebius-core/src/workflows/nodes`) is the one that runs a graph, and the desktop API
 * has no route that reports it yet, so the palette and the generic parameter form read this file instead. A node-kind
 * card adds its kind here — its ports, the parameters it asks for, and, when the generic form does not suit it, the
 * form it draws itself in `nodeForms`. Nothing else in the canvas changes when a kind lands.
 *
 * A kind that is not listed here is not an error: a workflow file may name a kind this file has not caught up with, and
 * the canvas draws it with one port each way and edits its parameters as JSON, so an unknown kind survives a round
 * trip through the editor.
 */
import type { ComponentType } from "react";
import type { Trigger, WorkflowDefinition, WorkflowNode } from "../../api/types/workflows";
import { AgentForm } from "./forms/agent";
import { ApprovalForm } from "./forms/approval";
import { CodeForm } from "./forms/code";
import { ConditionForm } from "./forms/condition";
import { ForEachForm } from "./forms/for_each";
import { HttpForm } from "./forms/http";
import { MergeForm } from "./forms/merge";
import { PublishForm } from "./forms/publish";
import { ManualTriggerForm, ScheduleTriggerForm, WebhookTriggerForm } from "./forms/trigger";

/**
 * One parameter a kind asks for, and how to ask it. `node` is the id of another node of the graph; `list` is a list of
 * strings, one per line; `json` is any value, typed as JSON.
 */
export type ParamField =
  | {
      type: "text";
      name: string;
      label: string;
      help?: string;
      placeholder?: string;
      /** The value a new node starts with. */
      start?: string;
      required?: boolean;
    }
  | { type: "node"; name: string; label: string; help?: string; required?: boolean }
  | { type: "number"; name: string; label: string; help?: string; start?: number; min?: number }
  | { type: "boolean"; name: string; label: string; help?: string; start?: boolean }
  | { type: "list"; name: string; label: string; help?: string }
  | { type: "json"; name: string; label: string; help?: string };

/** A kind of node the palette offers. */
export interface NodeKind {
  /** The name the definition writes, and the server's registry answers for. */
  kind: string;
  /** What the palette and the node are headed with. */
  title: string;
  /** One line on what the kind does, shown under the palette entry. */
  description: string;
  inputs: string[];
  outputs: string[];
  /** The parameters the generic form is built from. */
  params: ParamField[];
}

/** Every kind the palette offers, in the order it offers them. */
export const nodeKinds: NodeKind[] = [
  {
    kind: "code",
    title: "Code",
    description: "Runs Luau that reshapes the value and outputs what it returns.",
    inputs: ["in"],
    outputs: ["out"],
    params: [
      {
        type: "text",
        name: "source",
        label: "Source",
        help: "Luau that gets the value as input and returns what the node gives back. Expressions such as {{ trigger.tag }} are filled before it runs.",
        placeholder: "return input",
        required: true,
      },
    ],
  },
  {
    kind: "condition",
    title: "Condition",
    description:
      "Sends the value down the first port whose expression is true, or the fallback port.",
    inputs: ["in"],
    outputs: [],
    params: [
      {
        type: "json",
        name: "ports",
        label: "Ports",
        help: "The ports and their expressions, tried in order.",
      },
      {
        type: "text",
        name: "fallback",
        label: "Fallback",
        help: "The port taken when no expression is true.",
      },
    ],
  },
  {
    kind: "for_each",
    title: "For each",
    description: "Runs a sub-path once for every element of a list.",
    inputs: ["in"],
    outputs: ["each", "done"],
    params: [
      {
        type: "node",
        name: "rejoin",
        label: "Rejoins at",
        help: "The node the sub-path comes back at; its output is what each element gives back.",
        required: true,
      },
      {
        type: "number",
        name: "max_elements",
        label: "Most elements",
        help: "The most elements one run of the node may fan out over. Left alone, a hundred.",
        min: 0,
      },
    ],
  },
  {
    kind: "http",
    title: "HTTP",
    description: "Calls a URL, with a named secret when it needs one, and outputs the reply.",
    inputs: ["in"],
    outputs: ["out"],
    params: [
      {
        type: "text",
        name: "url",
        label: "URL",
        help: "Where the node calls. Expressions such as {{ trigger.tag }} are filled before it runs.",
        required: true,
      },
      {
        type: "text",
        name: "method",
        label: "Method",
        help: "The HTTP method the call uses; GET when it is left alone.",
      },
      {
        type: "text",
        name: "secret",
        label: "Secret",
        help: "The name of a secret the server holds, never its value.",
      },
      {
        type: "number",
        name: "timeout_seconds",
        label: "Timeout",
        help: "How long the call may take, in seconds.",
        min: 1,
      },
    ],
  },
  {
    kind: "agent",
    title: "Agent",
    description:
      "Runs an agent of the harness configuration repository in a worktree the run owns.",
    inputs: ["in"],
    outputs: [],
    params: [
      {
        type: "text",
        name: "agent",
        label: "Agent",
        help: "The agent that runs.",
        required: true,
      },
      {
        type: "text",
        name: "prompt",
        label: "Prompt",
        help: "What the agent reads. Expressions such as {{ trigger.tag }} are filled before it starts.",
        required: true,
      },
      {
        type: "text",
        name: "output",
        label: "Output",
        help: "The name of the port the node's value leaves by.",
        required: true,
      },
    ],
  },
  {
    kind: "approval",
    title: "Approval",
    description: "Waits for a person to answer before the run goes on.",
    inputs: ["in"],
    outputs: ["out", "failed"],
    params: [
      {
        type: "text",
        name: "prompt",
        label: "Prompt",
        help: "What the person deciding reads.",
        required: true,
      },
      {
        type: "number",
        name: "timeout_seconds",
        label: "Deadline",
        help: "How long the run waits for an answer, in seconds.",
        min: 1,
      },
    ],
  },
  {
    kind: "publish",
    title: "Publish",
    description: "Publishes a change with a title and a body.",
    inputs: ["in"],
    outputs: ["out"],
    params: [
      {
        type: "text",
        name: "title",
        label: "Title",
        help: "The title of what the node publishes.",
        required: true,
      },
      {
        type: "text",
        name: "body",
        label: "Body",
        help: "The body that goes with it.",
        required: true,
      },
    ],
  },
  {
    kind: "merge",
    title: "Merge",
    description: "Joins two branches and emits what arrived by each side.",
    inputs: ["a", "b"],
    outputs: ["out"],
    params: [
      {
        type: "node",
        name: "a",
        label: "Side A",
        help: "The node whose value arrives by side a.",
        required: true,
      },
      {
        type: "node",
        name: "b",
        label: "Side B",
        help: "The node whose value arrives by side b.",
        required: true,
      },
      {
        type: "boolean",
        name: "both",
        label: "Both sides",
        help: "Emit every side that arrived, not only one.",
      },
    ],
  },
];

/**
 * The ports a kind has; a kind this file does not know has one each way. A condition's ports are the ones its
 * parameters declare, so its card follows the form beside it.
 */
export function portsOf(kind: string, params?: unknown): { inputs: string[]; outputs: string[] } {
  const known = nodeKinds.find((entry) => entry.kind === kind);
  if (!known) {
    return { inputs: ["in"], outputs: ["out"] };
  }
  if (kind === "condition") {
    return { inputs: known.inputs, outputs: conditionPorts(params) };
  }
  if (kind === "agent") {
    return { inputs: known.inputs, outputs: agentPorts(params) };
  }
  return { inputs: known.inputs, outputs: known.outputs };
}

/** The names a condition's parameters give its ports, in the order it tries them, with the fallback last. */
function conditionPorts(params: unknown): string[] {
  const given =
    typeof params === "object" && params !== null ? (params as Record<string, unknown>) : {};
  const outputs = Array.isArray(given.ports)
    ? given.ports.flatMap((entry): string[] => {
        if (typeof entry !== "object" || entry === null) {
          return [];
        }
        const name = (entry as Record<string, unknown>).name;
        return typeof name === "string" && name !== "" ? [name] : [];
      })
    : [];
  if (
    typeof given.fallback === "string" &&
    given.fallback !== "" &&
    !outputs.includes(given.fallback)
  ) {
    outputs.push(given.fallback);
  }
  return outputs;
}

/** A palette entry for a kind the registry does not describe, drawn with one port each way. */
export function genericKind(kind: string): NodeKind {
  return {
    kind,
    title: kind,
    description: "A kind this web app has no form for yet; its parameters are edited as JSON.",
    inputs: ["in"],
    outputs: ["out"],
    params: [],
  };
}

/**
 * What the palette offers: the kinds of the registry and, after them, the kinds this graph already has nodes of that
 * the registry does not describe, so a graph whose kinds have not landed here yet stays editable.
 */
export function paletteKinds(graph: WorkflowDefinition): NodeKind[] {
  const extra: NodeKind[] = [];
  const seen = new Set(nodeKinds.map((entry) => entry.kind));
  for (const node of graph.nodes) {
    if (seen.has(node.kind)) {
      continue;
    }
    seen.add(node.kind);
    extra.push(genericKind(node.kind));
  }
  return [...nodeKinds, ...extra];
}

/** The parameters a node of a kind starts with. */
export function startingParams(kind: NodeKind): Record<string, unknown> {
  const params: Record<string, unknown> = {};
  for (const field of kind.params) {
    if (field.type === "text" && field.start !== undefined) {
      params[field.name] = field.start;
    } else if (field.type === "number" && field.start !== undefined) {
      params[field.name] = field.start;
    } else if (field.type === "boolean" && field.start !== undefined) {
      params[field.name] = field.start;
    }
  }
  return params;
}

/** What a kind's own form is given: its node, the graph around it, and the way to give parameters back. */
export interface NodeFormProps {
  node: WorkflowNode;
  graph: WorkflowDefinition;
  onParams: (params: Record<string, unknown>) => void;
}

/**
 * The forms kinds draw themselves. A kind left out is drawn by the generic parameter form from the fields it reports,
 * which is where a kind with nothing special about it stays.
 */
export const nodeForms: Record<string, ComponentType<NodeFormProps>> = {
  agent: AgentForm,
  approval: ApprovalForm,
  code: CodeForm,
  condition: ConditionForm,
  for_each: ForEachForm,
  http: HttpForm,
  merge: MergeForm,
  publish: PublishForm,
};

/** The form a kind draws itself, when it has one. */
export function formFor(kind: string): ComponentType<NodeFormProps> | undefined {
  return nodeForms[kind];
}

/** The names an agent node's parameters give its output port; the kind's own name when it declares none. */
function agentPorts(params: unknown): string[] {
  const given =
    typeof params === "object" && params !== null ? (params as Record<string, unknown>) : {};
  return typeof given.output === "string" && given.output !== "" ? [given.output] : ["out"];
}

/** What a trigger's own form is given: the trigger and the way to give its parameters back. */
export interface TriggerFormProps {
  trigger: Trigger;
  onTrigger: (trigger: Trigger) => void;
}

/** The forms the triggers draw themselves. A trigger left out is drawn as its JSON, as before. */
export const triggerForms: Record<string, ComponentType<TriggerFormProps>> = {
  manual: ManualTriggerForm,
  schedule: ScheduleTriggerForm,
  webhook: WebhookTriggerForm,
};

/** The form a trigger draws itself, when it has one. */
export function triggerFormFor(kind: string): ComponentType<TriggerFormProps> | undefined {
  return triggerForms[kind];
}
