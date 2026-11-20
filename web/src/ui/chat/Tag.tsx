/**
 * A short label that states a state or a place: the status of a chat, the agent it is with, the repository it works
 * in. It is a label and not a control, so it is words at caption size, never a coloured mark on its own, and it wraps
 * when the words are long.
 *
 * A tone names the state, never the colour: `progress`, `attention`, `complete` and `error` are the four the palette
 * carries, and `plain` is for a label that states a fact rather than a state. A toned tag is a state, so it is a
 * status icon and a word with no fill (the icon's name says the state); only a plain tag keeps its tint.
 */
import { cn } from "../lib/cn";
import { StatusIcon, type StatusState } from "../page/StatusIcon";

export type TagTone = "plain" | "progress" | "attention" | "complete" | "error";

const TONE_STATE: Record<Exclude<TagTone, "plain">, StatusState> = {
  progress: "running",
  attention: "needsYou",
  complete: "done",
  error: "needsYou",
};

export function Tag({
  tone = "plain",
  className,
  children,
}: {
  tone?: TagTone;
  className?: string;
  children: React.ReactNode;
}) {
  if (tone !== "plain") {
    return (
      <span
        className={cn(
          "inline-flex max-w-full items-center gap-1 break-words text-xs font-medium text-text",
          className,
        )}
      >
        <StatusIcon state={TONE_STATE[tone]} />
        {children}
      </span>
    );
  }
  return (
    <span
      className={cn(
        "inline-flex max-w-full items-center gap-1 break-words rounded-full bg-sunken px-2 py-0.5 text-xs font-medium text-secondary",
        className,
      )}
    >
      {children}
    </span>
  );
}
