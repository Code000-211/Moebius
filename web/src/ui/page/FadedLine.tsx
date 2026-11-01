/**
 * One line of text that stays on one line: a command, a path, the name of a chat. When the words are wider than the
 * box the line fades out where it ran short, rather than being cut mid-word, and which end stays in view is the whole
 * choice: the beginning of a name is the part that identifies it, the end of a command or a path is the part that says
 * what it was. A line that fits is drawn plain, because a fade that lies about hidden content is worse than none, and
 * the rest of a faded line is never far off: the row it belongs to opens on a click, and its tooltip holds the whole
 * text. A line that shows all of itself is given no tooltip, because it would only repeat what is already read.
 *
 * It masks its own edge rather than laying a gradient over the top, so the words fade into whatever the line is drawn
 * on, the same way [`ScrollFades`](../../app/board/ScrollFades.tsx) fades a scroller.
 */
import { type ReactNode, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";

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
  const box = useRef<HTMLSpanElement | null>(null);
  const watch = useRef<ResizeObserver | null>(null);
  const watched = useRef(new Set<Element>());
  const [runsOut, setRunsOut] = useState(false);
  // The words are measured against the room they are given: a box wider than the line it holds has nothing to fade.
  // Measuring only asks to render when the answer changes, so a render that changed nothing costs one read.
  const measure = useCallback(() => {
    const line = box.current;
    const words = line?.firstElementChild;
    if (!line || !words) return;
    const next = words.getBoundingClientRect().width > line.clientWidth + 1;
    setRunsOut((held) => (held === next ? held : next));
  }, []);

  // Every render measures once more, so words that arrived with it are measured with them, and each box is watched for
  // the day the column it sits in narrows under it — watched once, not on every render.
  useLayoutEffect(() => {
    measure();
    const line = box.current;
    if (!line || typeof ResizeObserver === "undefined") return;
    if (!watch.current) watch.current = new ResizeObserver(measure);
    const observer = watch.current;
    if (!watched.current.has(line)) {
      watched.current.add(line);
      observer.observe(line);
    }
    const words = line.firstElementChild;
    if (words && !watched.current.has(words)) {
      watched.current.add(words);
      observer.observe(words);
    }
  });
  useEffect(
    () => () => {
      watch.current?.disconnect();
      watch.current = null;
      watched.current.clear();
    },
    [],
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
