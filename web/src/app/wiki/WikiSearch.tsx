/**
 * The search box above the wiki's tree and what it finds (`GET /v1/wiki/search`): the pages whose title or text has the
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
import { Notice } from "../../ui/page/Notice";
import { SkeletonList } from "../../ui/page/Skeleton";

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
        className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-secondary"
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
  onOpen,
}: {
  query: string;
  onOpen: (path: string) => void;
}) {
  const settled = useSettled(query.trim());
  const found = useQuery({
    queryKey: keys.wikiSearch(settled),
    queryFn: () => searchWiki(settled),
    enabled: settled !== "",
    retry: false,
  });
  if (found.isError) {
    return <Notice tone="error">{sayError(found.error, "Could not search the wiki.")}</Notice>;
  }
  if (settled !== query.trim() || found.isPending) {
    return <SkeletonList rows={3} message="Searching the wiki" />;
  }
  if (found.data.length === 0) {
    return <p className="m-0 px-3 text-sm text-secondary">No page matches “{settled}”.</p>;
  }
  return (
    <nav aria-label="Search results" className="flex flex-col gap-0.5">
      <p role="status" className="m-0 px-3 pb-1 text-xs font-semibold text-secondary">
        {found.data.length === 1 ? "1 page" : `${found.data.length} pages`}
      </p>
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight takes the bullets off, and WebKit then drops the list */}
      <ul role="list" className="m-0 flex flex-col gap-0.5 p-0">
        {found.data.map((hit) => (
          <li key={hit.path}>
            <button
              type="button"
              onClick={() => onOpen(hit.path)}
              className="flex w-full cursor-pointer flex-col gap-0.5 rounded-control bg-transparent px-3 py-1.5 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover"
            >
              <span className="truncate text-base text-text">{hit.name}</span>
              <span className="truncate text-xs text-secondary">
                {hit.project ? `Project ${hit.project}` : "Organisation"}
                {hit.path !== hit.name && ` · ${hit.path}`}
              </span>
              {hit.snippet !== "" && (
                <span className="line-clamp-2 text-sm text-secondary">{hit.snippet}</span>
              )}
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}
