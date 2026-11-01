// From BloopAI/vibe-kanban packages/ui/src/components/ChatErrorMessage.tsx (Apache-2.0), changed: what went wrong is
// shown whole — a provider's refusal is the thing to read, so it is never cut and there is nothing to open; the box is
// the tint of an error with the words of it in the colour of text, and role="alert" so it is announced. It is as wide
// as the message it answers, and it is not set inside a card: a tinted box floating in a white one reads as two
// notices about the same thing.
import { WarningCircleIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

interface ChatErrorMessageProps {
  content: string;
  /** What kind of error it is, such as "The run failed". */
  title?: string;
  className?: string;
  /** A button on the right, such as Copy. */
  actions?: ReactNode;
}

export function ChatErrorMessage({ content, title, className, actions }: ChatErrorMessageProps) {
  return (
    <div
      role="alert"
      className={cn(
        "reveal-copy flex flex-wrap items-start gap-3 rounded-card bg-error-fill p-4",
        className,
      )}
    >
      <WarningCircleIcon aria-hidden="true" className="mt-1 size-5 shrink-0 text-error" />
      <div className="min-w-0 flex-1">
        {title && <p className="text-sm font-semibold text-error-text">{title}</p>}
        <p className="mt-1 break-words text-base leading-relaxed text-text">{content}</p>
      </div>
      {actions}
    </div>
  );
}
