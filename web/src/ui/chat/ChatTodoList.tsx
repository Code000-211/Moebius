// From BloopAI/vibe-kanban packages/ui/src/components/ChatTodoList.tsx (Apache-2.0), changed: no i18n; lucide's icons
// are Phosphor's; the header is a real button 40px tall and says how far the list has come; every item is body text
// with its state said in a word as well as shown by the shape of its icon.
import {
  CaretDownIcon,
  CheckCircleIcon,
  CircleIcon,
  ListChecksIcon,
  ProhibitIcon,
} from "@phosphor-icons/react";
import { cn } from "../lib/cn";

export interface TodoItemLike {
  content: string;
  status?: string | null;
}

interface ChatTodoListProps {
  todos: TodoItemLike[];
  expanded?: boolean;
  onToggle?: () => void;
}

/**
 * What a state of one item is called, and the shape and hue that say it without a word. `current` marks the item the
 * agent is at now, which is the one line of the list drawn in the full colour of the text.
 */
function stateOf(status?: string | null) {
  const s = (status || "").toLowerCase();
  if (s === "completed")
    return { word: "Done", Icon: CheckCircleIcon, tone: "text-complete", current: false } as const;
  if (s === "in_progress" || s === "in-progress")
    return { word: "Working", Icon: CircleIcon, tone: "text-progress", current: true } as const;
  if (s === "cancelled")
    return { word: "Dropped", Icon: ProhibitIcon, tone: "text-secondary", current: false } as const;
  return { word: "Not started", Icon: CircleIcon, tone: "text-secondary", current: false } as const;
}

export function ChatTodoList({ todos, expanded, onToggle }: ChatTodoListProps) {
  const done = todos.filter((todo) => todo.status?.toLowerCase() === "completed").length;

  return (
    <div className="max-w-prose">
      <button
        type="button"
        className="flex min-h-10 w-full cursor-pointer items-center gap-3 rounded-control px-3 text-left text-base font-semibold text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <ListChecksIcon aria-hidden="true" className="size-4 shrink-0" />
        <span className="flex-1 break-words">Todos</span>
        <span className="shrink-0 font-normal tabular-nums">
          {done} of {todos.length} done
        </span>
        <CaretDownIcon
          aria-hidden="true"
          className={cn(
            "size-4 shrink-0 transition-transform duration-(--motion-routine) ease-out",
            expanded && "rotate-180",
          )}
        />
      </button>
      {expanded && todos.length > 0 && (
        <ul className="flex flex-col gap-2 px-3 pb-2">
          {todos.map((todo, index) => {
            const state = stateOf(todo.status);
            const dropped = todo.status?.toLowerCase() === "cancelled";
            return (
              // biome-ignore lint/suspicious/noArrayIndexKey: a todo list is replaced whole, its items have no id
              <li key={`${todo.content}-${index}`} className="flex items-start gap-3">
                <span className="flex items-center gap-2 pt-1">
                  <state.Icon aria-hidden="true" className={cn("size-4 shrink-0", state.tone)} />
                  <span className="sr-only">{state.word}</span>
                </span>
                <span
                  className={cn(
                    "min-w-0 break-words text-base leading-relaxed",
                    state.current ? "text-text" : "text-secondary",
                    dropped && "line-through",
                  )}
                >
                  {todo.content}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
