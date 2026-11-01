/**
 * One short line of what happened: an outcome (`ok`) or a refusal of the server (`error`). It says what happened and
 * what to do next, in the words of the standard, and sits on its own tint so it reads as a state, not as copy. The
 * words take the room that is left and wrap into it; the way out of it, when there is one, keeps its own place at the
 * end of the line.
 */
import { CheckCircleIcon, WarningCircleIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function Notice({
  tone = "ok",
  children,
  action,
  className,
}: {
  tone?: "ok" | "error";
  children: ReactNode;
  /** What to do about it, when there is something: dismiss it, try again. */
  action?: ReactNode;
  className?: string;
}) {
  const Icon = tone === "error" ? WarningCircleIcon : CheckCircleIcon;
  return (
    <p
      role={tone === "error" ? "alert" : "status"}
      className={cn(
        "m-0 flex min-h-10 flex-wrap items-center gap-3 rounded-control px-4 py-2 text-base",
        tone === "error" ? "bg-error-fill text-error-text" : "bg-complete-fill text-complete-text",
        className,
      )}
    >
      <Icon className="size-5 shrink-0" aria-hidden="true" />
      {/* The words are a box of their own, so they wrap into whatever is left over rather than the longest
          unbreakable word deciding how wide the sheet has to be. */}
      <span className="min-w-0 flex-1 break-words">{children}</span>
      {action && <span className="flex shrink-0 items-center gap-2">{action}</span>}
    </p>
  );
}
