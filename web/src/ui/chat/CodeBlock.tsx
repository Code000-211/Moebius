/** A block of code with its language and a Copy button in the head; the highlighted code is the `children`. */
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
    <div className="my-base overflow-hidden rounded-md border bg-recessed">
      <div className="flex items-center justify-between border-b px-base py-half">
        <span className="font-mono text-xs text-low">{language ?? "text"}</span>
        <CopyButton text={code} label="Copy code" />
      </div>
      <pre className="overflow-x-auto p-base font-mono text-xs leading-5 text-high">{children}</pre>
    </div>
  );
}
