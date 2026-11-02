/**
 * A list of folders built out of the paths themselves, for a sidebar that already knows every path it holds — the
 * files a change touched — where the repository's tree, which the server reads one folder at a time, does not fit.
 * Folders come before files and both are named into order, which is how the server lists a folder too, so the two
 * kinds of list read alike.
 */

/** A name in the list, and the names under it when it is a folder. */
export interface PathNode {
  name: string;
  path: string;
  /** The same words the server's tree listing uses, so one row component serves both. */
  kind: "dir" | "file";
  children: PathNode[];
}

/** Folders before files, and each kind named into order. */
function inOrder(a: PathNode, b: PathNode): number {
  if (a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
  return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
}

/**
 * The list the paths make with every folder open: `src/lib.rs` and `notes.txt` are the folder `src` holding `lib.rs`,
 * then `notes.txt`. A path is a file and everything above it is a folder, which is all a sidebar of paths can know.
 */
export function treeOf(paths: readonly string[]): PathNode[] {
  const roots: PathNode[] = [];
  const folders: PathNode[] = [];
  for (const path of paths) {
    const parts = path.split("/");
    let siblings = roots;
    let here = "";
    for (const [at, name] of parts.entries()) {
      here = at === 0 ? name : `${here}/${name}`;
      const last = at === parts.length - 1;
      let node = siblings.find((each) => each.name === name);
      if (node === undefined) {
        node = { name, path: here, kind: last ? "file" : "dir", children: [] };
        siblings.push(node);
        if (!last) folders.push(node);
      }
      siblings = node.children;
    }
  }
  roots.sort(inOrder);
  for (const folder of folders) folder.children.sort(inOrder);
  return roots;
}
