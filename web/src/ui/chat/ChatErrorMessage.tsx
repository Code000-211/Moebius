// From BloopAI/vibe-kanban packages/ui/src/components/ChatErrorMessage.tsx (Apache-2.0), changed: a box with the error and a Copy button, shown whole (an error of a provider is the thing to read); role="alert"; the `content` can be opened and closed only when a caller passes `onToggle`.
import { WarningCircleIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

interface ChatErrorMessageProps {
  content: string;
  /** What kind of error it is, such as "The run failed". */
  title?: string;
  className?: string;
  expanded?: boolean;
  onToggle?: () => void;
  /** A button on the right, such as Copy. */
  actions?: ReactNode;
}

export function ChatErrorMessage({
  content,
  title,
  className,
  expanded = true,
  onToggle,
  actions,
}: ChatErrorMessageProps) {
  return (
    <div
      role="alert"
      className={cn(
        "flex items-start gap-base rounded-md border border-error/40 bg-error/10 px-plusfifty py-base text-sm text-error",
        className,
      )}
    >
      <WarningCircleIcon className="size-icon-base shrink-0 pt-0.5" />
      <div className="min-w-0 flex-1">
        {title && <div className="text-xs font-bold">{title}</div>}
        <button
          type="button"
          className={cn(
            "block w-full text-left",
            !expanded && "truncate",
            expanded && "whitespace-pre-wrap break-words",
            !onToggle && "cursor-default",
          )}
          onClick={onToggle}
          tabIndex={onToggle ? 0 : -1}
        >
          {content}
        </button>
      </div>
      {actions}
    </div>
  );
}
