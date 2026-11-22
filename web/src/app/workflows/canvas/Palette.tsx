/** The kinds a graph is built from, where a person takes them from: a slim list, one row per kind. */
import { useState } from "react";
import type { WorkflowDefinition } from "../../../api/types/workflows";
import { cn } from "../../../ui/lib/cn";
import { type NodeKind, paletteKinds, startingParams } from "../nodeKinds";
import { KindIcon } from "./kindIcons";

/** What a drop onto the graph carries: the kind dropped, and where the pointer was released. */
export const DROP_MIME = "application/x-moebius-node";

export function Palette({
  graph,
  onAdd,
}: {
  /** The graph the kinds are offered for, so a kind already on it is not hidden by a name the registry lacks. */
  graph: WorkflowDefinition;
  /** Adds a node of a kind. With no place, after the chosen step, or where there is room. */
  onAdd: (kind: NodeKind, params: Record<string, unknown>, at?: { x: number; y: number }) => void;
}) {
  const [dragged, setDragged] = useState<string | null>(null);
  const kinds = paletteKinds(graph);
  return (
    <nav
      aria-label="Add a step"
      className="flex w-[170px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-hairline bg-panel px-2 py-3"
    >
      <p className="m-0 mb-1 px-2 text-sm font-semibold text-text">Add a step</p>
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
            "flex h-8 cursor-grab items-center gap-2 rounded-control px-2 text-left text-sm text-text",
            "hover:bg-hover",
            dragged === kind.kind && "bg-hover",
          )}
        >
          <KindIcon kind={kind.kind} className="size-4 shrink-0 text-secondary" />
          <span className="min-w-0 flex-1 truncate">{kind.title}</span>
        </button>
      ))}
      <p className="m-0 mt-2 px-2 text-xs text-secondary">Drag onto the canvas</p>
    </nav>
  );
}

/** The kind a drop carries, when it carries one. */
export function droppedKind(event: React.DragEvent): string | null {
  return event.dataTransfer.getData(DROP_MIME) || event.dataTransfer.getData("text/plain") || null;
}
