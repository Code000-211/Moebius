/**
 * One line of text that stays on one line: a command, a path, the name of a chat. When the words are wider than the
 * box the line fades out where it ran short, rather than being cut mid-word, and which end stays in view is the whole
 * choice: the beginning of a name is the part that identifies it, the end of a command or a path is the part that says
 * what it was. A line that fits is drawn plain, because a fade that lies about hidden content is worse than none, and
 * the rest of a faded line is never far off: the row it belongs to opens on a click, and its tooltip holds the whole
 * text. A line that shows all of itself is given no tooltip, because it would only repeat what is already read.
 *
 * It masks its own edge rather than laying a gradient over the top, so the words fade into whatever the line is drawn
 * on, the same way [`ScrollFades`](./ScrollFades.tsx) fades a scroller.
 */
import { type ReactNode, useState } from "react";
import { cn } from "../lib/cn";
import { useBoxWatch } from "../lib/useBoxWatch";

export function FadedLine({
  text,
  keep = "tail",
  title,
  className,
}: {
  /** The words of the line. */
  text: ReactNode;
  /** Which end stays in view when the line runs out: its beginning (`head`) or its end (`tail`). */
  keep?: "head" | "tail";
  /** The whole of a line whose head or tail is faded out, for the tooltip of the row it sits in. */
  title?: string;
  className?: string;
}) {
  const [runsOut, setRunsOut] = useState(false);
  // The words are measured against the room they are given: a box wider than the line it holds has nothing to fade,
  // and the words are watched too, because it is a new word that usually makes a line run out.
  const { box } = useBoxWatch<HTMLSpanElement>(
    (line) => {
      const words = line.firstElementChild;
      if (!words) return;
      const next = words.getBoundingClientRect().width > line.clientWidth + 1;
      setRunsOut((held) => (held === next ? held : next));
    },
    { children: true },
  );
  return (
    <span
      ref={box}
      title={runsOut ? title : undefined}
      className={cn(
        "min-w-0 overflow-hidden whitespace-nowrap",
        runsOut && (keep === "tail" ? "fade-tail" : "fade-end"),
        className,
      )}
    >
      <span>{text}</span>
    </span>
  );
}
