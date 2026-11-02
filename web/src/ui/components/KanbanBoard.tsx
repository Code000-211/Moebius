// From BloopAI/vibe-kanban packages/ui/src/components/KanbanBoard.tsx (Apache-2.0), changed: no i18n and no "add task" button (a card comes from an issue); a column is a lane of the canvas with no border, and its header counts the cards in it; the drag context is a flex row of columns, not a divided grid; the Feature and Status types of the Gantt chart are gone; Card is a plain div (React 19 ref prop); a card is a borderless sheet whose state is a word, and it is dragged by its whole body, with the keyboard too (the original had a grip for phones, which the app does not need); the card is a group and not a named button, and its title is the button that opens it, so the issue link and the pull-request badge inside it stay in the accessibility tree; dragging uses dnd-kit (a pointer sensor, a keyboard sensor that moves to the next column even when it is scrolled out of view, a drag overlay and spoken announcements) instead of @hello-pangea/dnd, and a column as a whole takes a drop.
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
import { type ReactNode, type Ref, type RefObject, useCallback, useRef, useState } from "react";
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
        "flex min-h-40 min-w-72 flex-1 flex-col rounded-card bg-lane p-3",
        isOver && "ring-2 ring-text ring-inset",
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
  /** The card's run is working: a hairline of the colour of work in progress. */
  isLive?: boolean;
};

const faceClass = ({ isLive, isSelected, isOpen, className }: KanbanCardFaceProps) =>
  cn(
    "rounded-card bg-surface p-4 shadow-card",
    "transition-shadow duration-(--motion-routine) ease-out hover:shadow-raised",
    isLive && "ring-1 ring-progress-border ring-inset",
    isSelected ? "ring-2 ring-text ring-inset" : isOpen && "ring-2 ring-hairline-strong ring-inset",
    className,
  );

/** The look of a card without its dragging: what the drag overlay shows under the pointer. */
export const KanbanCardFace = (props: KanbanCardFaceProps) => (
  <div
    aria-hidden
    // A picture of a card, so nothing in it is a target: the links it shows belong to the card being dragged.
    inert
    className={faceClass({ ...props, className: cn("shadow-raised", props.className) })}
  >
    {props.children ?? <p className="m-0 text-base font-semibold">{props.name}</p>}
  </div>
);

export type KanbanCardProps = KanbanCardFaceProps & {
  id: string;
  /** The name of the column the card is in. */
  column: string;
  /**
   * Opens the card. Its title is the control that does this, so a keyboard and a screen reader have a real one; the
   * rest of the card answers a plain click too, which is what a pointer expects of a card.
   */
  onOpen?: () => void;
  tabIndex?: number;
  forwardedRef?: Ref<HTMLDivElement>;
  dragDisabled?: boolean;
  testId?: string;
};

export const KanbanCard = ({
  id,
  column,
  name,
  children,
  className,
  onOpen,
  tabIndex,
  forwardedRef,
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
    // The drag library needs this box as its handle and describes it through `attributes`. It is a group, not a
    // button: a card holds links, which a <button> may not, and naming a button makes what is inside it
    // presentational, so the state word and the issue link would leave the accessibility tree.
    // biome-ignore lint/a11y/useSemanticElements: a <fieldset> groups the controls of a form, which a card is not
    <div
      {...attributes}
      {...listeners}
      role="group"
      aria-roledescription="card"
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
      onClick={onOpen}
      onKeyDown={(e) => {
        listeners?.onKeyDown?.(e);
        // Enter opens the card where it lands, Space picks it up for a drag; Enter that ended a drag is taken.
        if (e.key === "Enter" && !e.defaultPrevented) onOpen?.();
      }}
    >
      {children ?? <p className="m-0 text-base font-semibold">{name}</p>}
    </div>
  );
};

// =============================================================================

export type KanbanCardsProps = {
  children: ReactNode;
  className?: string;
};

export const KanbanCards = ({ children, className }: KanbanCardsProps) => (
  <div className={cn("flex flex-1 flex-col gap-3", className)}>{children}</div>
);

// =============================================================================
// Kanban Header
// =============================================================================

export type KanbanHeaderProps = {
  name: string;
  /** How many cards the column shows. */
  count: number;
  /** How many cards the column holds, when it shows fewer than that. */
  total?: number;
  /**
   * The mark beside the column's name: the shape and colour of the state it holds. It never carries the meaning on its
   * own — the name is beside it — and says nothing to the person hearing the page, so it is hidden from them.
   */
  mark?: ReactNode;
  className?: string;
};

export const KanbanHeader = ({ name, count, total, mark, className }: KanbanHeaderProps) => (
  <header
    className={cn("mb-3 flex shrink-0 flex-wrap items-center justify-between gap-2", className)}
  >
    <h2 className="m-0 flex min-w-0 items-center gap-2 text-lg font-semibold tracking-tight text-text">
      {mark}
      {name}
    </h2>
    <div className="flex min-w-0 items-center gap-2">
      <span className="text-sm text-secondary tabular-nums">
        {total !== undefined && total > count ? `${count} of ${total}` : count}
      </span>
    </div>
  </header>
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
      // Taken only now that there is a column to move to: a key that is eaten without moving anything stops the page
      // scrolling as well.
      event.preventDefault();
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
      <div className={cn("flex min-h-full items-stretch gap-4", className)}>{children}</div>
      <DragOverlay dropAnimation={null}>
        {activeId ? <div {...{ [OVERLAY_ATTRIBUTE]: "" }}>{renderOverlay(activeId)}</div> : null}
      </DragOverlay>
    </DndContext>
  );
};
