/**
 * The Inbox (the desktop app's `screens/inbox`): everything that needs the person in one list, newest first, filtered
 * by kind and by repository, each item with the buttons that act on it. The list is the whole page, so the filters sit
 * above it under a word each and an empty list says what belongs here. It reads the snapshot, which the event stream
 * keeps current, so an item arrives and leaves on its own. The filters are in the address (`?kind=`, `?repository=`).
 */
import { CheckCircleIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { useSearchParams } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { useNow } from "../../ui/lib/useNow";
import { Chip } from "../../ui/page/Chip";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { ItemCard } from "./ItemCard";
import { chipName, count, type Filters, kindsOf, STUCK, shown, summary } from "./model";

/** A row of filter chips, with the word for what they filter by. */
function FilterRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <fieldset className="m-0 flex min-w-0 flex-col gap-2 border-0 p-0">
      <legend className="mb-2 p-0 text-sm font-semibold text-secondary">{label}</legend>
      <div className="flex flex-wrap gap-2">{children}</div>
    </fieldset>
  );
}

export function InboxPage() {
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const [params, setParams] = useSearchParams();
  const now = useNow();
  if (data.isPending) return <Loader message="Reading the inbox" className="py-8" />;
  if (data.isError) {
    return (
      <>
        <PageHeader title="Inbox" />
        <ReadFailed
          error={data.error}
          fallback="The inbox could not be read. Try again."
          busy={data.isFetching}
          onRetry={() => void data.refetch()}
        />
      </>
    );
  }
  const snapshot = data.data;
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
      <PageHeader
        title="Inbox"
        context={
          total === 0 ? undefined : (
            <span className="tabular-nums">{summary(total, items.length)}</span>
          )
        }
      />
      <search aria-label="Filter the inbox" className="mb-6 flex flex-col gap-4">
        <FilterRow label="Kind">
          <Chip selected={filters.kind === null} onClick={() => setFilter({ kind: null })}>
            All <span className="tabular-nums">{count(snapshot, null, filters.repository)}</span>
          </Chip>
          {kindsOf(snapshot).map((kind) => (
            <Chip
              key={kind}
              selected={filters.kind === kind}
              tone={kind === STUCK ? "error" : "neutral"}
              onClick={() => setFilter({ kind })}
            >
              {chipName(kind)}{" "}
              <span className="tabular-nums">{count(snapshot, kind, filters.repository)}</span>
            </Chip>
          ))}
        </FilterRow>
        {snapshot.boards.length > 1 && (
          <FilterRow label="Repository">
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
                {board.repository.name}{" "}
                <span className="tabular-nums">
                  {count(snapshot, filters.kind, board.repository.id)}
                </span>
              </Chip>
            ))}
          </FilterRow>
        )}
      </search>
      {items.length === 0 ? (
        total === 0 ? (
          <Empty
            title="All caught up"
            icon={<CheckCircleIcon className="size-6 text-complete-text" weight="fill" />}
            body="Nothing is waiting on you. A stuck run, a change ready to merge or a proposal from the Retro will turn up here."
          />
        ) : (
          <Empty
            title="Nothing here with those filters"
            body="No item matches the filter. Show everything to see the rest."
            action={
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setFilter({ kind: null, repository: null })}
              >
                Show everything
              </Button>
            }
          />
        )
      ) : (
        <div className="flex flex-col gap-4">
          {items.map((item) => (
            <ItemCard key={item.id} item={item} snapshot={snapshot} now={now} />
          ))}
        </div>
      )}
    </>
  );
}
