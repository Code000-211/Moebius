/**
 * The pane a chosen step opens in, at the right of the canvas (`InspectorPane`): a node, the trigger or an edge. The
 * edits are a draft held in the pane; Save gives them to the graph, and the pane guards the draft when it is closed.
 */
import { type ReactNode, useState } from "react";
import type {
  Trigger,
  WorkflowDefinition,
  WorkflowEdge,
  WorkflowNode,
  WorkflowNodeRun,
} from "../../../api/types/workflows";
import { DeleteButton } from "../../../ui/components/EditorWindow";
import { Input } from "../../../ui/components/Input";
import { Field } from "../../../ui/page/Field";
import { JsonValue } from "../../../ui/page/JsonValue";
import { triggerProblem } from "../forms/trigger";
import { formFor, nodeKinds, triggerFormFor } from "../nodeKinds";
import { InspectorPane } from "./InspectorPane";
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
  project,
  selection,
  run,
  onSaveNode,
  onSaveTrigger,
  onDeleteNode,
  onDeleteEdge,
  onClose,
}: {
  graph: WorkflowDefinition;
  /** The project of the workflow, when known. */
  project?: number;
  selection: Selection;
  /** The chosen node's part of the run a person is looking at, when one is picked. */
  run?: WorkflowNodeRun | null;
  /** The node's new id and parameters, once the window's Save is pressed. */
  onSaveNode: (from: string, to: string, params: Record<string, unknown>) => void;
  onSaveTrigger: (trigger: Trigger) => void;
  onDeleteNode: (id: string) => void;
  onDeleteEdge: (edge: WorkflowEdge) => void;
  onClose: () => void;
}) {
  if (selection === null) {
    return null;
  }
  if (selection.kind === "trigger") {
    return (
      <TriggerWindow
        trigger={graph.trigger[0]}
        project={project}
        onSave={(trigger) => {
          onSaveTrigger(trigger);
          onClose();
        }}
        onClose={onClose}
      />
    );
  }
  if (selection.kind === "edge") {
    return (
      <EdgeWindow
        edge={selection.edge}
        onDelete={() => onDeleteEdge(selection.edge)}
        onClose={onClose}
      />
    );
  }
  const node = nodeOf(graph, selection.id);
  if (!node) {
    return null;
  }
  return (
    <NodeWindow
      key={node.id}
      node={node}
      graph={graph}
      run={run ?? null}
      onSave={(to, params) => {
        onSaveNode(node.id, to, params);
        onClose();
      }}
      onDelete={() => onDeleteNode(node.id)}
      onClose={onClose}
    />
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

function NodeWindow({
  node,
  graph,
  run,
  onSave,
  onDelete,
  onClose,
}: {
  node: WorkflowNode;
  graph: WorkflowDefinition;
  run: WorkflowNodeRun | null;
  onSave: (id: string, params: Record<string, unknown>) => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  const kind = nodeKinds.find((entry) => entry.kind === node.kind);
  const Custom = formFor(node.kind);
  const [id, setId] = useState(node.id);
  const [params, setParams] = useState<Record<string, unknown>>(
    () => (node.params ?? {}) as Record<string, unknown>,
  );
  const working: WorkflowNode = { ...node, params };
  const dirty =
    id.trim() !== node.id || JSON.stringify(params) !== JSON.stringify(node.params ?? {});
  return (
    <InspectorPane
      title={node.id}
      description={kind?.description}
      dirty={dirty}
      canSave={idProblem(id, node, graph) === undefined}
      onSave={() => onSave(id.trim(), params)}
      onClose={onClose}
      actions={
        <DeleteButton onClick={onDelete} data-testid="delete-node">
          Delete node
        </DeleteButton>
      }
    >
      <Field label="Id" hint="The name the file writes the node by, and the edges to it.">
        <Input aria-label="Id" mono value={id} onChange={(event) => setId(event.target.value)} />
        {idProblem(id, node, graph) ? (
          <p className="mt-1 m-0 text-xs text-error-text" data-testid="id-problem">
            {idProblem(id, node, graph)}
          </p>
        ) : null}
      </Field>
      {Custom ? (
        <Custom node={working} graph={graph} onParams={setParams} />
      ) : (
        <ParamForm
          node={working}
          graph={graph}
          fields={kind?.params ?? []}
          known={kind !== undefined}
          onParams={setParams}
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
    </InspectorPane>
  );
}

/** Why the id typed cannot be the node's, or nothing when the graph can take it. */
function idProblem(
  draft: string,
  node: WorkflowNode,
  graph: WorkflowDefinition,
): string | undefined {
  const trimmed = draft.trim();
  if (trimmed === "") return "A node needs an id.";
  if (trimmed === TRIGGER) return "The trigger's id is reserved.";
  if (trimmed !== node.id && graph.nodes.some((other) => other.id === trimmed)) {
    return "Another node already has this id.";
  }
  return undefined;
}

function EdgeWindow({
  edge,
  onDelete,
  onClose,
}: {
  edge: WorkflowEdge;
  onDelete: () => void;
  onClose: () => void;
}) {
  return (
    <InspectorPane
      title={`${edge.from} → ${edge.to}`}
      dirty={false}
      onClose={onClose}
      actions={
        <DeleteButton onClick={onDelete} data-testid="delete-edge">
          Delete edge
        </DeleteButton>
      }
    >
      <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm">
        <dt className="text-secondary">Leaves</dt>
        <dd className="m-0 font-mono text-text">{edge.from}</dd>
        <dt className="text-secondary">By port</dt>
        <dd className="m-0 font-mono text-text">{edge.port}</dd>
        <dt className="text-secondary">Comes to</dt>
        <dd className="m-0 font-mono text-text">{edge.to}</dd>
      </dl>
    </InspectorPane>
  );
}

function TriggerWindow({
  trigger,
  project,
  onSave,
  onClose,
}: {
  trigger: Trigger | undefined;
  project?: number;
  onSave: (trigger: Trigger) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(trigger);
  const Custom = draft ? triggerFormFor(draft.kind) : undefined;
  return (
    <InspectorPane
      title="Trigger"
      description={trigger ? trigger.kind : "This workflow has none"}
      dirty={JSON.stringify(draft) !== JSON.stringify(trigger)}
      canSave={triggerProblem(draft) === null}
      onSave={() => draft && onSave(draft)}
      onClose={onClose}
    >
      {draft ? (
        Custom ? (
          <Custom trigger={draft} onTrigger={setDraft} project={project} />
        ) : null
      ) : (
        <p className="m-0 text-sm text-secondary">
          A workflow needs exactly one trigger, and this file has none.
        </p>
      )}
    </InspectorPane>
  );
}
