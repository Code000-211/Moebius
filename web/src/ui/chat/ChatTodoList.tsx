// From BloopAI/vibe-kanban packages/ui/src/components/ChatTodoList.tsx (Apache-2.0), changed: no i18n; lucide's icons are Phosphor's; the colours are Deep Velvet names; the header is a real button.
import {
  CaretDownIcon,
  CheckIcon,
  CircleDashedIcon,
  CircleIcon,
  ListChecksIcon,
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

function getStatusIcon(status?: string | null) {
  const s = (status || "").toLowerCase();
  if (s === "completed") return <CheckIcon aria-hidden className="size-4 text-success" />;
  if (s === "in_progress" || s === "in-progress") {
    return <CircleDashedIcon aria-hidden className="size-4 text-accent" />;
  }
  return <CircleIcon aria-hidden className="size-4 text-low" />;
}

export function ChatTodoList({ todos, expanded, onToggle }: ChatTodoListProps) {
  return (
    <div className="text-sm">
      <button
        type="button"
        className="flex w-full items-center gap-base text-left text-low"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <ListChecksIcon className="size-icon-base shrink-0" />
        <span className="flex-1">Updated todos</span>
        <CaretDownIcon
          className={cn("size-icon-base shrink-0 transition-transform", expanded && "rotate-180")}
        />
      </button>
      {expanded && todos.length > 0 && (
        <ul className="ml-6 pt-base [&>li+li]:pt-1">
          {todos.map((todo, index) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a todo list is replaced whole, its items have no id
            <li key={`${todo.content}-${index}`} className="flex items-start gap-2">
              <span className="flex h-4 w-4 shrink-0 items-center justify-center pt-0.5">
                {getStatusIcon(todo.status)}
              </span>
              <span className="break-words leading-5">
                {todo.status?.toLowerCase() === "cancelled" ? (
                  <s className="text-low">{todo.content}</s>
                ) : (
                  todo.content
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
