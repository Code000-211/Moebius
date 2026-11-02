/**
 * A unified diff drawn for reading, in the two columns the Files box of a repository uses: the names of the files a
 * change touched down the left, each with what it costs, and the lines of the file chosen down the right, with their
 * numbers. Written for Moebius on `parse.ts`. The lines show unified or side by side (old file left, new file right).
 * It is the densest screen of the app and stays that way — monospace at 13px, 24px lines — but long lines wrap rather
 * than run sideways, a change is a sign as well as a tint, and a file shows at most `MAX_LINES` lines, with what is
 * left out said. Each column scrolls on its own, so picking the next file never moves the list you picked from.
 */
import { useMemo, useState } from "react";
import { FileRow, FolderRow } from "../ui/files/TreeRows";
import { type PathNode, treeOf } from "../ui/files/tree";
import { cn } from "../ui/lib/cn";
import { plural } from "../ui/lib/format";
import { Empty } from "../ui/page/Empty";
import { Delta } from "./Delta";
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

/**
 * The sidebar: the paths a change touched, nested into the folders they sit in. Every folder starts open, because a
 * change names few enough files to read at once and a closed folder would hide the very file someone is looking for;
 * what a person closes stays closed while the diff is on the screen. Each file carries what it costs, so the column
 * can be scanned for the biggest thing in it.
 */
function TreeBranch({
  nodes,
  depth,
  costs,
  chosen,
  onSelect,
  closed,
  onToggle,
}: {
  nodes: PathNode[];
  depth: number;
  /** What each path adds and removes, by path. */
  costs: Map<string, FileDiff>;
  chosen: string;
  onSelect: (path: string) => void;
  closed: ReadonlySet<string>;
  onToggle: (path: string) => void;
}) {
  return (
    <>
      {nodes.map((node) => {
        if (node.kind === "dir") {
          const isOpen = !closed.has(node.path);
          return (
            <li key={node.path}>
              <FolderRow
                name={node.name}
                path={node.path}
                depth={depth}
                open={isOpen}
                onToggle={onToggle}
              />
              {isOpen && (
                <ul className="m-0 list-none p-0">
                  <TreeBranch
                    nodes={node.children}
                    depth={depth + 1}
                    costs={costs}
                    chosen={chosen}
                    onSelect={onSelect}
                    closed={closed}
                    onToggle={onToggle}
                  />
                </ul>
              )}
            </li>
          );
        }
        const cost = costs.get(node.path);
        return (
          <li key={node.path} data-testid="diff-file">
            <FileRow
              name={node.name}
              path={node.path}
              depth={depth}
              chosen={node.path === chosen}
              onOpen={onSelect}
              trailing={
                cost && (
                  <Delta
                    additions={cost.additions}
                    deletions={cost.deletions}
                    className="shrink-0 text-sm"
                  />
                )
              }
            />
          </li>
        );
      })}
    </>
  );
}

export function DiffView({ diff, truncated = false }: { diff: string; truncated?: boolean }) {
  const files = useMemo(() => parseUnifiedDiff(diff), [diff]);
  const nodes = useMemo(() => treeOf(files.map((each) => each.path)), [files]);
  const costs = useMemo(() => new Map(files.map((each) => [each.path, each] as const)), [files]);
  const [chosen, setChosen] = useState<string | null>(null);
  /** The folders a person closed. Everything else is open, so a diff says all of itself first. */
  const [closed, setClosed] = useState<ReadonlySet<string>>(new Set());
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
  // The first file of the diff, until a row is clicked; a file that leaves the diff takes the next one in line.
  const file = files.find((each) => each.path === chosen) ?? (files[0] as FileDiff);
  const sum = totals(files);
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4" data-testid="diff">
      {/* What the change costs, in one line. It needs no key to the colours: each number carries its own sign. */}
      <p className="m-0 text-sm text-secondary tabular-nums" data-testid="diff-summary">
        {`${plural(files.length, "file", "files")} · `}
        <Delta additions={sum.additions} deletions={sum.deletions} />
        {truncated && <span>{" · the rest of the diff is left out"}</span>}
      </p>
      {/* Two inset columns separated by space alone, as the Files box of a repository is: the names where they are
          compared, the lines where they are read, each scrolling on its own. The list stops where its last row stops,
          as the repository's tree does; held to the height of the column beside it, it would read as a box that
          failed to fill. */}
      <div className="flex min-h-0 flex-1 flex-col gap-4 md:flex-row md:items-stretch">
        <div className="flex w-full min-w-0 shrink-0 flex-col md:w-64">
          <div
            data-testid="diff-files"
            className="w-full min-h-0 shrink max-h-96 overflow-y-auto rounded-card bg-sunken p-2 md:max-h-none"
          >
            <ul
              /* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */
              role="list"
              aria-label="Changed files"
              className="m-0 list-none p-0 text-base"
            >
              <TreeBranch
                nodes={nodes}
                depth={0}
                costs={costs}
                chosen={file.path}
                onSelect={setChosen}
                closed={closed}
                onToggle={(path) =>
                  setClosed((all) => {
                    const next = new Set(all);
                    if (!next.delete(path)) next.add(path);
                    return next;
                  })
                }
              />
            </ul>
          </div>
        </div>
        {/* The code block ends where its last line does. Only the header and the lines carry the colour of code,
            so a file with a short diff does not trail a band of empty code down to the bottom of the taller list
            beside it; the lines shrink to the column and scroll there when a diff is longer than it. */}
        <section className="flex min-w-0 flex-1 flex-col">
          <header className="flex shrink-0 flex-wrap items-center gap-3 rounded-t-card bg-code px-3 py-1">
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
            <Delta additions={file.additions} deletions={file.deletions} className="text-sm" />
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
          </header>
          <div className="min-h-0 overflow-y-auto rounded-b-card bg-code" data-testid="diff-lines">
            <FileBody file={file} mode={mode} />
          </div>
        </section>
      </div>
    </div>
  );
}
