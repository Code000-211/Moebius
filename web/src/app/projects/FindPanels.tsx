/**
 * "Go to file" and "Search in files" of the Files panel. Go to file ranks the paths of the default branch by what is
 * typed and opens the one chosen (Enter opens the best, the arrows move); Search in files runs `git grep` on the server
 * and lists the hits per file, and a click opens the file at the line. Both are sheets over the editor: they float on a
 * raised layer, their rows are 40px, and a search that finds nothing says so and what to try next.
 */
import { useMutation, useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { filePaths, searchFiles } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Search } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { cn } from "../../ui/lib/cn";
import { plural } from "../../ui/lib/format";
import { Empty } from "../../ui/page/Empty";
import { Notice } from "../../ui/page/Notice";
import { rank } from "./fuzzy";

/** How many paths "Go to file" lists. */
const SHOWN = 12;

/** The layer a find panel sits on: raised above the card it opens over, and borderless. */
const SHEET = "flex flex-col gap-4 rounded-card bg-surface p-4 shadow-raised";

/** The line a find panel gives when there is nothing to list yet. */
function Nothing({ title, hint }: { title: string; hint: string }) {
  return (
    <div className="rounded-control bg-sunken p-3">
      <p className="m-0 text-sm font-semibold text-text">{title}</p>
      <p className="m-0 mt-1 text-sm text-secondary">{hint}</p>
    </div>
  );
}

export function GoToFile({
  repository,
  onOpen,
  onClose,
}: {
  repository: number;
  onOpen: (path: string) => void;
  onClose: () => void;
}) {
  const paths = useQuery({
    queryKey: keys.filePaths(repository),
    queryFn: () => filePaths(repository),
  });
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const matches = useMemo(() => rank(query, paths.data ?? [], SHOWN), [query, paths.data]);
  const open = (path: string | undefined) => {
    if (path !== undefined) onOpen(path);
  };
  return (
    <div role="dialog" aria-label="Go to file" className={SHEET}>
      <div className="flex flex-wrap items-center gap-2">
        <Input
          aria-label="Go to file"
          placeholder="Go to file"
          value={query}
          autoFocus
          mono
          className="min-w-48 flex-1"
          onChange={(e) => {
            setQuery(e.target.value);
            setSelected(0);
          }}
          onCommandEnter={() => open(matches[selected])}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSelected((at) => Math.min(at + 1, matches.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setSelected((at) => Math.max(at - 1, 0));
            } else if (e.key === "Escape") {
              onClose();
            }
          }}
        />
        <Button size="sm" variant="secondary" onClick={onClose}>
          Close
        </Button>
      </div>
      {paths.isPending && (
        <p className="m-0 text-sm text-secondary" role="status">
          Reading the file names
        </p>
      )}
      {paths.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't read the file names. </span>
          {isApiError(paths.error)
            ? paths.error.message
            : "The server did not answer for this repository."}
        </Notice>
      )}
      {paths.data && matches.length === 0 && (
        <Nothing
          title="No file matches"
          hint="Type a few letters of the name or the folder, in order."
        />
      )}
      <ul className="m-0 flex max-h-80 list-none flex-col gap-1 p-0">
        {matches.map((path, at) => (
          <li key={path}>
            <button
              type="button"
              aria-current={at === selected ? "true" : undefined}
              onClick={() => open(path)}
              title={path}
              className={cn(
                "min-h-10 w-full break-all rounded-control px-3 py-2 text-left font-mono text-sm transition-colors duration-(--motion-routine) ease-out hover:bg-hover",
                at === selected ? "bg-selected text-text" : "text-secondary",
              )}
            >
              {path}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** "3 hits in 2 files". */
export function hitsText(result: Search): string {
  const hits = plural(result.total, "hit", "hits");
  const files = plural(result.files.length, "file", "files");
  return `${hits} in ${files}${result.truncated ? " (more than shown)" : ""}`;
}

export function SearchInFiles({
  repository,
  onOpen,
  onClose,
}: {
  repository: number;
  onOpen: (path: string, line: number) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [regex, setRegex] = useState(false);
  const search = useMutation({
    mutationFn: () => searchFiles(repository, query.trim(), regex),
  });
  const run = () => {
    if (query.trim() !== "" && !search.isPending) search.mutate();
  };
  return (
    <div role="dialog" aria-label="Search in files" className={SHEET}>
      <div className="flex flex-wrap items-center gap-2">
        <Input
          aria-label="Search in files"
          placeholder="Search in files"
          value={query}
          autoFocus
          className="min-w-48 flex-1"
          onChange={(e) => setQuery(e.target.value)}
          onCommandEnter={run}
          onKeyDown={(e) => e.key === "Escape" && onClose()}
        />
        <label className="flex min-h-10 cursor-pointer items-center gap-2 rounded-control px-3 text-sm text-secondary hover:bg-hover">
          <input
            type="checkbox"
            className="size-4 accent-current"
            checked={regex}
            onChange={(e) => setRegex(e.target.checked)}
          />
          Regex
        </label>
        <Button size="sm" disabled={query.trim() === "" || search.isPending} onClick={run}>
          Search
        </Button>
        <Button size="sm" variant="secondary" onClick={onClose}>
          Close
        </Button>
      </div>
      {search.isPending && (
        <p className="m-0 text-sm text-secondary" role="status">
          Reading the matches
        </p>
      )}
      {search.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't search the files. </span>
          <span className="break-words">
            {isApiError(search.error)
              ? search.error.message
              : "The server did not answer for this search."}
            {regex && " Check the regular expression, or search without Regex."}
          </span>
        </Notice>
      )}
      {search.data && (
        <>
          <p className="m-0 text-sm text-secondary tabular-nums">{hitsText(search.data)}</p>
          {search.data.files.length === 0 ? (
            <Empty
              plain
              title="No match"
              body={`Nothing on the default branch contains ${query.trim()}. Try fewer letters, or search without Regex.`}
            />
          ) : (
            <ul className="m-0 flex max-h-80 list-none flex-col gap-4 overflow-y-auto p-0">
              {search.data.files.map((file) => (
                <li key={file.path} className="flex flex-col gap-1">
                  <p
                    className="m-0 break-all font-mono text-sm font-semibold text-text"
                    title={file.path}
                  >
                    {file.path}
                  </p>
                  <ul className="m-0 list-none p-0">
                    {file.hits.map((hit) => (
                      <li key={hit.line}>
                        <button
                          type="button"
                          onClick={() => onOpen(file.path, hit.line)}
                          className="flex min-h-10 w-full items-start gap-2 rounded-control px-3 py-2 text-left font-mono text-sm text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover"
                        >
                          <span className="w-10 shrink-0 text-right text-secondary tabular-nums">
                            {hit.line}
                          </span>
                          <span className="min-w-0 break-words whitespace-pre-wrap text-text">
                            {hit.text}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </div>
  );
}
