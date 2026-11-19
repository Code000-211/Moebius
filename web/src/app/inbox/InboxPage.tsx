/**
 * The Inbox: everything that needs the person in one quiet list, newest first, a row for each item without buttons
 * (`InboxRow`). The kinds are tabs with the number of items of each, and the shared filter bar (`ui/filters`) filters
 * by repository and by change. Choosing a row reads it (it loses its dot) and shows what to decide in the right
 * sidebar (`Decision`). The list is the whole page, so an empty list says what belongs here. It reads the snapshot,
 * which the event stream keeps current, so an item arrives and leaves on its own. What is chosen is in the address
 * (`?kind=`, `?repository=`, `?change=`, `?item=`), so a reload keeps it. The Mentions tab (`?kind=mentions`) is the
 * person's own notices — what was given to them, where they were mentioned, what changed on a card they follow — with
 * a read mark the server keeps for them (`MentionsList`); there is no bell.
 */
import { CheckCircleIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId } from "react";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { changeInboxAssignees } from "../../api/calls/inbox";
import { notices as readNotices } from "../../api/calls/notices";
import { keys } from "../../api/keys";
import type { Snapshot } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { FilterBar } from "../../ui/filters/FilterBar";
import type { FilterField } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { PageProperties } from "../../ui/page/Properties";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { TabPanel, Tabs } from "../../ui/page/Tabs";
import { AssigneesProperty } from "../people/AssigneesProperty";
import { Decision } from "./Decision";
import { InboxRow } from "./InboxRow";
import { MentionsList } from "./MentionsList";
import { changeOf, chipName, count, type Filters, kindsOf, shown, summary } from "./model";
import { MENTIONS, unreadCount } from "./notices";
import { useRead } from "./read";

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
  const queryClient = useQueryClient();
  const [params, setQuery] = useQueryParams();
  const tabs = useId();
  const now = useNow();
  const { read, markRead } = useRead();
  const mentions = useQuery({ queryKey: keys.notices, queryFn: readNotices });
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
  const onMentions = filters.kind === MENTIONS;
  const items = onMentions ? [] : shown(snapshot, filters);
  const total = snapshot.inbox.length;
  const choice = params.get("item");
  const chosen = items.find((each) => String(each.id) === choice) ?? null;

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
          { value: MENTIONS, label: `Mentions ${unreadCount(mentions.data ?? [])}` },
          ...kindsOf(snapshot).map((kind) => ({
            value: kind,
            label: `${chipName(kind)} ${count(snapshot, kind, filters.repository)}`,
          })),
        ]}
        className="mb-4"
      />
      {!onMentions && (
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
      )}
      <TabPanel idBase={tabs} value={filters.kind ?? ""} className="flex flex-col gap-4">
        {onMentions ? (
          <MentionsList snapshot={snapshot} now={now} />
        ) : items.length === 0 ? (
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
          <div className="overflow-hidden rounded-card bg-surface shadow-card">
            {/* biome-ignore lint/a11y/noRedundantRoles: list-style: none drops the list in WebKit */}
            <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
              {items.map((item) => (
                <li key={item.id}>
                  <InboxRow
                    item={item}
                    snapshot={snapshot}
                    now={now}
                    chosen={chosen?.id === item.id}
                    unread={!read.has(item.id)}
                    onChoose={() => setQuery({ item: String(item.id) })}
                  />
                </li>
              ))}
            </ul>
          </div>
        )}
      </TabPanel>
      {chosen && <ReadOnShow id={chosen.id} markRead={markRead} />}
      {chosen && (
        <PageProperties label="Decide">
          <AssigneesProperty
            key={`assignees-${chosen.id}`}
            assignees={chosen.assignees}
            onSave={async (next) => {
              try {
                await changeInboxAssignees(chosen.id, chosen.assignees, next);
              } finally {
                void queryClient.invalidateQueries({ queryKey: keys.snapshot });
              }
            }}
          />
          <Decision key={chosen.id} item={chosen} snapshot={snapshot} />
        </PageProperties>
      )}
    </>
  );
}

/** What is chosen is read, whether a person chose it or the address did. */
function ReadOnShow({ id, markRead }: { id: number; markRead: (id: number) => void }) {
  useEffect(() => markRead(id), [id, markRead]);
  return null;
}
