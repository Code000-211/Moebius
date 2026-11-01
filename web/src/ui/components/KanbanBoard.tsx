// From BloopAI/vibe-kanban packages/ui/src/components/KanbanBoard.tsx (Apache-2.0), changed: no i18n and no "add task" button (a card comes from an issue); a column is a lane box with a border colour that says what its cards do; the header takes a CSS colour and a count and shows the column's script; the drag context is a flex row of columns, not a divided grid; the Feature and Status types of the Gantt chart are gone; Card is a plain div (React 19 ref prop); the card takes a selected/live look from Deep Velvet and is dragged by its whole body, with the keyboard too (the original had a grip for phones, which the app does not need); dragging uses dnd-kit (a pointer sensor, a keyboard sensor that moves to the next column even when it is scrolled out of view, a drag overlay and spoken announcements) instead of @hello-pangea/dnd, and a column as a whole takes a drop.
import {
  type Announcements,
  type CollisionDetection,
  DndContext,
  DragOverlay,
  type KeyboardCoordinateGetter,
  KeyboardSensor,
  PointerSensor,
  pointerWithin,
  rectIntersection,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  type Ref,
  type RefObject,
  useCallback,
  useRef,
  useState,
} from "react";
import { cn } from "../lib/cn";

/** What a drag ends with: the card (its id) and the column it was let go over, none when it was let go elsewhere or cancelled. */
export type DropResult = { cardId: string; column: string | null };

// =============================================================================
// Kanban Board (a column)
// =============================================================================

export type KanbanBoardProps = {
  children: ReactNode;
  /** The name of the column, for screen readers. */
  name: string;
  className?: string;
};

export const KanbanBoard = ({ children, name, className }: KanbanBoardProps) => {
  // The whole column takes a drop; its name is the id of the drop.
  const { setNodeRef, isOver } = useDroppable({ id: name });
  return (
    <section
      ref={setNodeRef}
      aria-label={name}
      className={cn(
        "flex min-h-40 min-w-[220px] flex-1 flex-col rounded-xl border bg-lane p-[10px]",
        isOver && "ring-2 ring-accent ring-inset",
        className,
      )}
    >
      {children}
    </section>
  );
};

// =============================================================================
// Kanban Card (Draggable)
// =============================================================================
// Kanban Card (Draggable)
// =============================================================================

export type KanbanCardFaceProps = {
  name: string;
  children?: ReactNode;
  className?: string;
  isSelected?: boolean;
  isOpen?: boolean;
  /** The card is being worked on: its border is the live colour. */
  isLive?: boolean;
};

const faceClass = ({ isLive, isSelected, isOpen, className }: KanbanCardFaceProps) =>
  cn(
    "mb-[8px] rounded-lg border bg-card p-[10px] outline-none",
    "focus-visible:ring-2 focus-visible:ring-accent",
    isLive && "border-live-border",
    isSelected
      ? "border-accent ring-1 ring-accent ring-inset"
      : isOpen && "ring-2 ring-secondary-foreground ring-inset",
    className,
  );

/** The look of a card without its dragging: what the drag overlay shows under the pointer. */
export const KanbanCardFace = (props: KanbanCardFaceProps) => (
  <div aria-hidden className={faceClass({ ...props, className: cn("shadow-lg", props.className) })}>
    {props.children ?? <p className="m-0 font-medium text-sm">{props.name}</p>}
  </div>
);

export type KanbanCardProps = KanbanCardFaceProps & {
  id: string;
  /** The name of the column the card is in. */
  column: string;
  onClick?: (e: MouseEvent<HTMLDivElement>) => void;
  tabIndex?: number;
  forwardedRef?: Ref<HTMLDivElement>;
  onKeyDown?: (e: KeyboardEvent) => void;
  dragDisabled?: boolean;
  testId?: string;
};

