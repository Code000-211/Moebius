/**
 * Two rules of `docs/design-web.md` that the source can be read for: a shortcut key (`<kbd>`) is shown only on the
 * Shortcuts page, and a list has no row of column headers. Both are read from the files of the app, not from what a
 * page draws today, so a new page breaks them as well as an old one; the scanning functions have a negative test each,
 * which feeds them source that breaks the rule.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = import.meta.dirname;

/** Every `.ts` and `.tsx` file under `dir` that is not a test, as paths from `src/` with `/`. */
function sources(dir: string = SRC): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sources(path);
    if (!/\.tsx?$/.test(entry.name) || /\.(test|e2e)\.tsx?$/.test(entry.name)) return [];
    return [relative(SRC, path).split(sep).join("/")];
  });
}

/** The files, among `files` (path to text), that draw a `<kbd>` and are not part of the Shortcuts page. */
function kbdOutsideShortcuts(files: Record<string, string>): string[] {
  return Object.entries(files)
    .filter(([path, text]) => !path.startsWith("app/shortcuts/") && /<kbd[\s>]/.test(text))
    .map(([path]) => path);
}

/** The files, among `files`, that draw rows with `ListRow` or `VirtualList` and also a row of column headers. */
function columnHeadersInLists(files: Record<string, string>): string[] {
  const list = /\b(VirtualList|ListRow)\b/;
  const header = /<th[\s>]|<thead[\s>]|role=["']columnheader["']|role=["']row["'][^>]*columnheader/;
  return Object.entries(files)
    .filter(([, text]) => list.test(text) && header.test(text))
    .map(([path]) => path);
}

const app = (): Record<string, string> =>
  Object.fromEntries(sources().map((path) => [path, readFileSync(join(SRC, path), "utf8")]));

describe("a shortcut key is shown only on the Shortcuts page", () => {
  it("is drawn nowhere else in the app", () => {
    expect(kbdOutsideShortcuts(app())).toEqual([]);
  });

  it("fails on a <kbd> in a menu", () => {
    expect(
      kbdOutsideShortcuts({
        "app/shell/Menu.tsx": "<span>Open</span><kbd className='x'>O</kbd>",
        "app/shortcuts/ShortcutsPage.tsx": "<kbd>O</kbd>",
      }),
    ).toEqual(["app/shell/Menu.tsx"]);
  });
});

describe("a list has no column headers", () => {
  it("has none in the files that draw rows of a list", () => {
    expect(columnHeadersInLists(app())).toEqual([]);
  });

  it("fails on a list with a header row", () => {
    expect(
      columnHeadersInLists({
        "app/x/X.tsx": "<VirtualList /><thead><tr><th>Name</th></tr></thead>",
        "app/x/Y.tsx": "<VirtualList />",
        "app/x/Z.tsx": '<div role="columnheader">Name</div><ListRow />',
        "app/x/Table.tsx": "<table><thead><th>Name</th></thead></table>",
      }),
    ).toEqual(["app/x/X.tsx", "app/x/Z.tsx"]);
  });
});
