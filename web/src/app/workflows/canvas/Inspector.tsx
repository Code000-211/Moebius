/** The panel beside the graph: what the person has chosen, and what is done to it. */
import { TrashIcon } from "@phosphor-icons/react";
import { type ReactNode, useEffect, useState } from "react";
import type {
  Trigger,
  WorkflowDefinition,
  WorkflowEdge,
  WorkflowNode,
} from "../../../api/types/workflows";
import type { WorkflowNodeRun } from "../../../api/workflows";
import { Button } from "../../../ui/components/Button";
import { Input } from "../../../ui/components/Input";
import { Field } from "../../../ui/page/Field";
import { JsonValue } from "../../../ui/page/JsonValue";
import { formFor, nodeKinds } from "../nodeKinds";
import { nodeOf, TRIGGER } from "./model";
import { ParamForm } from "./ParamForm";

/** What the panel is showing: one node, one edge, or the start of the graph; nothing when the pane is empty. */
export type Selection =
  | { kind: "node"; id: string }
  | { kind: "edge"; edge: WorkflowEdge }
  | { kind: "trigger" }
  | null;

export function Inspector({
  graph,
  selection,
  run,
  onRename,
  onParams,
  onDeleteNode,
  onDeleteEdge,
}: {
  graph: WorkflowDefinition;
  selection: Selection;
  /** The chosen node's part of the run a person is looking at, when one is picked. */
  run?: WorkflowNodeRun | null;
  onRename: (from: string, to: string) => void;
  onParams: (id: string, params: Record<string, unknown>) => void;
  onDeleteNode: (id: string) => void;
  onDeleteEdge: (edge: WorkflowEdge) => void;
}) {
  return (
    <aside
      data-testid="canvas-inspector"
      className="flex w-80 shrink-0 flex-col gap-4 overflow-y-auto border-l border-hairline bg-surface p-4"
    >
      <Body
        graph={graph}
        selection={selection}
        run={run ?? null}
        onRename={onRename}
        onParams={onParams}
        onDeleteNode={onDeleteNode}
        onDeleteEdge={onDeleteEdge}
      />
    </aside>
  );
}

function Body({
  graph,
  selection,
  run,
  onRename,
  onParams,
  onDeleteNode,
  onDeleteEdge,
}: Parameters<typeof Inspector>[0] & { run: WorkflowNodeRun | null }) {
  if (selection === null) {
    return (
      <p className="m-0 text-sm text-secondary">
        Choose a node to edit what it is given, or an edge to take it away.
      </p>
    );
  }
  if (selection.kind === "trigger") {
    return <TriggerPanel trigger={graph.trigger[0]} />;
  }
  if (selection.kind === "edge") {
    return <EdgePanel edge={selection.edge} onDelete={() => onDeleteEdge(selection.edge)} />;
  }
  const node = nodeOf(graph, selection.id);
  if (!node) {
    return <p className="m-0 text-sm text-secondary">This node is no longer on the graph.</p>;
  }
  return (
    <NodePanel
      node={node}
      graph={graph}
      run={run}
      onRename={(to) => onRename(node.id, to)}
      onParams={(params) => onParams(node.id, params)}
      onDelete={() => onDeleteNode(node.id)}
    />
  );
}

function Head({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <div className="flex min-w-0 flex-col">
      <h2 className="m-0 truncate font-mono text-base font-semibold text-text">{title}</h2>
      <p className="m-0 text-xs text-secondary">{subtitle}</p>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2 border-t border-hairline pt-3">
      <h3 className="m-0 text-xs font-semibold uppercase tracking-wide text-secondary">{title}</h3>
      {children}
    </section>
  );
}

function NodePanel({
  node,
  graph,
  run,
  onRename,
  onParams,
  onDelete,
}: {
  node: WorkflowNode;
  graph: WorkflowDefinition;
  run: WorkflowNodeRun | null;
  onRename: (to: string) => void;
  onParams: (params: Record<string, unknown>) => void;
  onDelete: () => void;
}) {
  const kind = nodeKinds.find((entry) => entry.kind === node.kind);
  const Custom = formFor(node.kind);
  return (
    <>
      <Head
        title={node.id}
        subtitle={kind ? kind.description : "A kind the registry does not describe"}
      />
      <Field label="Id" hint="The name the file writes the node by, and the edges to it.">
        <IdField node={node} graph={graph} onRename={onRename} />
      </Field>
      {Custom ? (
        <Custom node={node} graph={graph} onParams={onParams} />
      ) : (
        <ParamForm
          key={node.id}
          node={node}
          graph={graph}
          fields={kind?.params ?? []}
          known={kind !== undefined}
          onParams={onParams}
        />
      )}
      {run === null ? null : (
        <Section title="In the picked run">
          <div>
            <p className="m-0 mb-1 text-xs text-secondary">Input</p>
            <JsonValue value={run.input} name="input" />
          </div>
          <div>
            <p className="m-0 mb-1 text-xs text-secondary">Output</p>
            <JsonValue value={run.output} name="output" />
          </div>
          {run.error ? <p className="m-0 text-sm text-error-text">{run.error}</p> : null}
        </Section>
      )}
      <div>
        <Button variant="danger" size="sm" onClick={onDelete} data-testid="delete-node">
          <TrashIcon size={16} />
          Delete node
        </Button>
      </div>
    </>
  );
}

/** The node's id, given back only when it is a name the graph can take. */
function IdField({
  node,
  graph,
  onRename,
}: {
  node: WorkflowNode;
  graph: WorkflowDefinition;
  onRename: (to: string) => void;
}) {
  const [draft, setDraft] = useState(node.id);
  useEffect(() => setDraft(node.id), [node.id]);
  const trimmed = draft.trim();
  const error =
    trimmed === ""
      ? "A node needs an id."
      : trimmed === TRIGGER
        ? "The trigger's id is reserved."
        : trimmed !== node.id && graph.nodes.some((other) => other.id === trimmed)
          ? "Another node already has this id."
          : undefined;
  return (
    <>
      <Input
        aria-label="Id"
        mono
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => {
          if (error === undefined && trimmed !== node.id) {
            onRename(draft);
          }
        }}
      />
      {error ? (
        <p className="mt-1 m-0 text-xs text-error-text" data-testid="id-problem">
          {error}
        </p>
      ) : null}
    </>
  );
}

function EdgePanel({ edge, onDelete }: { edge: WorkflowEdge; onDelete: () => void }) {
  return (
    <>
      <Head title={`${edge.from} → ${edge.to}`} subtitle="An edge of the graph" />
      <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm">
        <dt className="text-secondary">Leaves</dt>
        <dd className="m-0 font-mono text-text">{edge.from}</dd>
        <dt className="text-secondary">By port</dt>
        <dd className="m-0 font-mono text-text">{edge.port}</dd>
        <dt className="text-secondary">Comes to</dt>
        <dd className="m-0 font-mono text-text">{edge.to}</dd>
      </dl>
      <div>
        <Button variant="danger" size="sm" onClick={onDelete} data-testid="delete-edge">
          <TrashIcon size={16} />
          Delete edge
        </Button>
      </div>
    </>
  );
}

function TriggerPanel({ trigger }: { trigger: Trigger | undefined }) {
  return (
    <>
      <Head title="Trigger" subtitle={trigger ? trigger.kind : "This workflow has none"} />
      {trigger ? (
        <Section title="What it is given">
          <JsonValue value={trigger.params ?? {}} name="trigger" />
        </Section>
      ) : (
        <p className="m-0 text-sm text-secondary">
          A workflow needs exactly one trigger, and this file has none.
        </p>
      )}
    </>
  );
}
