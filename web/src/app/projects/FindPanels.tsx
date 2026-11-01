/**
 * "Go to file" and "Search in files" of the Files panel. Go to file ranks the paths of the default branch by what is
 * typed and opens the one chosen (Enter opens the best, the arrows move); Search in files runs `git grep` on the
 * server and lists the hits per file, and a click opens the file at the line.
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
import { Notice } from "../../ui/page/Notice";
import { rank } from "./fuzzy";

/** How many paths "Go to file" lists. */
const SHOWN = 12;

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
    <div
      role="dialog"
      aria-label="Go to file"
      className="flex flex-col gap-base rounded-lg border bg-panel p-base"
    >
      <div className="flex items-center gap-base">
        <Input
          aria-label="Go to file"
          placeholder="Go to file"
          value={query}
          autoFocus
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
        <Button size="sm" variant="outline" onClick={onClose}>
          Close
        </Button>
      </div>
      {paths.isPending && <p className="m-0 text-[12.5px] text-low">Reading the files…</p>}
      {paths.isError && <Notice tone="error">{paths.error.message}</Notice>}
      {paths.data && matches.length === 0 && (
        <p className="m-0 text-[12.5px] text-low">No file matches.</p>
      )}
      <ul className="m-0 flex list-none flex-col p-0">
        {matches.map((path, at) => (
          <li key={path}>
            <button
              type="button"
              data-selected={at === selected}
              onClick={() => open(path)}
              className={cn(
                "w-full truncate rounded-sm px-base py-half text-left font-mono text-[12.5px] hover:bg-card",
                at === selected ? "bg-card text-accent" : "text-normal",
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
  const hits = result.total === 1 ? "1 hit" : `${result.total} hits`;
  const files = result.files.length === 1 ? "1 file" : `${result.files.length} files`;
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
    <div
      role="dialog"
      aria-label="Search in files"
      className="flex flex-col gap-base rounded-lg border bg-panel p-base"
    >
      <div className="flex flex-wrap items-center gap-base">
        <Input
          aria-label="Search in files"
          placeholder="Search in files"
          value={query}
          autoFocus
          onChange={(e) => setQuery(e.target.value)}
          onCommandEnter={run}
          onKeyDown={(e) => e.key === "Escape" && onClose()}
          className="min-w-[200px] flex-1"
        />
        <label className="flex items-center gap-half text-xs text-normal">
          <input type="checkbox" checked={regex} onChange={(e) => setRegex(e.target.checked)} />
          Regex
        </label>
        <Button size="sm" disabled={query.trim() === "" || search.isPending} onClick={run}>
          Search
        </Button>
        <Button size="sm" variant="outline" onClick={onClose}>
          Close
        </Button>
      </div>
      {search.isPending && <p className="m-0 text-[12.5px] text-low">Searching.</p>}
      {search.isError && (
        <Notice tone="error">
          {isApiError(search.error) ? search.error.message : "The search failed."}
        </Notice>
      )}
      {search.data && (
        <>
          <p className="m-0 font-mono text-[12px] text-low">{hitsText(search.data)}</p>
          <ul className="m-0 flex max-h-[320px] list-none flex-col gap-base overflow-y-auto p-0">
            {search.data.files.map((file) => (
              <li key={file.path}>
                <div className="font-mono text-[12px] font-bold text-high">{file.path}</div>
                <ul className="m-0 list-none p-0">
                  {file.hits.map((hit) => (
                    <li key={hit.line}>
                      <button
                        type="button"
                        onClick={() => onOpen(file.path, hit.line)}
                        className="flex w-full gap-base rounded-sm px-base py-half text-left font-mono text-[12px] text-normal hover:bg-card"
                      >
                        <span className="w-10 shrink-0 text-right text-low">{hit.line}</span>
                        <span className="min-w-0 truncate">{hit.text}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
