/**
 * A unified diff drawn for reading: a list of the files with what each adds and removes, and the lines of the file
 * chosen with their numbers. Written for Moebius on `parse.ts`. The lines show unified or side by side (old file left,
 * new file right). It is the densest screen of the app and stays that way — monospace at 13px, 24px lines — but long
 * lines wrap rather than run sideways, added and removed are named in words as well as tinted, and a file shows at most
 * `MAX_LINES` lines, with what is left out said.
 */
import { useMemo, useState } from "react";
import { cn } from "../ui/lib/cn";
import { plural } from "../ui/lib/format";
import { Empty } from "../ui/page/Empty";
import { type DiffLine, type FileDiff, parseUnifiedDiff, sideBySide, totals } from "./parse";

const MAX_LINES = 2000;

type Mode = "unified" | "side";

const STATUS_WORD: Record<FileDiff["status"], string> = {
  modified: "",
  added: "New file",
  deleted: "Deleted",
  renamed: "Renamed",
};

/** The tint behind a line of each kind; context and note lines sit on the code colour. */
const LINE_TONE = {
  added: "bg-diff-added",
  removed: "bg-diff-removed",
  context: "",
  note: "",
} as const;

const MARK_TONE = {
  added: "text-complete-text",
  removed: "text-error-text",
} as const;

const MARK_WORD = { added: "Added", removed: "Removed" } as const;

/** What a file adds and removes, as words and digits, never as colour alone. */
function Counts({ file }: { file: Pick<FileDiff, "additions" | "deletions"> }) {
  return (
    <span className="shrink-0 text-sm tabular-nums">
      <span className="text-complete-text">{file.additions} added</span>
      <span className="text-secondary"> · </span>
      <span className="text-error-text">{file.deletions} removed</span>
    </span>
  );
}

/** The edge stripe of a changed line: its hue again, at the edge of the tint. */
function Stripe({ kind }: { kind: "added" | "removed" }) {
  return (
    <span
      className={cn(
        "w-1 shrink-0 self-stretch",
        kind === "added" ? "bg-diff-added-mark" : "bg-diff-removed-mark",
      )}
      aria-hidden="true"
    />
  );
}

/** The marker column of a line: the sign, with the word for someone who reads it out. */
function Mark({ kind }: { kind: "added" | "removed" }) {
  return (
    <span className={cn("w-4 shrink-0 select-none font-semibold", MARK_TONE[kind])}>
      {kind === "added" ? "+" : "-"}
      <span className="sr-only">{MARK_WORD[kind]} line</span>
    </span>
  );
}

/** The line numbers of one side, in a column that never moves the text. */
function LineNumber({ value }: { value: number | null }) {
  return (
    <span className="w-10 shrink-0 select-none text-right text-secondary tabular-nums">
      {value ?? ""}
    </span>
  );
}

/**
 * One line of a column of the side by side view; null is the empty half of a row. Only the left column carries the
 * hairline, so the two files read as old and new rather than as two boxes.
 */
function Cell({ line, left }: { line: DiffLine | null; left: boolean }) {
  if (line === null) {
    return (
      <div
        data-testid="diff-cell"
        data-kind="empty"
        className={cn("min-w-0 bg-lane", left && "border-r border-hairline")}
      />
    );
  }
  return (
    <div
      data-testid="diff-cell"
      data-kind={line.kind}
      className={cn(
        "flex min-w-0 items-start gap-2 px-3",
        left && "border-r border-hairline",
        line.kind !== "context" && line.kind !== "note" && LINE_TONE[line.kind],
      )}
    >
      {line.kind !== "context" && line.kind !== "note" && <Stripe kind={line.kind} />}
      <LineNumber value={line.kind === "added" ? line.newNumber : line.oldNumber} />
      {line.kind !== "context" && line.kind !== "note" && <Mark kind={line.kind} />}
      <span className="min-w-0 break-words whitespace-pre-wrap text-text">{line.text}</span>
    </div>
  );
}

