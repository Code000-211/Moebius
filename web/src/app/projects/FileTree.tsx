/**
 * The Files tree of a repository's default branch: a directory is read when it is first opened, a click on a file opens
 * it. Rows are 40px tall and stand at body size, so the tree reads as a list of names rather than a wall of code; the
 * name alone is the row's label, and the path it stands for is its tooltip.
 */
import { CaretDownIcon, CaretRightIcon, FileIcon, FolderIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { tree } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { cn } from "../../ui/lib/cn";

interface TreeProps {
  repository: number;
  open: ReadonlySet<string>;
  current: string | null;
  onToggle: (directory: string) => void;
  onOpen: (path: string) => void;
}

/** How far a row of depth `depth` stands in, on the 8pt grid. */
const indent = (depth: number) => ({ paddingLeft: `${8 + depth * 16}px` });

const ROW =
  "flex min-h-10 w-full items-center gap-2 rounded-control pr-2 text-left transition-colors duration-(--motion-routine) ease-out";

export function FileTree(props: TreeProps) {
  return (
    <ul className="m-0 list-none p-0 text-base" aria-label="Files">
      <Directory {...props} path="" depth={0} />
    </ul>
  );
}

function Directory({ path, depth, ...props }: TreeProps & { path: string; depth: number }) {
  const { repository, open, current, onToggle, onOpen } = props;
  const listing = useQuery({
    queryKey: keys.tree(repository, path),
    queryFn: () => tree(repository, path),
  });
  if (listing.isPending) {
    return (
      <li className="min-h-10 py-2 pr-2 text-sm text-secondary" style={indent(depth)}>
        Reading
      </li>
    );
  }
  if (listing.isError) {
    return (
      <li
        role="alert"
        className="min-h-10 break-words py-2 pr-2 text-sm text-error-text"
        style={indent(depth)}
      >
        <span className="font-semibold">Couldn't read this folder. </span>
        {listing.error.message}
      </li>
    );
  }
  if (listing.data.entries.length === 0) {
    return (
      <li className="min-h-10 py-2 pr-2 text-sm text-secondary" style={indent(depth)}>
        No files in this folder.
      </li>
    );
  }
  return (
    <>
      {listing.data.entries.map((entry) => {
        if (entry.kind === "file") {
          return (
            <li key={entry.path}>
              <button
                type="button"
                style={indent(depth)}
                title={entry.path}
                onClick={() => onOpen(entry.path)}
                className={cn(
                  ROW,
                  "hover:bg-hover",
                  current === entry.path ? "bg-selected text-text" : "text-secondary",
                )}
              >
                <FileIcon className="size-4 shrink-0" aria-hidden="true" />
                <span className="break-words">{entry.name}</span>
              </button>
            </li>
          );
        }
        const isOpen = open.has(entry.path);
        return (
          <li key={entry.path}>
            <button
              type="button"
              style={indent(depth)}
              title={entry.path}
              aria-expanded={isOpen}
              onClick={() => onToggle(entry.path)}
              className={cn(ROW, "text-secondary hover:bg-hover")}
            >
              {isOpen ? (
                <CaretDownIcon className="size-4 shrink-0" aria-hidden="true" />
              ) : (
                <CaretRightIcon className="size-4 shrink-0" aria-hidden="true" />
              )}
              <FolderIcon className="size-4 shrink-0" aria-hidden="true" />
              <span className="break-words">{entry.name}</span>
            </button>
            {isOpen && (
              <ul className="m-0 list-none p-0">
                <Directory {...props} path={entry.path} depth={depth + 1} />
              </ul>
            )}
          </li>
        );
      })}
    </>
  );
}
