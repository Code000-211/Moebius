/**
 * The search box above the wiki's tree and what it finds (`GET /v1/wiki/search`): the pages whose title, labels or text have the
 * words typed, the best first, each with its name, its folder and the line around the match. While something is typed
 * the matches stand in the place of the tree; a click opens the page and clears the box. The query is sent a moment
 * after the last key, so typing does not ask the server for every letter.
 */
import { MagnifyingGlassIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { searchWiki } from "../../api/calls/wiki";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { Input } from "../../ui/components/Input";
import { SkeletonList } from "../../ui/page/Skeleton";
import { useToast } from "../../ui/page/Toast";

/** How long the box waits after a key before it asks the server. */
const WAIT_MS = 250;

/** The words typed, once they have stood still for a moment. */
export function useSettled(value: string, wait = WAIT_MS): string {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), wait);
    return () => clearTimeout(timer);
  }, [value, wait]);
  return settled;
}

export function WikiSearchBox({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="relative">
      <MagnifyingGlassIcon
        className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted"
        aria-hidden
      />
      <Input
        type="search"
        aria-label="Search the wiki"
        placeholder="Search pages"
        autoComplete="off"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="pl-9"
      />
    </div>
  );
}

export function WikiSearchResults({
  query,
  project,
  onOpen,
}: {
  query: string;
  /** A project's key, to find only its pages. */
  project?: string;
  onOpen: (path: string) => void;
}) {
  const toast = useToast();
  const settled = useSettled(query.trim());
  const found = useQuery({
    queryKey: keys.wikiSearch(settled, project),
    queryFn: () => searchWiki(settled, project),
    enabled: settled !== "",
    retry: false,
  });
  const failed = found.error;
  useEffect(() => {
    if (failed) toast({ kind: "error", text: sayError(failed, "Could not search the wiki.") });
  }, [failed, toast]);
  if (found.isError) return null;
  if (settled !== query.trim() || found.isPending) {
    return <SkeletonList rows={3} message="Searching the wiki" />;
  }
  if (found.data.length === 0) {
    return <p className="m-0 px-3 text-sm text-muted">No page matches “{settled}”.</p>;
  }
  return (
    <nav aria-label="Search results" className="flex flex-col gap-0.5">
      <p role="status" className="m-0 px-3 pb-1 text-xs font-medium text-muted">
        {found.data.length === 1 ? "1 page" : `${found.data.length} pages`}
      </p>
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight takes the bullets off, and WebKit then drops the list */}
      <ul role="list" className="m-0 flex flex-col p-0">
        {found.data.map((hit) => (
          <li key={hit.path}>
            <button
              type="button"
              onClick={() => onOpen(hit.path)}
              className="flex min-h-9 w-full cursor-pointer flex-col gap-0.5 rounded-control bg-transparent px-3 py-1.5 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover"
            >
              <span className="truncate text-base text-text">{hit.name}</span>
              <span className="truncate text-xs text-muted">
                {hit.project ? `Project ${hit.project}` : "Organisation"}
                {hit.path !== hit.name && ` · ${hit.path}`}
              </span>
              {hit.snippet !== "" && (
                <span className="line-clamp-2 text-sm text-muted">{hit.snippet}</span>
              )}
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}
