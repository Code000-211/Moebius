/**
 * The tree at the left of the Wiki page: the organisation's pages, then a folder for each project under its name, each
 * folder (a category of its pages, with how many it holds) opened or closed with a click, and "New page" at the foot of
 * a group to write one there. The chosen page is marked and the folders above it are open, so that a page reached by its
 * address shows where it sits; while labels filter the tree every category that is left is open. The project's Wiki tab
 * draws only its own group (`organisation={false}`).
 */
import { CaretDownIcon, CaretRightIcon, FileTextIcon, PlusIcon } from "@phosphor-icons/react";
import { useState } from "react";
import { cn } from "../../ui/lib/cn";
import { foldersOf, PROJECTS, type WikiTree as Tree, type TreeNode } from "./model";

const NO_MATCH = "No page has these labels.";

export function WikiTree({
  tree,
  chosen,
  canWrite,
  organisation = true,
  filtered = false,
  onChoose,
  onNew,
}: {
  tree: Tree;
  chosen: string | null;
  canWrite: boolean;
  /** Whether the organisation's own group is drawn. */
  organisation?: boolean;
  /** Whether labels narrow the tree, which opens its categories and changes what an empty group says. */
  filtered?: boolean;
  onChoose: (path: string) => void;
  /** Starts a new page in the folder, empty at the top. */
  onNew: (folder: string) => void;
}) {
  // A folder is open when it was opened, or when the chosen page is in it.
  const [toggled, setToggled] = useState<Record<string, boolean>>({});
  const above = new Set(chosen ? foldersOf(chosen) : []);
  const isOpen = (path: string) => toggled[path] ?? (filtered || above.has(path));
  const toggle = (path: string) => setToggled((now) => ({ ...now, [path]: !isOpen(path) }));
  const rows = (nodes: TreeNode[], depth: number) =>
    nodes.map((node) => (
      <Row
        key={node.path}
        node={node}
        depth={depth}
        chosen={chosen}
        isOpen={isOpen}
        onToggle={toggle}
        onChoose={onChoose}
      />
    ));
  return (
    <nav aria-label="Pages of the wiki" className="flex flex-col gap-3">
      {organisation && (
        <Group label="Organisation" onNew={canWrite ? () => onNew("") : undefined}>
          {tree.organisation.length === 0 ? (
            <p className="m-0 px-3 text-sm text-muted">{filtered ? NO_MATCH : "No page yet."}</p>
          ) : (
            rows(tree.organisation, 0)
          )}
        </Group>
      )}
      {tree.projects.map((project) => {
        const folder = `${PROJECTS}/${project.key}`;
        return (
          <Group
            key={project.key}
            label={project.name}
            onNew={canWrite ? () => onNew(folder) : undefined}
          >
            {project.nodes.length === 0 ? (
              <p className="m-0 px-3 text-sm text-muted">{filtered ? NO_MATCH : "No page yet."}</p>
            ) : (
              rows(project.nodes, 0)
            )}
          </Group>
        );
      })}
    </nav>
  );
}

/** A group of the tree: a quiet row with its name, its pages, and "New page" at its foot. */
function Group({
  label,
  onNew,
  children,
}: {
  label: string;
  onNew?: () => void;
  children: React.ReactNode;
}) {
  return (
    <section aria-label={label} className="flex flex-col">
      <h2 className="m-0 flex h-[30px] items-center truncate px-3 text-sm font-medium text-muted">
        {label}
      </h2>
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight takes the bullets off, and WebKit then drops the list */}
      <ul role="list" className="m-0 flex flex-col p-0">
        {children}
      </ul>
      {onNew && (
        <button
          type="button"
          onClick={onNew}
          aria-label={`New page in ${label}`}
          className="flex min-h-9 cursor-pointer items-center gap-2 rounded-control bg-transparent px-3 text-left text-base text-muted transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
        >
          <PlusIcon className="size-[15px]" aria-hidden />
          New page
        </button>
      )}
    </section>
  );
}

function Row({
  node,
  depth,
  chosen,
  isOpen,
  onToggle,
  onChoose,
}: {
  node: TreeNode;
  depth: number;
  chosen: string | null;
  isOpen: (path: string) => boolean;
  onToggle: (path: string) => void;
  onChoose: (path: string) => void;
}) {
  const indent = { paddingLeft: `${0.75 + depth * 1}rem` };
  const row =
    "relative flex min-h-9 w-full cursor-pointer items-center gap-2 rounded-control pr-3 text-left text-base transition-colors duration-(--motion-routine) ease-out";
  if (node.kind === "page") {
    const selected = chosen === node.path;
    return (
      <li className="flex flex-col">
        <button
          type="button"
          style={indent}
          aria-current={selected ? "page" : undefined}
          onClick={() => onChoose(node.path)}
          className={cn(
            row,
            selected
              ? "bg-selected font-medium text-text before:absolute before:inset-y-1 before:left-0 before:w-0.5 before:rounded-full before:bg-accent"
              : "bg-transparent text-text hover:bg-hover",
          )}
        >
          <FileTextIcon className="size-[15px] shrink-0 text-muted" aria-hidden />
          <span className="min-w-0 truncate">{node.name}</span>
        </button>
      </li>
    );
  }
  const open = isOpen(node.path);
  const Caret = open ? CaretDownIcon : CaretRightIcon;
  return (
    <li className="flex flex-col">
      <button
        type="button"
        style={indent}
        aria-expanded={open}
        onClick={() => onToggle(node.path)}
        className={cn(row, "bg-transparent text-text hover:bg-hover")}
      >
        <Caret className="size-[15px] shrink-0 text-muted" aria-hidden />
        <span className="min-w-0 truncate">{node.name}</span>
        <span className="ml-auto shrink-0 text-xs tabular-nums text-muted">{node.count}</span>
      </button>
      {open && (
        // biome-ignore lint/a11y/noRedundantRoles: preflight takes the bullets off, and WebKit then drops the list
        <ul role="list" className="m-0 flex flex-col p-0">
          {node.children.map((child) => (
            <Row
              key={child.path}
              node={child}
              depth={depth + 1}
              chosen={chosen}
              isOpen={isOpen}
              onToggle={onToggle}
              onChoose={onChoose}
            />
          ))}
        </ul>
      )}
    </li>
  );
}
