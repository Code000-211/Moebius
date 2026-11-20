/**
 * A state said as a small mark and a line of grey text, the way the board, the Inbox and Fleet say what a thing is
 * doing: "Live" with the running ring and the agent's name, "Blocked" with the blocked icon, "Waiting on #24" and "Your
 * turn" as the grey text alone. It replaces the coloured pill (`Status`) for those pages; the word is always there,
 * so the mark is decorative and the colour is never the only signal. The label is never a control.
 */
import { cn } from "../lib/cn";
import { StatusIcon, type StatusState } from "./StatusIcon";

/** The mark before the word: the running ring, one of the status icons, or none for a state that is only said. */
export type StateMark = "none" | "live" | StatusState;

export function MarkIcon({ mark }: { mark: StateMark }) {
  if (mark === "none") return null;
  if (mark === "live") return <StatusIcon state="running" decorative />;
  return <StatusIcon state={mark} decorative />;
}

export function MarkLabel({
  mark = "none",
  word,
  detail,
  className,
}: {
  mark?: StateMark;
  word: string;
  /** Said after the word, such as the agent of a live card. */
  detail?: string;
  className?: string;
}) {
  return (
    <span
      className={cn("inline-flex min-w-0 items-center gap-1.5 text-sm text-secondary", className)}
    >
      <MarkIcon mark={mark} />
      <span className="[overflow-wrap:anywhere]">{word}</span>
      {detail && <span className="[overflow-wrap:anywhere]">· {detail}</span>}
    </span>
  );
}
