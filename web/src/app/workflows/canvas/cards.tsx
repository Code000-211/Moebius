/** The cards on the canvas: a node, and the start of the graph. */
import { Handle, type NodeProps, Position } from "@xyflow/react";
import type { Trigger } from "../../../api/types/workflows";
import type { WorkflowNodeRun } from "../../../api/workflows";
import { Tag } from "../../../ui/chat/Tag";
import { cn } from "../../../ui/lib/cn";
import { runStatus } from "../model";

/** What a card on the canvas is given to draw. */
export interface CardData extends Record<string, unknown> {
  /** The node's own id, or `trigger` for the start. */
  id: string;
  kind: string;
  /** What the card is headed with. */
  title: string;
  /** One line of the parameters, so a person recognises a node without opening it. */
  summary: string;
  inputs: string[];
  outputs: string[];
  /** The start of the graph draws its trigger rather than ports and parameters. */
  trigger?: Trigger;
  /** The messages the server refused a save with that belong to this card. */
  problems: string[];
  /** The node's part of the run a person is looking at, when one is. */
  run?: WorkflowNodeRun;
}

const WIDTH = "w-56";

/** A node of the graph: its kind, what it is given, the ports an edge joins, and how it went in a picked run. */
export function NodeCard({ data, selected }: NodeProps) {
  const card = data as unknown as CardData;
  return (
    <div
      data-testid="canvas-node"
      data-node-id={card.id}
      className={cn(
        "rounded-card bg-surface px-3 py-3 shadow-card transition-shadow duration-(--motion-routine)",
        WIDTH,
        selected && "canvas-node--selected",
        card.problems.length > 0 && "canvas-node--problem",
      )}
    >
      <CardHead id={card.id} kind={card.kind} run={card.run} />
      <p className="mt-1 truncate font-mono text-xs text-secondary" title={card.summary}>
        {card.summary === "" ? "Takes no parameters" : card.summary}
      </p>
      <Problems problems={card.problems} />
      <RunLine run={card.run} />
      <Ports inputs={card.inputs} outputs={card.outputs} />
    </div>
  );
}

/** Where the graph starts. It is not a node of the definition, so it has no ports of its own to leave by. */
export function TriggerCard({ data, selected }: NodeProps) {
  const card = data as unknown as CardData;
  const trigger = card.trigger;
  return (
    <div
      data-testid="canvas-node"
      data-node-id={card.id}
      className={cn(
        "rounded-card border border-hairline bg-sunken px-3 py-3",
        WIDTH,
        selected && "canvas-node--selected",
      )}
    >
      <p className="m-0 text-xs font-semibold uppercase tracking-wide text-secondary">Trigger</p>
      <p className="mt-1 m-0 truncate text-sm text-text">{card.title}</p>
      <p className="mt-1 m-0 truncate font-mono text-xs text-secondary">
        {trigger ? JSON.stringify(trigger.params ?? {}) : "This workflow has no trigger."}
      </p>
      <Problems problems={card.problems} />
      <Handle
        type="source"
        position={Position.Right}
        id="out"
        className="canvas-handle !right-[-7px]"
      />
    </div>
  );
}

function CardHead({ id, kind, run }: { id: string; kind: string; run?: WorkflowNodeRun }) {
  return (
    <div className="flex min-w-0 items-center gap-2">
      <span className="min-w-0 flex-1 truncate text-sm font-medium text-text" title={id}>
        {id}
      </span>
      {run ? (
        <Tag tone={runStatus(run.status).tone}>{runStatus(run.status).label}</Tag>
      ) : (
        <span className="shrink-0 font-mono text-xs text-secondary">{kind}</span>
      )}
    </div>
  );
}

function Problems({ problems }: { problems: string[] }) {
  if (problems.length === 0) {
    return null;
  }
  return (
    <ul className="mt-2 m-0 flex list-none flex-col gap-1 p-0">
      {problems.map((problem) => (
        <li key={problem} className="text-xs leading-snug text-error-text">
          {problem}
        </li>
      ))}
    </ul>
  );
}

/** What a node was given and what it gave back, at the node it happened to. */
function RunLine({ run }: { run?: WorkflowNodeRun }) {
  if (!run) {
    return null;
  }
  return (
    <dl className="mt-2 m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-1">
      <dt className="text-xs text-secondary">In</dt>
      <dd className="m-0 truncate font-mono text-xs text-secondary" title={short(run.input)}>
        {short(run.input)}
      </dd>
      <dt className="text-xs text-secondary">Out</dt>
      <dd className="m-0 truncate font-mono text-xs text-secondary" title={short(run.output)}>
        {short(run.output)}
      </dd>
    </dl>
  );
}

function Ports({ inputs, outputs }: { inputs: string[]; outputs: string[] }) {
  return (
    <>
      {inputs.map((port, index) => (
        <Handle
          key={port}
          type="target"
          position={Position.Left}
          id={port}
          title={`Comes in by ${port}`}
          className="canvas-handle !left-[-7px]"
          style={{ top: `${36 + index * 18}px` }}
        />
      ))}
      {outputs.map((port, index) => (
        <div
          key={port}
          className="pointer-events-none absolute right-2 flex items-center gap-2 text-xs text-secondary"
          style={{ top: `${28 + index * 16}px` }}
        >
          <span>{port}</span>
          <Handle
            type="source"
            position={Position.Right}
            id={port}
            title={`Leaves by ${port}`}
            className="canvas-handle pointer-events-auto !absolute !right-[-14px]"
          />
        </div>
      ))}
    </>
  );
}

function short(value: unknown): string {
  if (value === null || value === undefined) {
    return "—";
  }
  const text = typeof value === "string" ? value : JSON.stringify(value);
  return text.length > 60 ? `${text.slice(0, 57)}…` : text;
}