function SideBody({ file }: { file: FileDiff }) {
  return (
    <div className="bg-code py-2 font-mono text-sm leading-6" data-testid="diff-side">
      {file.hunks.map((hunk) => (
        <div key={hunk.header}>
          <Header line={hunk.header} />
          {sideBySide(hunk.lines).map((row, index) => (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: the rows are drawn once, in order
              key={index}
              className="grid grid-cols-2"
            >
              <Cell line={row.left} left />
              <Cell line={row.right} left={false} />
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

/** The `@@ … @@` line that says where the hunk sits. */
function Header({ line }: { line: string }) {
  return (
    <div className="bg-sunken px-3 text-secondary break-words whitespace-pre-wrap">{line}</div>
  );
}

function FileBody({ file, mode }: { file: FileDiff; mode: Mode }) {
  const lines = file.hunks.flatMap((hunk) => [
    { header: hunk.header as string | null, line: null },
    ...hunk.lines.map((line) => ({ header: null, line })),
  ]);
  const shown = lines.slice(0, MAX_LINES);
  if (file.binary) {
    return <p className="m-0 p-4 text-sm text-secondary">A binary file has no lines to show.</p>;
  }
  if (lines.length === 0) {
    return (
      <p className="m-0 p-4 text-sm text-secondary">
        {file.status === "renamed"
          ? `Renamed from ${file.oldPath}. No lines changed.`
          : "No lines changed."}
      </p>
    );
  }
  if (mode === "side") return <SideBody file={file} />;
  return (
    <div className="bg-code py-2 font-mono text-sm leading-6">
      {shown.map((entry, index) =>
        entry.line === null ? (
          <Header
            // biome-ignore lint/suspicious/noArrayIndexKey: the lines are drawn once, in order
            key={index}
            line={entry.header ?? ""}
          />
        ) : (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: the lines are drawn once, in order
            key={index}
            data-testid="diff-line"
            data-kind={entry.line.kind}
            className={cn(
              "flex items-start gap-2 px-3",
              entry.line.kind !== "context" &&
                entry.line.kind !== "note" &&
                LINE_TONE[entry.line.kind],
            )}
          >
            {entry.line.kind !== "context" && entry.line.kind !== "note" && (
              <Stripe kind={entry.line.kind} />
            )}
            <LineNumber value={entry.line.oldNumber} />
            <LineNumber value={entry.line.newNumber} />
            {entry.line.kind !== "context" && entry.line.kind !== "note" && (
              <Mark kind={entry.line.kind} />
            )}
            <span className="min-w-0 flex-1 break-words whitespace-pre-wrap text-text">
              {entry.line.text}
            </span>
          </div>
        ),
      )}
      {lines.length > shown.length && (
        <div className="px-3 py-2 text-sm text-secondary">
          {`${plural(lines.length - shown.length, "more line", "more lines")} left out`}
        </div>
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
      <div data-testid="diff-empty">
        <Empty
          title="No changes"
          body="Nothing was changed here yet. What a run or a change adds and removes appears in this list, file by file."
        />
      </div>
    );
  }
  const file = files[Math.min(chosen, files.length - 1)] as FileDiff;
  const sum = totals(files);
  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="diff">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <p className="m-0 text-sm text-secondary tabular-nums" data-testid="diff-summary">
          {`${plural(files.length, "file", "files")} · `}
          <span className="text-complete-text">{`${sum.additions} added`}</span>
          <span> · </span>
          <span className="text-error-text">{`${sum.deletions} removed`}</span>
          {truncated && <span>{" · the rest of the diff is left out"}</span>}
        </p>
        <ul
          /* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */
          role="list"
          aria-label="What the colours mean"
          className="m-0 flex flex-wrap items-center gap-2 p-0"
        >
          <li className="inline-flex items-center gap-2 rounded-full bg-diff-added px-3 py-1 text-sm font-semibold text-complete-text">
            <span className="size-2 shrink-0 rounded-full bg-diff-added-mark" aria-hidden="true" />
            Added
          </li>
          <li className="inline-flex items-center gap-2 rounded-full bg-diff-removed px-3 py-1 text-sm font-semibold text-error-text">
            <span
              className="size-2 shrink-0 rounded-full bg-diff-removed-mark"
              aria-hidden="true"
            />
            Removed
          </li>
        </ul>
      </div>
      <ul
        /* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */
        role="list"
        className="m-0 flex max-h-64 list-none flex-col gap-1 overflow-y-auto p-0"
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
                "flex min-h-10 w-full flex-wrap items-center gap-2 rounded-control px-3 py-2 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover",
                each === file ? "bg-selected text-text" : "text-secondary",
              )}
            >
              <span
                className="min-w-0 flex-1 break-all font-mono text-sm text-text"
                title={each.path}
              >
                {each.path}
              </span>
              {STATUS_WORD[each.status] && (
                <span className="shrink-0 text-sm font-semibold text-secondary">
                  {STATUS_WORD[each.status]}
                </span>
              )}
              <Counts file={each} />
            </button>
          </li>
        ))}
      </ul>
      <section className="overflow-hidden rounded-card bg-surface shadow-card">
        <header className="flex flex-col gap-2 p-4">
          <div className="flex flex-wrap items-center gap-4">
            <p
              className="m-0 min-w-0 flex-1 break-all font-mono text-sm font-semibold text-text"
              data-testid="diff-path"
              title={file.path}
            >
              {file.path}
            </p>
            {STATUS_WORD[file.status] && (
              <span className="text-sm font-semibold text-secondary">
                {STATUS_WORD[file.status]}
              </span>
            )}
            <Counts file={file} />
            <fieldset
              aria-label="Layout"
              className="m-0 flex gap-1 rounded-control border-0 bg-sunken p-1"
            >
              {(["unified", "side"] as const).map((each) => (
                <button
                  key={each}
                  type="button"
                  aria-pressed={mode === each}
                  onClick={() => setMode(each)}
                  className={cn(
                    "min-h-10 rounded-control px-3 text-sm font-semibold transition-colors duration-(--motion-routine) ease-out",
                    mode === each ? "bg-selected text-text" : "text-secondary hover:bg-hover",
                  )}
                >
                  {each === "unified" ? "Unified" : "Side by side"}
                </button>
              ))}
            </fieldset>
          </div>
        </header>
        <FileBody file={file} mode={mode} />
      </section>
    </div>
  );
}
