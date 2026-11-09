/**
 * The canvas: a workflow's graph built by hand and saved as a commit.
 *
 * The graph a person is editing is one piece of state — the workflow's parsed form, as `GET /v1/workflows/{name}`
 * answers it and as `PUT /v1/workflows/{name}` takes it back — and the cards and edges on the screen are drawn from
 * it, so what is saved is what is on the screen, positions included. The kinds on the palette come from the web
 * app's registry (`../nodeKinds`); a node's parameters are edited by the generic form, or by the form its kind draws
 * itself when it has one.
 *
 * A save the server refuses is not thrown away: its message names a node, an edge or the trigger, and is shown there
 * until the next try. A run can be picked and is drawn over the graph, each node it reached with its status and what
 * it was given and gave back. Leaving with unsaved changes asks first, whether by an app route or by the tab.
 */
import { ArrowLeftIcon, FloppyDiskIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Background,
  BackgroundVariant,
  Controls,
  type Edge,
  type EdgeChange,
  type NodeChange,
  ReactFlow,
  ReactFlowProvider,
  useEdgesState,
  useNodesState,
  useReactFlow,
} from "@xyflow/react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useBlocker, useParams } from "react-router";
import { saveWorkflow, workflow, workflowRuns } from "../../../api/calls/workflows";
import { ApiError } from "../../../api/http";
import { keys } from "../../../api/keys";
import type { WorkflowDefinition, WorkflowEdge } from "../../../api/types/workflows";
import { type WorkflowNodeRun, workflowRun } from "../../../api/workflows";
import { Button } from "../../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../../ui/components/Dialog";
import { cn } from "../../../ui/lib/cn";
import { Select } from "../../../ui/page/Field";
import { Notice } from "../../../ui/page/Notice";
import { PageHeader } from "../../../ui/page/PageHeader";
import { ReadFailed } from "../../../ui/page/ReadFailed";
import { SkeletonLines } from "../../../ui/page/Skeleton";
import { triggerProblem } from "../forms/trigger";
import { genericKind, type NodeKind, nodeKinds, startingParams } from "../nodeKinds";
import { NodeCard, TriggerCard } from "./cards";
import { type CardNode, flowEdges, flowNodes, type Marks, problemsOf } from "./flow";
import { Inspector, type Selection } from "./Inspector";
import {
  addNode,
  connect,
  edgeId,
  isDirty,
  moveNode,
  nextPosition,
  type Problem,
  problemFor,
  removeEdge,
  removeNode,
  renameNode,
  setParams,
  setTrigger,
  TRIGGER,
} from "./model";
import { droppedKind, Palette } from "./Palette";

const nodeTypes = { kind: NodeCard, trigger: TriggerCard };

export function WorkflowCanvasPage() {
  const { name = "" } = useParams();
  const definition = useQuery({
    queryKey: keys.workflow(name),
    queryFn: () => workflow(name),
  });
  const back = (
    <Button asChild variant="ghost" size="sm">
      <Link to={`/workflows/${encodeURIComponent(name)}`}>
        <ArrowLeftIcon className="size-4" aria-hidden="true" />
        {name}
      </Link>
    </Button>
  );

  if (definition.isPending) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <PageHeader title={`${name} — canvas`} back={back} />
        <SkeletonLines lines={6} message="Reading the definition" />
      </div>
    );
  }
  if (definition.isError) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <PageHeader title={`${name} — canvas`} back={back} />
        <ReadFailed
          error={definition.error}
          fallback="Could not read this workflow's definition. Try again."
          busy={definition.isFetching}
          onRetry={() => void definition.refetch()}
        />
      </div>
    );
  }
  return (
    <ReactFlowProvider>
      <Canvas
        key={definition.data.name}
        name={definition.data.name}
        loaded={definition.data.definition}
      />
    </ReactFlowProvider>
  );
}

