/** The kinds a graph is built from, where a person takes them from. */
import { useState } from "react";
import type { WorkflowDefinition } from "../../../api/types/workflows";
import { cn } from "../../../ui/lib/cn";
import { type NodeKind, paletteKinds, startingParams } from "../nodeKinds";

/** What a drop onto the graph carries: the kind dropped, and where the pointer was released. */
export const DROP_MIME = "application/x-moebius-node";

export function Palette({
  graph,
  onAdd,
}: {
  /** The graph the kinds are offered for, so a kind already on it is not hidden by a name the registry lacks. */
  graph: WorkflowDefinition;
  /** Adds a node of a kind. With no place, where there is room. */
  onAdd: (kind: NodeKind, params: Record<string, unknown>, at?: { x: number; y: number }) => void;
}) {
  const [dragged, setDragged] = useState<string | null>(null);
  const kinds = paletteKinds(graph);
  return (
    <div className="flex w-52 shrink-0 flex-col gap-2 overflow-y-auto border-r border-hairline bg-surface p-3">
      <p className="m-0 text-xs font-semibold uppercase tracking-wide text-secondary">Node kinds</p>
      {kinds.map((kind) => (
        <button
          key={kind.kind}
          type="button"
          data-testid="palette-kind"
          data-kind={kind.kind}
          draggable
          onDragStart={(event) => {
            setDragged(kind.kind);
            event.dataTransfer.effectAllowed = "copy";
            event.dataTransfer.setData(DROP_MIME, kind.kind);
            event.dataTransfer.setData("text/plain", kind.kind);
          }}
          onDragEnd={() => setDragged(null)}
          onClick={() => onAdd(kind, startingParams(kind))}
          title={kind.description}
          className={cn(
            "flex cursor-grab flex-col items-start gap-1 rounded-control border border-hairline bg-sunken px-3 py-2 text-left",
            "hover:border-hairline-strong",
            dragged === kind.kind && "border-hairline-strong bg-hover",
          )}
        >
          <span className="text-sm font-medium text-text">{kind.title}</span>
          <span className="font-mono text-xs text-secondary">{kind.kind}</span>
        </button>
      ))}
      <p className="mt-1 text-xs leading-snug text-secondary">
        Drag a kind onto the graph, or press one to add it where there is room.
      </p>
    </div>
  );
}

/** The kind a drop carries, when it carries one. */
export function droppedKind(event: React.DragEvent): string | null {
  return event.dataTransfer.getData(DROP_MIME) || event.dataTransfer.getData("text/plain") || null;
}
