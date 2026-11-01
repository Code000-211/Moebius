// From BloopAI/vibe-kanban packages/ui/src/components/ToolStatusDot.tsx (Apache-2.0), changed: a status is never a hue
// alone, so the dot now carries the word with it, and the colours are the status tokens of styles/theme.css; a
// call that finished is a quiet gray check rather than a pill, because it is the common case on every row.
import { CheckIcon } from "@phosphor-icons/react";
import { cn } from "../lib/cn";

export interface ToolStatusLike {
  status: string;
}

interface ToolStatusDotProps {
  status: ToolStatusLike;
  className?: string;
}

/** The tint of a status label and the hue of its dot. */
interface StatusView {
  label: string;
  /** The hue of the mark beside the word; a state that is not one of the four has no mark. */
  dot?: string;
}

const NEUTRAL: StatusView = { label: "bg-sunken text-secondary" };

/** The words each state is told by; a state outside this table keeps the word the server used. */
const WORDS: Record<string, string> = {
  success: "Done",
  failed: "Failed",
  denied: "Blocked",
  timed_out: "Timed out",
  created: "Running",
  pending_approval: "Needs approval",
};

const STATUSES: Record<string, StatusView> = {
  success: { label: "bg-complete-fill text-complete-text", dot: "bg-complete" },
  failed: { label: "bg-error-fill text-error-text", dot: "bg-error" },
  denied: { label: "bg-error-fill text-error-text", dot: "bg-error" },
  timed_out: { label: "bg-error-fill text-error-text", dot: "bg-error" },
  created: { label: "bg-progress-fill text-progress-text", dot: "bg-progress" },
  pending_approval: { label: "bg-attention-fill text-attention-text", dot: "bg-attention" },
};

/** The word a status is told by, so a caller can say it in its own sentence too. */
export function statusWord(status: string): string {
  return WORDS[status] ?? status.replaceAll("_", " ");
}

/** A tool's state: the word and the hue that goes with it, or one quiet mark where it finished. */
export function ToolStatusDot({ status, className }: ToolStatusDotProps) {
  // Almost every row of a chat and of the Fleet finished, so a pill on each one competes with the rare row that
  // failed, is blocked or waits. The check says finished on its own; the word stays for a screen reader.
  if (status.status === "success") {
    return (
      <span
        className={cn("inline-flex shrink-0 items-center text-secondary", className)}
        data-status={status.status}
        title={statusWord(status.status)}
      >
        <CheckIcon aria-hidden="true" className="size-4" />
        <span className="sr-only">{statusWord(status.status)}</span>
      </span>
    );
  }

  const view = STATUSES[status.status] ?? NEUTRAL;

  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-2 break-words rounded-full px-2 py-1 text-xs font-semibold",
        view.label,
        className,
      )}
      data-status={status.status}
    >
      {view.dot && (
        <span
          aria-hidden="true"
          className={cn("size-2 shrink-0 rounded-full", view.dot)}
          data-status={status.status}
        />
      )}
      {statusWord(status.status)}
    </span>
  );
}
