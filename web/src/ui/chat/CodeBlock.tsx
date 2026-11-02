/**
 * A block of code with its language and a Copy button in its head; the highlighted code is the `children`. It sits on
 * the colour of code, borderless and at the control radius, with the code at the size of metadata in the one place
 * where characters are counted rather than read.
 */
import type { ReactNode } from "react";
import { CopyButton } from "./CopyButton";

export function CodeBlock({
  language,
  code,
  children,
}: {
  language?: string;
  /** The plain text, which Copy puts on the clipboard. */
  code: string;
  children: ReactNode;
}) {
  return (
    <div className="reveal-copy my-4 overflow-hidden rounded-control bg-code">
      <div className="flex min-h-10 items-center justify-between gap-4 px-3">
        <span className="text-xs font-semibold text-secondary">{language ?? "Plain text"}</span>
        <CopyButton text={code} label="Copy code" />
      </div>
      <pre className="overflow-x-auto px-3 pb-3 font-mono text-sm leading-5 text-text">
        {children}
      </pre>
    </div>
  );
}
