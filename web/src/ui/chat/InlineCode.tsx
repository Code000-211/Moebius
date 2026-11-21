import type { ReactNode } from "react";

/**
 * A word of code inside a sentence: mono at 0.9em in the text colour on a faint grey step, so it reads as part of the
 * line. The dark `--code-bg` is for a fenced block (`CodeBlock`) only. The font size is relative and sets no line
 * height, and the padding is inline, so a span never makes its line taller.
 */
export function InlineCode({ children }: { children: ReactNode }) {
  return (
    <code className="break-words rounded-small bg-control-fill px-1 py-px font-mono text-[0.9em] text-text">
      {children}
    </code>
  );
}
