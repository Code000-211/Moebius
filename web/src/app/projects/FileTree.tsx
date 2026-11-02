/**
 * The Files tree of a repository's default branch: a folder is read when it is first opened, a click on a file opens
 * it. The rows themselves are the ones every list of files wears (`ui/files/TreeRows.tsx`); all of this is where the
 * names come from — the server, one folder at a time, so a huge repository costs nothing until a folder is opened.
 */
import { useQuery } from "@tanstack/react-query";
import { tree } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { FileRow, FolderRow, indent } from "../../ui/files/TreeRows";

interface TreeProps {
  repository: number;
  open: ReadonlySet<string>;
  current: string | null;
  onToggle: (directory: string) => void;
  onOpen: (path: string) => void;
}

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
              <FileRow
                name={entry.name}
                path={entry.path}
                depth={depth}
                chosen={current === entry.path}
                onOpen={onOpen}
              />
            </li>
          );
        }
        const isOpen = open.has(entry.path);
        return (
          <li key={entry.path}>
            <FolderRow
              name={entry.name}
              path={entry.path}
              depth={depth}
              open={isOpen}
              onToggle={onToggle}
            />
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
