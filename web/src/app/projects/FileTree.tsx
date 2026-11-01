/** The Files tree of a repository's default branch: a directory is read when it is first opened, a click on a file opens it. */
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

export function FileTree(props: TreeProps) {
  return (
    <ul className="m-0 list-none p-0 text-[13px]" aria-label="Files">
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
    return <li className="px-base py-half text-low">Reading…</li>;
  }
  if (listing.isError) {
    return (
      <li role="alert" className="px-base py-half text-error">
        {listing.error.message}
      </li>
    );
  }
  if (listing.data.entries.length === 0) {
    return <li className="px-base py-half text-low">Empty.</li>;
  }
  return (
    <>
      {listing.data.entries.map((entry) => {
        const indent = { paddingLeft: 8 + depth * 14 };
        if (entry.kind === "file") {
          return (
            <li key={entry.path}>
              <button
                type="button"
                style={indent}
                onClick={() => onOpen(entry.path)}
                className={cn(
                  "flex w-full items-center gap-half rounded-sm py-half pr-base text-left hover:bg-card",
                  current === entry.path ? "bg-card text-accent" : "text-normal",
                )}
              >
                <FileIcon className="size-icon-xs shrink-0" />
                <span className="truncate">{entry.name}</span>
              </button>
            </li>
          );
        }
        const isOpen = open.has(entry.path);
        return (
          <li key={entry.path}>
            <button
              type="button"
              style={indent}
              aria-expanded={isOpen}
              onClick={() => onToggle(entry.path)}
              className="flex w-full items-center gap-half rounded-sm py-half pr-base text-left text-normal hover:bg-card"
            >
              {isOpen ? (
                <CaretDownIcon className="size-icon-xs shrink-0" />
              ) : (
                <CaretRightIcon className="size-icon-xs shrink-0" />
              )}
              <FolderIcon className="size-icon-xs shrink-0" />
              <span className="truncate">{entry.name}</span>
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
