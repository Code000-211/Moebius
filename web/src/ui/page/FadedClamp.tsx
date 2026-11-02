/**
 * The first three lines of a longer piece of writing, the last one fading out: enough of an issue to tell one card
 * from the next in a column of them, without the column growing to the length of the longest description in it. The
 * whole of the writing is never far — the detail beside the list, the dock beside the board — which is what makes
 * cutting it honest, and why a piece that fits the three lines is drawn plain: a fade that lies about hidden content
 * is worse than none, so the box is measured before it is allowed to fade.
 *
 * Newlines are kept soft, because a Markdown file is hard-wrapped by whoever wrote it: the lines on the card are the
 * width of the card, not the width of the source.
 */
import { useState } from "react";
import { cn } from "../lib/cn";
import { useBoxWatch } from "../lib/useBoxWatch";

export function FadedClamp({ text, className }: { text: string; className?: string }) {
  const [cut, setCut] = useState(false);
  // A box whose content is taller than it is has lines behind the last one it shows.
  const { box } = useBoxWatch<HTMLSpanElement>((el) => {
    const next = el.scrollHeight > el.clientHeight + 1;
    setCut((held) => (held === next ? held : next));
  });
  return (
    <span ref={box} className={cn("clamp-3 break-words", cut && "fade-last-3", className)}>
      {text}
    </span>
  );
}
