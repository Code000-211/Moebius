/**
 * What the Wiki page works out from the pages: the tree of the organisation's pages and one folder per project (the
 * server keeps a project's pages in `projects/<key>/`), the headings a page's contents list, and whether a path a
 * person typed is a page's. The rules of a path are the server's (`docs/reference/desktop-api.md`, "The wiki"); the
 * server checks them again.
 */
import type { PageInfo } from "../../api/types/wiki";

/** The folder that holds the projects' pages. */
export const PROJECTS = "projects";

/** A row of the tree: a folder with what is in it, or a page. */
export type TreeNode =
  | { kind: "folder"; name: string; path: string; children: TreeNode[] }
  | { kind: "page"; name: string; path: string };

/** The tree of the wiki: the organisation's pages, then one folder per project, a project without pages too. */
export interface WikiTree {
  organisation: TreeNode[];
  projects: Array<{ key: string; name: string; nodes: TreeNode[] }>;
}

/** The tree of `pages`, with a folder for each of `projects`; folders first, then pages, each by name. */
export function buildTree(
  pages: readonly PageInfo[],
  projects: ReadonlyArray<{ key: string; name: string }>,
): WikiTree {
  const organisation: TreeNode[] = [];
  const byKey = new Map<string, TreeNode[]>(projects.map((project) => [project.key, []]));
  for (const page of pages) {
    const parts = page.path.split("/");
    if (parts[0] === PROJECTS && parts.length > 2) {
      const key = parts[1] ?? "";
      let nodes = byKey.get(key);
      if (!nodes) {
        // A folder of a key no project has now (a project that was removed); its pages stay readable.
        nodes = [];
        byKey.set(key, nodes);
      }
      insert(nodes, parts.slice(2), `${PROJECTS}/${key}`);
    } else {
      insert(organisation, parts, "");
    }
  }
  const named = new Map(projects.map((project) => [project.key, project.name]));
  return {
    organisation: sorted(organisation),
    projects: [...byKey.entries()].map(([key, nodes]) => ({
      key,
      name: named.get(key) ?? key,
      nodes: sorted(nodes),
    })),
  };
}

function insert(nodes: TreeNode[], parts: string[], base: string): void {
  const [first, ...rest] = parts;
  if (first === undefined) return;
  const path = base === "" ? first : `${base}/${first}`;
  if (rest.length === 0) {
    nodes.push({ kind: "page", name: first, path });
    return;
  }
  let folder = nodes.find(
    (node): node is Extract<TreeNode, { kind: "folder" }> =>
      node.kind === "folder" && node.name === first,
  );
  if (!folder) {
    folder = { kind: "folder", name: first, path, children: [] };
    nodes.push(folder);
  }
  insert(folder.children, rest, path);
}

function sorted(nodes: TreeNode[]): TreeNode[] {
  return nodes
    .map((node) => (node.kind === "folder" ? { ...node, children: sorted(node.children) } : node))
    .sort((a, b) =>
      a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === "folder" ? -1 : 1,
    );
}

/** The folders above `path`, the outermost first: what is opened in the tree so that the page shows. */
export function foldersOf(path: string): string[] {
  const parts = path.split("/");
  return parts.slice(0, -1).map((_, index) => parts.slice(0, index + 1).join("/"));
}

/** The folder a page is in, empty at the top. */
export function folderOf(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut < 0 ? "" : path.slice(0, cut);
}

/** A heading of a page, for its contents. */
export interface Heading {
  level: number;
  text: string;
}

/** The headings of levels 1 to 3 in `text`, in order, leaving out what is inside fenced code. */
export function headings(text: string): Heading[] {
  const found: Heading[] = [];
  let fence: string | null = null;
  for (const line of text.split("\n")) {
    const opened = /^\s{0,3}(`{3,}|~{3,})/.exec(line);
    if (opened) {
      const mark = opened[1] ?? "";
      if (fence === null) fence = mark[0] ?? null;
      else if (mark[0] === fence) fence = null;
      continue;
    }
    if (fence !== null) continue;
    const heading = /^\s{0,3}(#{1,3})\s+(.*?)\s*#*\s*$/.exec(line);
    if (heading?.[2]) {
      found.push({ level: heading[1]?.length ?? 1, text: heading[2].replace(/[*_`]/g, "") });
    }
  }
  return found;
}

const REFUSED = /[\\:*?"<>|]/;

/** Why `path` is not a page's path, or null when it is; under `projects/`, `keys` are the keys a page may be under. */
export function pathProblem(path: string, keys: readonly string[]): string | null {
  if (path === "") return "Give the page a name.";
  const parts = path.split("/");
  for (const part of parts) {
    if (part === "") return "A folder or name is empty.";
    if (part.startsWith(".")) return "A name cannot start with a dot.";
    if (part.trim() !== part) return "A name cannot start or end with a space.";
    if (new TextEncoder().encode(part).length > 120) return "A name is longer than 120 bytes.";
    // biome-ignore lint/suspicious/noControlCharactersInRegex: control characters are what is refused here
    if (REFUSED.test(part) || /[\u0000-\u001f\u007f]/.test(part)) {
      return 'A name cannot hold \\ : * ? " < > | or a control character.';
    }
  }
  if (parts[0] === PROJECTS && parts.length > 1) {
    if (parts.length < 3 || !keys.includes(parts[1] ?? "")) {
      return "A project's pages go in projects/<key>/, the folder of a project's key.";
    }
  }
  return null;
}