/** The editor for one workflow, starting from the graph as it is saved. */
function Canvas({ name, loaded }: { name: string; loaded: WorkflowDefinition }) {
  const queryClient = useQueryClient();
  const board = useReactFlow();
  const [draft, setDraft] = useState<WorkflowDefinition>(loaded);
  const [saved, setSaved] = useState<WorkflowDefinition>(loaded);
  const [selection, setSelection] = useState<Selection>(null);
  const [runId, setRunId] = useState<number | null>(null);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [commit, setCommit] = useState<string | null>(null);
  const [movedTrigger, setMovedTrigger] = useState<{ x: number; y: number } | null>(null);

  const dirty = isDirty(saved, draft);
  const triggerIssue = triggerProblem(draft.trigger[0]);
  const runs = useQuery({ queryKey: keys.workflowRuns(name), queryFn: () => workflowRuns(name) });
  const run = useQuery({
    queryKey: keys.workflowRun(runId ?? 0),
    queryFn: () => workflowRun(runId ?? 0),
    enabled: runId !== null,
  });

  const save = useMutation({
    mutationFn: () => saveWorkflow(name, draft),
    onSuccess: (written) => {
      setSaved(written.definition);
      setDraft(written.definition);
      setProblem(null);
      setCommit(written.commit ?? null);
      // The saved workflow is read again rather than written into the cache: the page beside this one reads that
      // cache too, in the shape its own route answers, and a part of it would leave that page short of a field.
      void queryClient.invalidateQueries({ queryKey: keys.workflow(name) });
      void queryClient.invalidateQueries({ queryKey: keys.workflows });
    },
    onError: (error) => setProblem(problemFor(messageOf(error))),
  });

  const marks: Marks = useMemo(
    () => ({
      node: selection?.kind === "node" ? selection.id : null,
      edge: selection?.kind === "edge" ? edgeId(selection.edge) : null,
      problems: problemsOf(problem),
      triggerProblem: problem?.on && "trigger" in problem.on ? problem.text : undefined,
      runs: runsOf(run.data?.nodes),
      triggerPosition: movedTrigger ?? undefined,
    }),
    [selection, problem, run.data, movedTrigger],
  );

  const flow = useMemo(
    () => ({ nodes: flowNodes(draft, marks), edges: flowEdges(draft, marks) }),
    [draft, marks],
  );
  const [cards, setCards, onCardChanges] = useNodesState<CardNode>(flow.nodes);
  const [wires, setWires, onWireChanges] = useEdgesState<Edge>(flow.edges);
  // The graph is what is drawn: whenever it changes, so does the board. Two things a redraw leaves alone: a card the
  // pointer is holding stays where the pointer put it until the drag is let go, and what the library measured about a
  // card is kept, since a card measured again loses the wires that hang off it until it is measured back.
  useEffect(() => {
    setCards((current) =>
      flow.nodes.map((node) => {
        const drawn = current.find((entry) => entry.id === node.id);
        return drawn
          ? {
              ...node,
              measured: drawn.measured,
              dragging: drawn.dragging,
              position: drawn.dragging ? drawn.position : node.position,
            }
          : node;
      }),
    );
    setWires(flow.edges);
  }, [flow, setCards, setWires]);

  const add = useCallback(
    (kind: NodeKind, params: Record<string, unknown>, at?: { x: number; y: number }) => {
      const withNode = addNode(draft, kind.kind, at ?? nextPosition(draft), params);
      const added = withNode.nodes[withNode.nodes.length - 1];
      setDraft(withNode);
      if (added) {
        setSelection({ kind: "node", id: added.id });
      }
    },
    [draft],
  );

  const onNodesChange = useCallback(
    (changes: NodeChange<CardNode>[]) => {
      onCardChanges(changes);
      for (const change of changes) {
        if (change.type === "remove" && change.id !== TRIGGER) {
          setDraft((graph) => removeNode(graph, change.id));
          setSelection(null);
        }
      }
    },
    [onCardChanges],
  );

  const onEdgesChange = useCallback(
    (changes: EdgeChange[]) => {
      onWireChanges(changes);
      for (const change of changes) {
        if (change.type !== "remove") {
          continue;
        }
        const gone = draft.edges.find((edge) => edgeId(edge) === change.id);
        if (gone) {
          setDraft((graph) => removeEdge(graph, gone));
          setSelection(null);
        }
      }
    },
    [onWireChanges, draft.edges],
  );

  function drop(event: React.DragEvent) {
    event.preventDefault();
    const kind = droppedKind(event);
    if (!kind) {
      return;
    }
    const entry = paletteKindOf(kind);
    add(
      entry,
      startingParams(entry),
      board.screenToFlowPosition({ x: event.clientX, y: event.clientY }),
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="canvas-page">
      <PageHeader
        title={name}
        back={
          <Button asChild variant="ghost" size="sm">
            <Link to={`/workflows/${encodeURIComponent(name)}`}>
              <ArrowLeftIcon className="size-4" aria-hidden="true" />
              {name}
            </Link>
          </Button>
        }
        context={draft.description === "" ? undefined : draft.description}
        actions={
          <div className="flex items-center gap-2">
            <Select
              aria-label="Run to show"
              value={runId === null ? "" : String(runId)}
              onChange={(value) => setRunId(value === "" ? null : Number(value))}
              options={[
                ["", "No run picked"],
                ...(runs.data ?? []).map((entry): [string, string] => [
                  String(entry.id),
                  `Run #${entry.id} · ${entry.status}`,
                ]),
              ]}
            />
            <span
              className={cn("text-sm", dirty ? "text-attention-text" : "text-secondary")}
              data-testid="canvas-state"
            >
              {dirty ? "Unsaved changes" : commit ? `Saved as ${commit.slice(0, 7)}` : "Saved"}
            </span>
            <Button
              variant="primary"
              size="sm"
              disabled={!dirty || save.isPending || triggerIssue !== null}
              title={triggerIssue ?? undefined}
              onClick={() => save.mutate()}
              data-testid="canvas-save"
            >
              <FloppyDiskIcon className="size-4" aria-hidden="true" />
              {save.isPending ? "Saving" : "Save"}
            </Button>
          </div>
        }
      />
      <div className="flex min-h-0 flex-1">
        <Palette graph={draft} onAdd={add} />
        <div className="relative flex min-w-0 flex-1 flex-col" data-testid="canvas-board">
          <ReactFlow
            nodes={cards}
            edges={wires}
            nodeTypes={nodeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onNodeDragStop={(_event, node) => {
              if (node.id === TRIGGER) {
                setMovedTrigger(node.position);
              } else {
                setDraft((graph) => moveNode(graph, node.id, node.position));
              }
            }}
            onConnect={(connection) => {
              const from = connection.source;
              const to = connection.target;
              if (!from || !to) {
                return;
              }
              const port = connection.sourceHandle ?? "out";
              setDraft((graph) => connect(graph, { from, to, port }) ?? graph);
            }}
            isValidConnection={(connection) =>
              Boolean(connection.source) &&
              connection.source !== connection.target &&
              !draft.edges.some(
                (edge) =>
                  edge.from === connection.source &&
                  edge.to === connection.target &&
                  edge.port === connection.sourceHandle,
              )
            }
            onNodeClick={(_event, node) =>
              setSelection(
                node.id === TRIGGER ? { kind: "trigger" } : { kind: "node", id: node.id },
              )
            }
            onEdgeClick={(_event, wire) => {
              const edge = edgeOf(wire);
              if (edge) {
                setSelection({ kind: "edge", edge });
              }
            }}
            onPaneClick={() => setSelection(null)}
            onInit={(instance) => {
              void instance.fitView({ padding: 0.2 });
            }}
            onDrop={drop}
            onDragOver={(event) => {
              event.preventDefault();
              event.dataTransfer.dropEffect = "copy";
            }}
            minZoom={0.2}
            maxZoom={2}
            deleteKeyCode={["Backspace", "Delete"]}
            proOptions={{ hideAttribution: true }}
          >
            <Background
              variant={BackgroundVariant.Dots}
              gap={20}
              size={1}
              color="var(--hairline)"
            />
            <Controls showInteractive={false} position="bottom-left" />
          </ReactFlow>
          {save.isError ? (
            <div
              className="absolute top-3 right-3 left-3 z-10 mx-auto max-w-2xl"
              data-testid="canvas-problem"
            >
              <Notice tone="error">
                {problem ? problem.text : "The save did not go through. Try again."}
              </Notice>
            </div>
          ) : null}
        </div>
        <Inspector
          graph={draft}
          selection={selection}
          run={
            selection?.kind === "node"
              ? (run.data?.nodes.find((entry) => entry.node === selection.id) ?? null)
              : null
          }
          onRename={(from, to) => {
            const renamed = renameNode(draft, from, to);
            if (!renamed) {
              return;
            }
            setDraft(renamed);
            // The panel follows the node it is showing, so its parameters and run stay beside the new name.
            setSelection({ kind: "node", id: to.trim() });
          }}
          onParams={(id, params) => setDraft((graph) => setParams(graph, id, params))}
          onTrigger={(trigger) => setDraft((graph) => setTrigger(graph, trigger))}
          onDeleteNode={(id) => {
            setDraft((graph) => removeNode(graph, id));
            setSelection(null);
          }}
          onDeleteEdge={(edge) => {
            setDraft((graph) => removeEdge(graph, edge));
            setSelection(null);
          }}
        />
      </div>
      <UnsavedChanges
        open={dirty && !save.isPending}
        onSave={async () => {
          try {
            await save.mutateAsync();
            return true;
          } catch {
            return false;
          }
        }}
      />
    </div>
  );
}

/**
 * Asks before a person leaves a graph with unsaved changes, whether by an app route or by closing the tab. The tab
 * itself shows the browser's own message; nothing of ours can be put in front of it. Saving on the way out stays on
 * this page when the server refuses the graph, so the refusal can be read where the node it names is.
 */
function UnsavedChanges({
  open,
  onSave,
}: {
  open: boolean;
  /** Saves the graph, and says whether it was saved. */
  onSave: () => Promise<boolean>;
}) {
  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      open && currentLocation.pathname !== nextLocation.pathname,
  );
  const [saving, setSaving] = useState(false);

  async function saveAndLeave() {
    setSaving(true);
    const saved = await onSave();
    setSaving(false);
    if (saved) {
      blocker.proceed?.();
    }
  }

  useEffect(() => {
    if (!open) {
      return;
    }
    function ask(event: BeforeUnloadEvent) {
      event.preventDefault();
      event.returnValue = true;
    }
    window.addEventListener("beforeunload", ask);
    return () => window.removeEventListener("beforeunload", ask);
  }, [open]);

  return (
    <Dialog open={blocker.state === "blocked"} onOpenChange={(next) => !next && blocker.reset?.()}>
      <DialogContent className="gap-4 p-6">
        <DialogHeader>
          <DialogTitle>Leave with unsaved changes?</DialogTitle>
        </DialogHeader>
        <p className="m-0 text-sm text-secondary">
          This graph has changes that are not in the file yet. Leaving now loses them.
        </p>
        <DialogFooter>
          <Button variant="secondary" onClick={() => blocker.reset?.()}>
            Stay
          </Button>
          <Button variant="secondary" disabled={saving} onClick={() => void saveAndLeave()}>
            {saving ? "Saving" : "Save and leave"}
          </Button>
          <Button variant="danger" onClick={() => blocker.proceed?.()}>
            Leave without saving
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** The nodes of a run, by the node they belong to. */
function runsOf(nodes: WorkflowNodeRun[] | undefined): Record<string, WorkflowNodeRun> | undefined {
  if (!nodes) {
    return undefined;
  }
  return Object.fromEntries(nodes.map((entry) => [entry.node, entry]));
}

/** A wire of the board as the definition writes it. */
function edgeOf(wire: Edge): WorkflowEdge | null {
  return wire.source && wire.target
    ? { from: wire.source, to: wire.target, port: wire.sourceHandle ?? "out" }
    : null;
}

/** The palette entry a dropped kind adds, whether the registry knows it or not. */
function paletteKindOf(kind: string): NodeKind {
  return nodeKinds.find((entry) => entry.kind === kind) ?? genericKind(kind);
}

function messageOf(error: unknown): string {
  return error instanceof ApiError ? error.message : "The save did not go through.";
}
