/**
 * The Inbox (the former desktop app's `screens/inbox`): everything that needs the person in one list, newest first, each
 * item with the buttons that act on it. The kinds are tabs with the number of items of each, and the shared filter bar
 * (`ui/filters`) filters by repository and by change. The list is the whole page, so an empty list says what belongs
 * here. It reads the snapshot, which the event stream keeps current, so an item arrives and leaves on its own. What is
 * chosen is in the address (`?kind=`, `?repository=`, `?change=`), so a reload keeps it.
 */
import { CheckCircleIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useId } from "react";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import type { Snapshot } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { FilterBar } from "../../ui/filters/FilterBar";
import type { FilterField } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { TabPanel, Tabs } from "../../ui/page/Tabs";
import { ItemCard } from "./ItemCard";
import { changeOf, chipName, count, type Filters, kindsOf, shown, summary } from "./model";

/** The fields of the bar: the repository, while there is more than one, and the change an item is about. */
function filterFields(snapshot: Snapshot, filters: Filters): FilterField[] {
  const changes = [
    ...new Set(
      snapshot.inbox.flatMap((item) => {
        const change = changeOf(item);
        return change === null ? [] : [change];
      }),
    ),
  ];
  if (filters.change != null && !changes.includes(filters.change)) changes.push(filters.change);
  return [
    {
      key: "repository",
      label: "Repository",
      options:
        snapshot.boards.length > 1
          ? snapshot.boards.map((board): [string, string] => [
              String(board.repository.id),
              board.repository.name,
            ])
          : [],
    },
    {
      key: "change",
      label: "Change",
      options: changes.map((change): [string, string] => [String(change), `Change ${change}`]),
    },
  ];
}

export function InboxPage() {
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const [params, setQuery] = useQueryParams();
  const tabs = useId();
  const now = useNow();
  if (data.isPending)
    return (
      <>
        <PageHeader title="Inbox" />
        <SkeletonList rows={3} message="Reading the inbox" />
      </>
    );
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
  const change = params.get("change");
  const filters: Filters = {
    change: change !== null && /^\d+$/.test(change) ? Number(change) : null,
    kind: params.get("kind"),
    repository: repository !== null && /^\d+$/.test(repository) ? Number(repository) : null,
  };
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
      <Tabs
        label="Kind"
        idBase={tabs}
        value={filters.kind ?? ""}
        onChange={(kind) => setQuery({ kind })}
        tabs={[
          { value: "", label: `All ${count(snapshot, null, filters.repository)}` },
          ...kindsOf(snapshot).map((kind) => ({
            value: kind,
            label: `${chipName(kind)} ${count(snapshot, kind, filters.repository)}`,
          })),
        ]}
        className="mb-4"
      />
      <FilterBar
        label="Filter the inbox"
        fields={filterFields(snapshot, filters)}
        values={{
          repository: filters.repository === null ? "" : String(filters.repository),
          change: filters.change == null ? "" : String(filters.change),
        }}
        onFilter={(key, value) => setQuery({ [key]: value })}
        onClear={() => setQuery({ repository: "", change: "" })}
      />
      <TabPanel idBase={tabs} value={filters.kind ?? ""} className="flex flex-col gap-4">
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
                  onClick={() => setQuery({ kind: "", repository: "", change: "" })}
                >
                  Show everything
                </Button>
              }
            />
          )
        ) : (
          items.map((item) => <ItemCard key={item.id} item={item} snapshot={snapshot} now={now} />)
        )}
      </TabPanel>
    </>
  );
}
