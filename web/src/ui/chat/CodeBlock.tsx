/**
 * A block of code on the dark indigo ground of code, the same in both themes, open by default. Its header row says
 * the language, the file when the fence names one (`rust path=src/engine.rs`) and carries Copy, set in mono in the
 * head colour over a faint hairline; the code below is highlighted with the syntax colours of the theme by the shared
 * grammar (ui/code/highlight.ts, loaded lazily — until it arrives, the plain text).
 */
import { lazy, Suspense } from "react";
import { CopyButton } from "./CopyButton";

const CodeHighlight = lazy(() => import("../code/CodeHighlight"));

/**
 * The file a fence's info string names: the value of `path=`, `file=` or `title=`, with or without quotes
 * (`rust path=src/engine.rs`). The info string is whatever follows the language on the fence line.
 */
export function fileOf(meta: string | undefined): string | undefined {
  const found = meta?.match(/(?:^|\s)(?:path|file|title)=(?:"([^"]*)"|'([^']*)'|(\S+))/);
  const name = found?.[1] ?? found?.[2] ?? found?.[3];
  return name ? name : undefined;
}

export function CodeBlock({
  language,
  file,
  code,
}: {
  language?: string;
  file?: string;
  code: string;
}) {
  return (
    <div className="reveal-copy my-4 overflow-hidden rounded-card bg-code-bg">
      <div className="flex h-[30px] items-center justify-between gap-4 pr-1 pl-3 font-mono text-[12px] text-code-head">
        <span className="flex min-w-0 items-center gap-2">
          <span>{language ?? "Plain text"}</span>
          {file && (
            <>
              <span aria-hidden="true">·</span>
              <span className="truncate">{file}</span>
            </>
          )}
        </span>
        <CopyButton
          text={code}
          label="Copy code"
          className="font-mono text-[12px] text-code-head opacity-100! hover:bg-code-head/15 hover:text-code-text"
        />
      </div>
      <pre className="m-0 overflow-x-auto px-3 py-3 font-mono text-[12px] leading-[1.65] text-code-text">
        <Suspense fallback={<code className="whitespace-pre">{code}</code>}>
          <CodeHighlight code={code} language={language} />
        </Suspense>
      </pre>
    </div>
  );
}
