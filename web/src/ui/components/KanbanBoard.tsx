// From BloopAI/vibe-kanban packages/ui/src/components/KanbanBoard.tsx (Apache-2.0), changed: no i18n and no "add task" button (a card comes from an issue); a column is a lane box with a border colour that says what its cards do; the header takes a CSS colour and a count and shows the column's script; the drag context is a flex row of columns, not a divided grid; the Feature and Status types of the Gantt chart are gone; Card is a plain div (React 19 ref prop); the card takes a selected/live look from Deep Velvet and is dragged by its whole body, with the keyboard too (the original had a grip for phones, which the app does not need).
import {
  DragDropContext,
  Draggable,
  type DraggableProvided,
  type DraggableStateSnapshot,
  Droppable,
  type DroppableProvided,
  type DropResult,
} from "@hello-pangea/dnd";
import type { KeyboardEvent, MouseEvent, ReactNode, Ref, RefObject } from "react";
import { cn } from "../lib/cn";

export type { DropResult } from "@hello-pangea/dnd";

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
  return (
    <section
      aria-label={name}
      className={cn(
        "flex min-h-40 min-w-[220px] flex-1 flex-col rounded-xl border bg-lane p-[10px]",
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

export type KanbanCardProps = {
  id: string;
  /** The name of the card, for screen readers. */
  name: string;
  index: number;
  children?: ReactNode;
  className?: string;
  onClick?: (e: MouseEvent<HTMLDivElement>) => void;
  tabIndex?: number;
  forwardedRef?: Ref<HTMLDivElement>;
  onKeyDown?: (e: KeyboardEvent) => void;
  isOpen?: boolean;
  isSelected?: boolean;
  dragDisabled?: boolean;
  /** The card is being worked on: its border is the live colour. */
  isLive?: boolean;
  testId?: string;
};

export const KanbanCard = ({
  id,
  name,
  index,
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
  return (
    <Draggable draggableId={id} index={index} isDragDisabled={dragDisabled}>
      {(provided: DraggableProvided, snapshot: DraggableStateSnapshot) => {
        // Combine DnD ref and forwarded ref
        const setRefs = (node: HTMLDivElement | null) => {
          provided.innerRef(node);
          if (typeof forwardedRef === "function") {
            forwardedRef(node);
          } else if (forwardedRef) {
            (forwardedRef as RefObject<HTMLDivElement | null>).current = node;
          }
        };

        return (
          // biome-ignore lint/a11y/useSemanticElements: the drag library needs this element as its handle (it supplies the same role and tabindex through dragHandleProps); a card holds links, which a <button> cannot
          <div
            role="button"
            tabIndex={tabIndex ?? 0}
            data-testid={testId}
            aria-label={name}
            className={cn(
              "mb-[8px] rounded-lg border bg-card p-[10px] outline-none",
              "focus-visible:ring-2 focus-visible:ring-accent",
              isLive && "border-live-border",
              snapshot.isDragging && "cursor-grabbing shadow-lg",
              isSelected
                ? "border-accent ring-1 ring-accent ring-inset"
                : isOpen && "ring-2 ring-secondary-foreground ring-inset",
              className,
            )}
            ref={setRefs}
            {...provided.draggableProps}
            {...provided.dragHandleProps}
            onClick={(e) => {
              if (!snapshot.isDragging) onClick?.(e);
            }}
            onKeyDown={onKeyDown}
          >
            {children ?? <p className="m-0 font-medium text-sm">{name}</p>}
          </div>
        );
      }}
    </Draggable>
  );
};

// =============================================================================
// Kanban Cards Container
// =============================================================================

export type KanbanCardsProps = {
  id: string;
  children: ReactNode;
  className?: string;
};

export const KanbanCards = ({ id, children, className }: KanbanCardsProps) => (
  <Droppable droppableId={id}>
    {(provided: DroppableProvided) => (
      <div
        className={cn("flex flex-1 flex-col", className)}
        ref={provided.innerRef}
        {...provided.droppableProps}
      >
        {children}
        {provided.placeholder}
      </div>
    )}
  </Droppable>
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
// Kanban Provider (DragDropContext)
// =============================================================================

export type KanbanProviderProps = {
  children: ReactNode;
  onDragEnd: (result: DropResult) => void;
  className?: string;
};

export const KanbanProvider = ({ children, onDragEnd, className }: KanbanProviderProps) => {
  return (
    <DragDropContext onDragEnd={onDragEnd}>
      <div className={cn("flex items-stretch gap-[10px] min-h-full", className)}>{children}</div>
    </DragDropContext>
  );
};