export const KanbanCard = ({
  id,
  column,
  name,
  children,
  className,
  onClick,
  tabIndex,
  forwardedRef,
  onKeyDown,
  isOpen,
  isSelected,
  dragDisabled = false,
  isLive,
  testId,
}: KanbanCardProps) => {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id,
    disabled: dragDisabled,
    data: { name, column },
  });
  // Combine the drag ref and the forwarded ref
  const setRefs = (node: HTMLDivElement | null) => {
    setNodeRef(node);
    if (typeof forwardedRef === "function") {
      forwardedRef(node);
    } else if (forwardedRef) {
      (forwardedRef as RefObject<HTMLDivElement | null>).current = node;
    }
  };

  return (
    // biome-ignore lint/a11y/useSemanticElements: the drag library needs this element as its handle (it supplies the role and the description through attributes); a card holds links, which a <button> cannot
    <div
      {...attributes}
      {...listeners}
      role="button"
      tabIndex={tabIndex ?? 0}
      data-testid={testId}
      aria-label={name}
      className={faceClass({
        name,
        isLive,
        isSelected,
        isOpen,
        className: cn(isDragging && "opacity-40", className),
      })}
      ref={setRefs}
      onClick={onClick}
      onKeyDown={(e) => {
        listeners?.onKeyDown?.(e);
        onKeyDown?.(e);
      }}
    >
      {children ?? <p className="m-0 font-medium text-sm">{name}</p>}
    </div>
  );
};

// =============================================================================

export type KanbanCardsProps = {
  children: ReactNode;
  className?: string;
};

export const KanbanCards = ({ children, className }: KanbanCardsProps) => (
  <div className={cn("flex flex-1 flex-col", className)}>{children}</div>
);

// =============================================================================
// Kanban Header
// =============================================================================

export type KanbanHeaderProps = {
  name: string;
  /** Any CSS colour, such as `var(--live)`: the colour of the name. */
  color: string;
  count: number;
  /** The line under the name, such as the column's script. */
  detail?: string;
  className?: string;
};

export const KanbanHeader = ({ name, color, count, detail, className }: KanbanHeaderProps) => (
  <div className={cn("shrink-0 border-b px-half pb-base pt-[6px] mb-base", className)}>
    <div className="flex items-baseline justify-between gap-base">
      <h2 className="m-0 text-[14px] font-bold" style={{ color }}>
        {name}
      </h2>
      <span className="font-mono text-[11px] text-low" title={`${count} cards`}>
        {count}
      </span>
    </div>
    {detail && <div className="mt-[2px] font-mono text-[11px] text-low">{detail}</div>}
  </div>
);

// =============================================================================
// Kanban Provider (DndContext)
// =============================================================================

export type KanbanProviderProps = {
  children: ReactNode;
  onDragEnd: (result: DropResult) => void;
  /** What the drag overlay shows for the card with this id. */
  renderOverlay: (cardId: string) => ReactNode;
  className?: string;
};

/** The id of the drag overlay's element, which the keyboard move measures. */
const OVERLAY_ATTRIBUTE = "data-kanban-overlay";

/**
 * A pointer drop is on the column under the pointer. A keyboard drop is on the column the arrow keys chose (`target`),
 * because the keyboard has no pointer and the chosen column may be scrolled out of view.
 */
function collisionsFor(target: RefObject<string | null>): CollisionDetection {
  return (args) => {
    if (args.pointerCoordinates) {
      const within = pointerWithin(args);
      return within.length > 0 ? within : rectIntersection(args);
    }
    return args.droppableContainers
      .filter((container) => container.id === target.current)
      .map((container) => ({ id: container.id, data: { droppableContainer: container } }));
  };
}

