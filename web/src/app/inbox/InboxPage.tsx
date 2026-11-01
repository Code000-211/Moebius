/**
 * The Inbox (the desktop app's `screens/inbox`): everything that needs the person in one list, newest first, filtered
 * by kind and by repository, each item with the buttons that act on it. It reads the snapshot, which the event stream
 * keeps current, so an item arrives and leaves on its own. The filters are in the address (`?kind=`, `?repository=`).
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import { Loader } from "../../ui/components/Loader";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Header } from "../shell/Page";
import { ItemCard } from "./ItemCard";
import { chipName, count, type Filters, kindsOf, STUCK, shown, summary } from "./model";

/** The time now in Unix seconds, kept current every 30 seconds for "5m ago". */
function useNow(): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 30_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

export function InboxPage() {
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const [params, setParams] = useSearchParams();
  const now = useNow();
  if (data.isPending) return <Loader message="Reading the inbox…" className="py-double" />;
  const snapshot = data.data;
  if (!snapshot) {
    return (
      <>
        <Header title="Inbox" />
        <p role="alert" className="text-sm text-error">
          The inbox could not be read.
        </p>
      </>
    );
  }
  const repository = params.get("repository");
  const filters: Filters = {
    kind: params.get("kind"),
    repository: repository !== null && /^\d+$/.test(repository) ? Number(repository) : null,
  };
  const setFilter = (changes: Record<string, string | null>) =>
    setParams(
      (current) => {
        const next = new URLSearchParams(current);
        for (const [key, value] of Object.entries(changes)) {
          if (value === null) next.delete(key);
          else next.set(key, value);
        }
        return next;
      },
      { replace: true },
    );
  const items = shown(snapshot, filters);
  const total = snapshot.inbox.length;

  return (
    <>
      <Header title="Inbox" />
      <p className="m-0 mb-[12px] font-mono text-[12.5px] text-normal">
        {summary(total, items.length)}
      </p>
      <search className="mb-[12px] flex flex-col gap-base" aria-label="Filter the inbox">
        <div className="flex flex-wrap gap-base">
          <Chip selected={filters.kind === null} onClick={() => setFilter({ kind: null })}>
            {`All ${count(snapshot, null, filters.repository)}`}
          </Chip>
          {kindsOf(snapshot).map((kind) => (
            <Chip
              key={kind}
              selected={filters.kind === kind}
              red={kind === STUCK}
              onClick={() => setFilter({ kind })}
            >
              {`${chipName(kind)} ${count(snapshot, kind, filters.repository)}`}
            </Chip>
          ))}
        </div>
        {snapshot.boards.length > 1 && (
          <div className="flex flex-wrap gap-base">
            <Chip
              selected={filters.repository === null}
              onClick={() => setFilter({ repository: null })}
            >
              Every repository
            </Chip>
            {snapshot.boards.map((board) => (
              <Chip
                key={board.repository.id}
                selected={filters.repository === board.repository.id}
                onClick={() => setFilter({ repository: String(board.repository.id) })}
              >
                {`${board.repository.name} ${count(snapshot, filters.kind, board.repository.id)}`}
              </Chip>
            ))}
          </div>
        )}
      </search>
      {items.length === 0 ? (
        <Box title="Inbox">
          <p className="m-0 text-[13px] text-normal">
            {total === 0 ? "Everything runs on its own." : "No item matches the filter."}
          </p>
        </Box>
      ) : (
        <div className="flex flex-col gap-[12px]">
          {items.map((item) => (
            <ItemCard key={item.id} item={item} snapshot={snapshot} now={now} />
          ))}
        </div>
      )}
    </>
  );
}
