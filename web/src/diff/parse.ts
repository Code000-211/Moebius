/**
 * Reads a unified diff, as `git diff` writes it, into files with their hunks and lines, for the diff view. It keeps what
 * the view shows: the path (and the old one of a rename), whether the file was added, deleted or is binary, each hunk's
 * header and its lines with their numbers in the old and the new file. A line that is not part of a diff is dropped.
 */

export type LineKind = "context" | "added" | "removed" | "note";

export interface DiffLine {
  kind: LineKind;
  /** The text without the leading `+`, `-` or space. */
  text: string;
  /** The number in the old file; none for an added line. */
  oldNumber: number | null;
  /** The number in the new file; none for a removed line. */
  newNumber: number | null;
}

export interface Hunk {
  /** The `@@ -a,b +c,d @@ heading` line. */
  header: string;
  lines: DiffLine[];
}

export type FileStatus = "modified" | "added" | "deleted" | "renamed";

export interface FileDiff {
  /** The path in the new tree (the old one for a deleted file). */
  path: string;
  /** The path before, for a rename. */
  oldPath: string | null;
  status: FileStatus;
  binary: boolean;
  additions: number;
  deletions: number;
  hunks: Hunk[];
}

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/** The path of a `--- a/x` or `+++ b/x` line, without the `a/` or `b/`; none for `/dev/null`. */
function pathOf(line: string): string | null {
  let path = line.slice(4).split("\t")[0] ?? "";
  if (path === "/dev/null") return null;
  if (path.startsWith('"') && path.endsWith('"')) path = path.slice(1, -1);
  return path.replace(/^[ab]\//, "");
}

/** The new path of a `diff --git a/x b/x` line, for a file with no `+++` line (a rename, a mode change, a binary). */
function pathOfHeader(line: string): string {
  const rest = line.slice("diff --git ".length);
  const half = (rest.length - 1) / 2;
  if (Number.isInteger(half) && rest[half] === " ") return rest.slice(half + 1).replace(/^b\//, "");
  const at = rest.lastIndexOf(" b/");
  return at >= 0 ? rest.slice(at + 3) : rest;
}

export function parseUnifiedDiff(text: string): FileDiff[] {
  const files: FileDiff[] = [];
  let file: FileDiff | null = null;
  let hunk: Hunk | null = null;
  let oldNumber = 0;
  let newNumber = 0;

  for (const line of text.split("\n")) {
    if (line.startsWith("diff --git ")) {
      file = {
        path: pathOfHeader(line),
        oldPath: null,
        status: "modified",
        binary: false,
        additions: 0,
        deletions: 0,
        hunks: [],
      };
      files.push(file);
      hunk = null;
      continue;
    }
    if (!file) continue;
    if (hunk === null) {
      if (line.startsWith("new file mode")) file.status = "added";
      else if (line.startsWith("deleted file mode")) file.status = "deleted";
      else if (line.startsWith("rename from ")) {
        file.status = "renamed";
        file.oldPath = line.slice("rename from ".length);
      } else if (line.startsWith("rename to ")) file.path = line.slice("rename to ".length);
      else if (line.startsWith("Binary files ") || line.startsWith("GIT binary patch")) {
        file.binary = true;
      } else if (line.startsWith("--- ")) {
        const old = pathOf(line);
        if (old === null) file.status = "added";
        else if (file.status === "deleted") file.path = old;
      } else if (line.startsWith("+++ ")) {
        const next = pathOf(line);
        if (next === null) file.status = "deleted";
        else file.path = next;
      }
    }
    const header = HUNK.exec(line);
    if (header) {
      hunk = { header: line, lines: [] };
      file.hunks.push(hunk);
      oldNumber = Number(header[1]);
      newNumber = Number(header[2]);
      continue;
    }
    if (!hunk) continue;
    const sign = line[0];
    const body = line.slice(1);
    if (sign === "+") {
      hunk.lines.push({ kind: "added", text: body, oldNumber: null, newNumber: newNumber++ });
      file.additions += 1;
    } else if (sign === "-") {
      hunk.lines.push({ kind: "removed", text: body, oldNumber: oldNumber++, newNumber: null });
      file.deletions += 1;
    } else if (sign === " ") {
      hunk.lines.push({
        kind: "context",
        text: body,
        oldNumber: oldNumber++,
        newNumber: newNumber++,
      });
    } else if (sign === "\\") {
      hunk.lines.push({ kind: "note", text: line, oldNumber: null, newNumber: null });
    }
  }
  return files;
}

/** How many lines the files add and remove in all. */
export function totals(files: FileDiff[]): { additions: number; deletions: number } {
  return files.reduce(
    (sum, file) => ({
      additions: sum.additions + file.additions,
      deletions: sum.deletions + file.deletions,
    }),
    { additions: 0, deletions: 0 },
  );
}

/** One row of the side-by-side view: the old file's line on the left, the new file's on the right; either may be empty. */
export interface SideRow {
  left: DiffLine | null;
  right: DiffLine | null;
}

/**
 * Lays the lines of a hunk out in two columns. A context line stands on both sides; a run of removed lines is set
 * against the run of added lines that follows it, line by line, and the longer run's rest stands against nothing.
 */
export function sideBySide(lines: DiffLine[]): SideRow[] {
  const rows: SideRow[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i] as DiffLine;
    if (line.kind === "context") {
      rows.push({ left: line, right: line });
      i += 1;
    } else if (line.kind === "note") {
      rows.push({ left: line, right: null });
      i += 1;
    } else {
      const removed: DiffLine[] = [];
      const added: DiffLine[] = [];
      while (lines[i]?.kind === "removed") removed.push(lines[i++] as DiffLine);
      while (lines[i]?.kind === "added") added.push(lines[i++] as DiffLine);
      for (let n = 0; n < Math.max(removed.length, added.length); n++) {
        rows.push({ left: removed[n] ?? null, right: added[n] ?? null });
      }
    }
  }
  return rows;
}
