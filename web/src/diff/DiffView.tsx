/**
 * A unified diff drawn for reading: a list of the files with their added and removed lines, and the lines of the file
 * chosen with their numbers, the added ones in green and the removed ones in red. Written for Moebius on `parse.ts`.
 * The lines show unified or side by side (old file left, new file right). A file shows at most `MAX_LINES` lines; the rest is said to be left out.
 */
import { useMemo, useState } from "react";
import { cn } from "../ui/lib/cn";
import { type DiffLine, type FileDiff, parseUnifiedDiff, sideBySide, totals } from "./parse";

const MAX_LINES = 2000;

type Mode = "unified" | "side";

const STATUS_WORD: Record<FileDiff["status"], string> = {
  modified: "",
  added: "new",
  deleted: "deleted",
  renamed: "renamed",
};

function Counts({ file }: { file: Pick<FileDiff, "additions" | "deletions"> }) {
  return (
    <span className="shrink-0 font-mono text-[11px]">
      <span className="text-success">+{file.additions}</span>{" "}
      <span className="text-error">-{file.deletions}</span>
    </span>
  );
}

function Cell({ line }: { line: DiffLine | null }) {
  return (
    <div
      data-testid="diff-cell"
      data-kind={line?.kind ?? "empty"}
      className={cn(
        "flex min-w-0 whitespace-pre",
        line?.kind === "added" && "bg-teal-fill text-success",
        line?.kind === "removed" && "bg-red-fill text-error",
        (line?.kind === "context" || line?.kind === "note") && "text-normal",
      )}
    >
      <span className="w-10 shrink-0 select-none pr-base text-right text-low">
        {line ? (line.kind === "added" ? line.newNumber : line.oldNumber) : ""}
      </span>
      <span>{line?.text}</span>
    </div>
  );
}

function SideBody({ file }: { file: FileDiff }) {
  return (
    <div
      className="overflow-x-auto bg-recessed py-half font-mono text-xs leading-5"
      data-testid="diff-side"
    >
      {file.hunks.map((hunk) => (
        <div key={hunk.header}>
          <div className="whitespace-pre bg-lane px-base text-low">{hunk.header}</div>
          {sideBySide(hunk.lines).map((row, index) => (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: the rows are drawn once, in order
              key={index}
              className="grid grid-cols-2 divide-x divide-line"
            >
              <Cell line={row.left} />
              <Cell line={row.right} />
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

function FileBody({ file, mode }: { file: FileDiff; mode: Mode }) {
  const lines = file.hunks.flatMap((hunk) => [
    { header: hunk.header as string | null, line: null },
    ...hunk.lines.map((line) => ({ header: null, line })),
  ]);
  const shown = lines.slice(0, MAX_LINES);
  if (file.binary) return <p className="m-0 p-base text-xs text-low">A binary file.</p>;
  if (lines.length === 0) {
    return (
      <p className="m-0 p-base text-xs text-low">
        {file.status === "renamed" ? `Renamed from ${file.oldPath}.` : "No lines changed."}
      </p>
    );
  }
  if (mode === "side") return <SideBody file={file} />;
  return (
    <div className="overflow-x-auto bg-recessed py-half font-mono text-xs leading-5">
      {shown.map((entry, index) =>
        entry.line === null ? (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: the lines are drawn once, in order
            key={index}
            className="whitespace-pre bg-lane px-base text-low"
          >
            {entry.header}
          </div>
        ) : (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: the lines are drawn once, in order
            key={index}
            data-testid="diff-line"
            data-kind={entry.line.kind}
            className={cn(
              "flex whitespace-pre",
              entry.line.kind === "added" && "bg-teal-fill text-success",
              entry.line.kind === "removed" && "bg-red-fill text-error",
              (entry.line.kind === "context" || entry.line.kind === "note") && "text-normal",
            )}
          >
            <span className="w-10 shrink-0 select-none pr-half text-right text-low">
              {entry.line.oldNumber ?? ""}
            </span>
            <span className="w-10 shrink-0 select-none pr-base text-right text-low">
              {entry.line.newNumber ?? ""}
            </span>
            <span className="w-3 shrink-0 select-none">
              {entry.line.kind === "added" ? "+" : entry.line.kind === "removed" ? "-" : ""}
            </span>
            <span>{entry.line.text}</span>
          </div>
        ),
      )}
      {lines.length > shown.length && (
        <div className="px-base text-low">… {lines.length - shown.length} more lines left out</div>
      )}
    </div>
  );
}

export function DiffView({ diff, truncated = false }: { diff: string; truncated?: boolean }) {
  const files = useMemo(() => parseUnifiedDiff(diff), [diff]);
  const [chosen, setChosen] = useState(0);
  const [mode, setMode] = useState<Mode>("unified");
  if (files.length === 0) {
    return (
      <p className="m-0 text-xs text-low" data-testid="diff-empty">
        No changes.
      </p>
    );
  }
  const file = files[Math.min(chosen, files.length - 1)] as FileDiff;
  const sum = totals(files);
  return (
    <div className="flex min-w-0 flex-col gap-base" data-testid="diff">
      <div className="font-mono text-[11px] text-low" data-testid="diff-summary">
        {files.length} {files.length === 1 ? "file" : "files"} ·{" "}
        <span className="text-success">+{sum.additions}</span>{" "}
        <span className="text-error">-{sum.deletions}</span>
        {truncated && " · the rest of the diff is left out"}
      </div>
      <ul
        className="m-0 flex max-h-40 list-none flex-col gap-[2px] overflow-y-auto p-0"
        aria-label="Changed files"
      >
        {files.map((each, index) => (
          <li key={each.path}>
            <button
              type="button"
              data-testid="diff-file"
              aria-current={each === file ? "true" : undefined}
              onClick={() => setChosen(index)}
              className={cn(
                "flex w-full items-center gap-base rounded-sm px-base py-[2px] text-left text-xs hover:bg-card",
                each === file ? "bg-card text-high" : "text-normal",
              )}
            >
              <span className="min-w-0 flex-1 truncate font-mono">{each.path}</span>
              {STATUS_WORD[each.status] && (
                <span className="shrink-0 text-[11px] text-low">{STATUS_WORD[each.status]}</span>
              )}
              <Counts file={each} />
            </button>
          </li>
        ))}
      </ul>
      <div className="overflow-hidden rounded-md border border-line">
        <div className="flex items-center gap-base border-b border-line bg-surface px-base py-half">
          <span
            className="min-w-0 flex-1 truncate font-mono text-xs text-high"
            data-testid="diff-path"
          >
            {file.path}
          </span>
          <Counts file={file} />
          <fieldset aria-label="Layout" className="m-0 flex gap-[2px] border-0 p-0">
            {(["unified", "side"] as const).map((each) => (
              <button
                key={each}
                type="button"
                aria-pressed={mode === each}
                onClick={() => setMode(each)}
                className={cn(
                  "rounded-sm px-base py-[2px] text-[11px]",
                  mode === each ? "bg-card text-high" : "text-low hover:bg-card",
                )}
              >
                {each === "unified" ? "Unified" : "Side by side"}
              </button>
            ))}
          </fieldset>
        </div>
        <FileBody file={file} mode={mode} />
      </div>
    </div>
  );
}