export const KanbanProvider = ({
  children,
  onDragEnd,
  renderOverlay,
  className,
}: KanbanProviderProps) => {
  const [activeId, setActiveId] = useState<string | null>(null);
  // The column the keyboard drag is over, among the columns in the order of the page.
  const target = useRef<string | null>(null);
  const byKeyboard = useRef(false);
  const collisionDetection = useRef(collisionsFor(target)).current;

  // Left and right move to the neighbouring column, scrolled into view; up and down stay (the order within a column is the server's).
  const coordinates: KeyboardCoordinateGetter = useCallback(
    (event, { context, currentCoordinates }) => {
      if (event.code !== "ArrowRight" && event.code !== "ArrowLeft") return undefined;
      event.preventDefault();
      const columns = [...context.droppableContainers.getEnabled()]
        .filter((container) => container.node.current)
        .sort(
          (a, b) =>
            (a.node.current?.getBoundingClientRect().left ?? 0) -
            (b.node.current?.getBoundingClientRect().left ?? 0),
        );
      const at = columns.findIndex((container) => container.id === target.current);
      const next =
        columns[
          Math.max(0, Math.min(columns.length - 1, at + (event.code === "ArrowRight" ? 1 : -1)))
        ];
      const node = next?.node.current;
      if (!next || !node) return undefined;
      target.current = String(next.id);
      node.scrollIntoView({ behavior: "instant", inline: "center", block: "nearest" });
      // The overlay is fixed on the screen, so it is moved by what lies between its middle and the column's.
      const overlay = document.querySelector(`[${OVERLAY_ATTRIBUTE}]`)?.getBoundingClientRect();
      const column = node.getBoundingClientRect();
      if (!overlay) return currentCoordinates;
      return {
        x:
          currentCoordinates.x +
          column.left +
          column.width / 2 -
          (overlay.left + overlay.width / 2),
        y: currentCoordinates.y,
      };
    },
    [],
  );
  const sensors = useSensors(
    // A small distance, so that a click still opens the dock.
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: coordinates,
      keyboardCodes: { start: ["Space"], cancel: ["Escape"], end: ["Space", "Enter"] },
    }),
  );

  const nameOf = (item: { data: { current?: { name?: string } | null } }) =>
    item.data.current?.name ?? "the card";
  const announcements: Announcements = {
    onDragStart: ({ active }) =>
      `Picked up ${nameOf(active)}. Press the left or right arrow to choose a column, space or enter to drop it, escape to cancel.`,
    onDragOver: ({ active, over }) =>
      over
        ? `${nameOf(active)} is over the column ${over.id}.`
        : `${nameOf(active)} is not over a column.`,
    onDragEnd: ({ active, over }) =>
      over
        ? `Dropped ${nameOf(active)} on the column ${over.id}.`
        : `Dropped ${nameOf(active)} outside the columns. Nothing changed.`,
    onDragCancel: ({ active }) => `Cancelled. ${nameOf(active)} stays where it was.`,
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={collisionDetection}
      accessibility={{
        announcements,
        screenReaderInstructions: {
          draggable:
            "To pick up a card, press space. Then use the left and right arrows to move it to another column, space or enter to drop it, escape to cancel.",
        },
      }}
      onDragStart={({ active, activatorEvent }) => {
        byKeyboard.current = activatorEvent instanceof KeyboardEvent;
        setActiveId(String(active.id));
        // The keyboard starts in the column the card is in.
        target.current = active.data.current?.column ?? null;
      }}
      onDragEnd={({ active, over }) => {
        setActiveId(null);
        // A key pressed just before the drop may not have reached `over` yet: the keyboard's own choice is the drop.
        const column = byKeyboard.current ? target.current : over ? String(over.id) : null;
        onDragEnd({ cardId: String(active.id), column });
      }}
      onDragCancel={() => setActiveId(null)}
    >
      <div className={cn("flex items-stretch gap-[10px] min-h-full", className)}>{children}</div>
      <DragOverlay dropAnimation={null}>
        {activeId ? <div {...{ [OVERLAY_ATTRIBUTE]: "" }}>{renderOverlay(activeId)}</div> : null}
      </DragOverlay>
    </DndContext>
  );
};
