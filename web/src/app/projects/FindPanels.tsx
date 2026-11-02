/**
 * The two search bars of the Files panel, both shown all the time: "Go to file" above the tree, "Search in files" above
 * the file being read. Neither waits for a button or a sheet — each works off what is typed, after a short pause, so a
 * search runs once per word rather than once per letter. Go to file ranks the paths of the default branch and lists the
 * best of them in a dropdown under the bar: the arrows move the choice, Enter or a click opens it, and the dropdown
 * closes when the caret leaves the bar or the file is chosen, so the tree below is never pushed about. Search
 * in files runs `git grep` on the server and lists the hits per file, and a click opens the file at the line.
 */
import { MagnifyingGlassIcon, XIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { type KeyboardEvent, type RefObject, useEffect, useMemo, useRef, useState } from "react";
import { filePaths, searchFiles } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Search } from "../../api/types/projects";
import { IconButton } from "../../ui/components/IconButton";
import { Input } from "../../ui/components/Input";
import { cn } from "../../ui/lib/cn";
import { plural } from "../../ui/lib/format";
import { Empty } from "../../ui/page/Empty";
import { Notice } from "../../ui/page/Notice";
import { rank } from "./fuzzy";

/** How many paths "Go to file" lists. */
const SHOWN = 12;

/** How long the typing waits before the search runs, in ms: a word arrives inside it, a letter does not. */
const SETTLE = 250;

/** `value` once it has not changed for `SETTLE`, so typing costs one search and not one per keystroke. */
function useSettled(value: string): string {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), SETTLE);
    return () => clearTimeout(timer);
  }, [value]);
  return settled;
}

/** A search bar: a field with a magnifier in front of it, and an X that empties it while there is something to empty. */
function SearchBar({
  label,
  value,
  onValue,
  onEnter,
  onKeyDown,
  onFocus,
  onBlur,
  fieldRef,
}: {
  label: string;
  value: string;
  onValue: (value: string) => void;
  /** Enter, for a search that is worth doing before the pause is over. */
  onEnter?: () => void;
  onKeyDown?: (e: KeyboardEvent<HTMLInputElement>) => void;
  onFocus?: () => void;
  /** The caret left the field; the argument is where it went, if anywhere. */
  onBlur?: (next: EventTarget | null) => void;
  /** The field, so the page's shortcut can put the caret in it. */
  fieldRef: RefObject<HTMLInputElement | null>;
}) {
  const clear = () => onValue("");
  return (
    <div className="relative flex min-w-0 flex-1 items-center">
      <MagnifyingGlassIcon
        className="pointer-events-none absolute left-3 size-4 text-muted"
        aria-hidden="true"
      />
      <Input
        ref={fieldRef}
        aria-label={label}
        placeholder={label}
        value={value}
        className={cn("min-w-0 flex-1 pl-9", value !== "" && "pr-10")}
        onChange={(e) => onValue(e.target.value)}
        onFocus={onFocus}
        onBlur={(e) => onBlur?.(e.relatedTarget)}
        onCommandEnter={onEnter}
        onKeyDown={(e) => {
          if (e.key === "Escape") clear();
          onKeyDown?.(e);
        }}
      />
      {value !== "" && (
        <IconButton
          icon={XIcon}
          aria-label={`Clear ${label.toLowerCase()}`}
          className="absolute right-0"
          onClick={clear}
        />
      )}
    </div>
  );
}

