/**
 * A page's history: the commits that changed it, newest first, each with its author, its age and its message, and the
 * page's path when it was another before a rename or a move. A row opens the page as that commit left it.
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { wikiHistory } from "../../api/calls/wiki";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { useToast } from "../../ui/page/Toast";

export function PageHistory({
  path,
  at,
  onOpen,
}: {
  path: string;
  /** The version being read, marked in the list. */
  at: string | null;
  onOpen: (commit: string) => void;
}) {
  const now = useNow();
  const toast = useToast();
  const history = useQuery({ queryKey: keys.wikiHistory(path), queryFn: () => wikiHistory(path) });
  const failed = history.error;
  useEffect(() => {
    if (failed)
      toast({ kind: "error", text: sayError(failed, "Could not read the page's history.") });
  }, [failed, toast]);
  if (history.isPending) return <SkeletonCard lines={3} />;
  if (history.isError) return null;
  return (
    <ul
      // biome-ignore lint/a11y/noRedundantRoles: preflight takes the bullets off, and WebKit then drops the list
      role="list"
      aria-label={`History of ${path}`}
      className="m-0 flex flex-col border-y border-hairline p-0"
    >
      {history.data.map((version, index) => (
        <li key={version.commit} className="flex flex-col border-b border-hairline last:border-b-0">
          <button
            type="button"
            aria-current={
              at === version.commit || (at === null && index === 0) ? "true" : undefined
            }
            onClick={() => onOpen(version.commit)}
            className="flex min-h-9 w-full cursor-pointer flex-col gap-0.5 rounded-control bg-transparent px-3 py-1.5 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover aria-[current=true]:bg-selected"
          >
            <span className="flex min-w-0 items-baseline justify-between gap-3">
              <span className="min-w-0 break-words text-base text-text">{version.message}</span>
              <span className="shrink-0 font-mono text-xs text-muted">
                {version.commit.slice(0, 7)}
              </span>
            </span>
            <span className="text-xs text-muted">
              {version.author} · {ago(now, version.at)}
              {version.path !== path && ` · as ${version.path}`}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}
