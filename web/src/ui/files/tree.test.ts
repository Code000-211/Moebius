/**
 * The sidebar of a change's diff is built from the paths the diff names, with no server to ask, so the nesting has to
 * come out of the slashes. What matters is that it reads like the tree of the repository's own files: folders first,
 * names in order, one row for a folder many files share.
 */
import { describe, expect, it } from "vitest";
import { type PathNode, treeOf } from "./tree";

/** The list as it reads from the top down: a kind, a name, and whatever is under it. */
function shape(nodes: PathNode[]): string[] {
  return nodes.flatMap((node) => [`${node.kind} ${node.name}`, ...shape(node.children)]);
}

/** Every path in the list, top row first. */
function paths(nodes: PathNode[]): string[] {
  return nodes.flatMap((node) => [node.path, ...paths(node.children)]);
}

describe("a folder list built out of paths", () => {
  it("is nothing when there are no paths", () => {
    expect(treeOf([])).toEqual([]);
  });

  it("nests a path under the folders above it", () => {
    expect(shape(treeOf(["src/app/board/Dock.tsx"]))).toEqual([
      "dir src",
      "dir app",
      "dir board",
      "file Dock.tsx",
    ]);
  });

  it("has one row for a folder many files share", () => {
    const [src] = treeOf(["src/b.ts", "src/a.ts"]);
    expect(src?.children.map((each) => each.name)).toEqual(["a.ts", "b.ts"]);
  });

  it("puts the folders before the files and names each kind into order", () => {
    expect(shape(treeOf(["zest.md", "beta/b.md", "Alpha/a.md"]))).toEqual([
      "dir Alpha",
      "file a.md",
      "dir beta",
      "file b.md",
      "file zest.md",
    ]);
  });

  it("keeps the whole path on every row, since that is what a row opens", () => {
    expect(paths(treeOf(["src/lib.rs", "notes.txt"]))).toEqual(["src", "src/lib.rs", "notes.txt"]);
  });
});