export function GoToFile({
  repository,
  onOpen,
  fieldRef,
}: {
  repository: number;
  onOpen: (path: string) => void;
  fieldRef: RefObject<HTMLInputElement | null>;
}) {
  const paths = useQuery({
    queryKey: keys.filePaths(repository),
    queryFn: () => filePaths(repository),
  });
  const [query, setQuery] = useState("");
  const [focused, setFocused] = useState(false);
  const dropdown = useRef<HTMLDivElement>(null);
  const settled = useSettled(query);
  const [selected, setSelected] = useState(0);
  const matches = useMemo(() => rank(settled, paths.data ?? [], SHOWN), [settled, paths.data]);
  // The bar has something in it and the typing has stopped: until it stops, whatever the list would say is already
  // stale. The read of the names is answered even inside the pause, because a list with nothing in it looks the same
  // as a bar that was never typed in.
  const listing =
    focused && query.trim() !== "" && (settled === query || paths.isPending || paths.isError);

  const open = (path: string | undefined) => {
    if (path === undefined) return;
    // The file is chosen, so the filter has done its job and the tree comes back.
    setQuery("");
    setSelected(0);
    onOpen(path);
  };
  /** Enter opens the best of what is typed now, even before the pause is over. */
  const openBest = () => {
    const list = settled === query ? matches : rank(query, paths.data ?? [], SHOWN);
    open(list[selected] ?? list[0]);
  };

  return (
    <div className="relative flex min-w-0 flex-col">
      <SearchBar
        label="Go to file"
        onFocus={() => setFocused(true)}
        onBlur={(next) => {
          // The dropdown is the bar's, so it goes when the caret leaves both of them. Its rows are buttons, so a click
          // on one moves the caret into the dropdown rather than out of it.
          if (!dropdown.current?.contains(next as Node | null)) setFocused(false);
        }}
        value={query}
        fieldRef={fieldRef}
        onValue={(value) => {
          setQuery(value);
          setSelected(0);
        }}
        onEnter={openBest}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setSelected((at) => Math.min(at + 1, matches.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setSelected((at) => Math.max(at - 1, 0));
          }
        }}
      />
      {/* The list hangs under the bar rather than pushing the tree down: the tree is where the file just found has to
          be seen, and a list that moved it would hide the thing it is answering. */}
      {listing && (
        <div
          ref={dropdown}
          className="absolute top-full left-0 z-menu mt-2 w-full rounded-card bg-surface p-1 text-text shadow-raised"
        >
          {paths.isPending && (
            <p className="m-0 px-3 py-2 text-sm text-secondary" role="status">
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
          {!paths.isPending && !paths.isError && matches.length === 0 && (
            <p className="m-0 px-3 py-2 text-sm text-secondary">
              No file matches. Try a few letters of the name or the folder, in order.
            </p>
          )}
          {matches.length > 0 && (
            <ul
              aria-label="Matching files"
              className="m-0 flex max-h-64 list-none flex-col gap-1 overflow-y-auto p-0"
            >
              {matches.map((path, at) => {
                const slash = path.lastIndexOf("/");
                const name = slash === -1 ? path : path.slice(slash + 1);
                const folder = slash === -1 ? "" : `${path.slice(0, slash)}/`;
                return (
                  <li key={path}>
                    <button
                      type="button"
                      aria-label={path}
                      aria-current={at === selected ? "true" : undefined}
                      onClick={() => open(path)}
                      title={path}
                      className={cn(
                        "flex min-h-10 w-full items-center rounded-control px-3 py-2 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover",
                        at === selected ? "bg-selected text-text" : "text-secondary",
                      )}
                    >
                      {/* The folder gives way and the name does not: the row is chosen by its name. */}
                      {folder !== "" && (
                        <span className="shrink truncate text-sm text-secondary">{folder}</span>
                      )}
                      <span className="min-w-0 break-all">{name}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

/** "3 hits in 2 files". */
function hitsText(result: Search): string {
  const hits = plural(result.total, "hit", "hits");
  const files = plural(result.files.length, "file", "files");
  return `${hits} in ${files}${result.truncated ? " (more than shown)" : ""}`;
}

export function SearchInFiles({
  repository,
  onOpen,
  fieldRef,
}: {
  repository: number;
  onOpen: (path: string, line: number) => void;
  fieldRef: RefObject<HTMLInputElement | null>;
}) {
  const [query, setQuery] = useState("");
  const [regex, setRegex] = useState(false);
  const term = useSettled(query).trim();
  const search = useQuery({
    queryKey: keys.fileSearch(repository, term, regex),
    queryFn: () => searchFiles(repository, term, regex),
    // Nothing was typed yet, so there is nothing to ask the server for.
    enabled: term !== "",
    // The hits of the search before this one stay until the new ones arrive.
    placeholderData: (previous) => previous,
  });
  const looking = term !== "";

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <SearchBar label="Search in files" value={query} fieldRef={fieldRef} onValue={setQuery} />
        <label className="flex min-h-10 cursor-pointer items-center gap-2 rounded-control px-3 text-sm text-secondary hover:bg-hover">
          <input
            type="checkbox"
            className="size-4 accent-current"
            checked={regex}
            onChange={(e) => setRegex(e.target.checked)}
          />
          Regex
        </label>
      </div>
      {looking && search.isFetching && (
        <p className="m-0 text-sm text-secondary" role="status">
          Reading the matches
        </p>
      )}
      {looking && search.isError && (
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
      {looking && search.data !== undefined && (
        <>
          <p className="m-0 text-sm text-secondary tabular-nums">{hitsText(search.data)}</p>
          {search.data.files.length === 0 ? (
            <Empty
              plain
              title="No match"
              body={`Nothing on the default branch contains ${term}. Try fewer letters, or search without Regex.`}
            />
          ) : (
            <ul
              aria-label="Search hits"
              className="m-0 flex max-h-64 list-none flex-col gap-4 overflow-y-auto p-0"
            >
              {search.data.files.map((file) => (
                <li key={file.path} className="flex flex-col gap-1">
                  <p className="m-0 break-all text-sm font-semibold text-text" title={file.path}>
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
