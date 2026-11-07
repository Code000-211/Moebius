/**
 * The read-only graph of a workflow: each node where the definition saved it, with its kind and the parameters it is
 * given, and a line from every node to the node that waits for it. It draws from the saved positions and edges and
 * never moves a node, so opening a definition shows the canvas a person saved rather than a new arrangement of it.
 *
 * The lines are behind the cards, so a card is read on its own even where lines cross under it. The graph scrolls in
 * both directions when the saved positions reach past the page, rather than shrinking every card until the words are
 * unreadable.
 */
import type { WorkflowDefinition, WorkflowNode } from "../../api/workflows";
import { JsonValue } from "../../ui/page/JsonValue";

/** The width one node card takes, so a line can end at the middle of it. */
const CARD = 240;
/** How far below a node's top its middle sits, near enough for a line to meet it under the card. */
const MIDDLE = 64;
const GAP = 48;

export function WorkflowGraph({ definition }: { definition: WorkflowDefinition }) {
  const nodes = definition.nodes;
  const width = Math.max(320, ...nodes.map((node) => node.position.x + CARD), CARD + GAP * 2);
  const height = Math.max(240, ...nodes.map((node) => node.position.y + 180), 180 + GAP * 2);
  const byId = new Map(nodes.map((node) => [node.id, node]));
  return (
    <section
      aria-label="Definition graph"
      className="min-h-0 flex-1 overflow-auto rounded-card bg-lane p-4"
    >
      <div className="relative" style={{ width: width + GAP, height: height + GAP }}>
        <svg
          aria-hidden="true"
          className="pointer-events-none absolute inset-0"
          width={width + GAP}
          height={height + GAP}
        >
          <defs>
            <marker
              id="workflow-arrow"
              viewBox="0 0 10 10"
              refX="9"
              refY="5"
              markerWidth="6"
              markerHeight="6"
              orient="auto-start-reverse"
            >
              <path d="M 0 0 L 10 5 L 0 10 z" className="fill-hairline" />
            </marker>
          </defs>
          {definition.edges.map((edge) => {
            const from = byId.get(edge.from);
            const to = byId.get(edge.to);
            if (!from || !to) return null;
            const x1 = from.position.x + CARD / 2;
            const y1 = from.position.y + MIDDLE;
            const x2 = to.position.x + CARD / 2;
            const y2 = to.position.y + MIDDLE;
            const middle = (y1 + y2) / 2;
            return (
              <path
                key={`${edge.from}->${edge.to}`}
                d={`M ${x1} ${y1} C ${x1} ${middle}, ${x2} ${middle}, ${x2} ${y2}`}
                className="fill-none stroke-hairline"
                strokeWidth="1.5"
                markerEnd="url(#workflow-arrow)"
              />
            );
          })}
        </svg>
        {nodes.map((node) => (
          <NodeCard key={node.id} node={node} />
        ))}
        {nodes.length === 0 ? (
          <p className="m-0 max-w-prose text-sm text-secondary">This graph has no nodes.</p>
        ) : null}
      </div>
    </section>
  );
}

function NodeCard({ node }: { node: WorkflowNode }) {
  return (
    <article
      className="absolute flex flex-col gap-2 rounded-card bg-surface p-3 shadow-card"
      style={{ left: node.position.x, top: node.position.y, width: CARD }}
    >
      <div className="flex min-w-0 flex-col">
        <h3 className="m-0 truncate text-sm font-semibold text-text">{node.id}</h3>
        <span className="truncate text-xs text-secondary">{node.kind}</span>
      </div>
      <JsonValue value={node.parameters} name={`${node.id}-parameters`} />
    </article>
  );
}
