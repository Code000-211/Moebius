// From BloopAI/vibe-kanban packages/ui/src/components/ToolStatusDot.tsx (Apache-2.0), changed: a status is never a hue
// alone, so the mark now carries the word with it, as a status icon and a word with no pill; a
// call that finished is a quiet gray check, because it is the common case on every row.
import { CheckIcon } from "@phosphor-icons/react";
import { cn } from "../lib/cn";
import { StatusIcon, type StatusState } from "../page/StatusIcon";

export interface ToolStatusLike {
  status: string;
}

interface ToolStatusDotProps {
  status: ToolStatusLike;
  className?: string;
}

/** The state each status is drawn as; a status that is not one of these has no icon, only its word. */
const STATES: Record<string, StatusState> = {
  success: "done",
  failed: "blocked",
  denied: "blocked",
  timed_out: "blocked",
  created: "inProgress",
  pending_approval: "notStarted",
};

/** The words each state is told by; a state outside this table keeps the word the server used. */
const WORDS: Record<string, string> = {
  success: "Done",
  failed: "Failed",
  denied: "Blocked",
  timed_out: "Timed out",
  created: "Running",
  pending_approval: "Needs approval",
};

/** The word a status is told by, so a caller can say it in its own sentence too. */
export function statusWord(status: string): string {
  return WORDS[status] ?? status.replaceAll("_", " ");
}

/** A tool's state: a status icon and the word, or one quiet mark where it finished. */
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

  const state = STATES[status.status];

  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-2 break-words text-xs font-semibold",
        state ? "text-text" : "text-secondary",
        className,
      )}
      data-status={status.status}
    >
      {state && <StatusIcon state={state} decorative />}
      {statusWord(status.status)}
    </span>
  );
}
