/**
 * The Inbox: what needs the person, what an agent is working on and what is done, as one virtualised list of 36px rows
 * under group rows (`VirtualList`, `InboxRow`, `listOf` in `model.ts`). The tabs are Needs you (with how many wait), Mentions and Following; the header carries New issue.
 * The row that waits for a plan has Approve plan on it, which approves without opening anything. Choosing a row opens its card in the card window; an item that is about no card opens what to decide in a
 * centred window like the card's (`Decision`). It reads the snapshot, which the event stream keeps current, so an item arrives and
 * leaves on its own. What is chosen is in the address (`?kind=mentions`, `?repository=`, `?change=`, `?item=`), so a
 * reload keeps it; a repository or change in the address shows the filter bar so it can be taken off. The Mentions
 * tab is the person's own notices, with a read mark the server keeps for them (`MentionsList`); there is no bell.
 * Failures are toasts.
 */
import { ArrowsOutIcon, CheckCircleIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useState } from "react";
import { Link } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { changeInboxAssignees } from "../../api/calls/inbox";
import { readNotice, notices as readNotices } from "../../api/calls/notices";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { InboxItem, Snapshot } from "../../api/types/board";
import type { Notice } from "../../api/types/notices";
import { Button } from "../../ui/components/Button";
import { EditorWindow } from "../../ui/components/EditorWindow";
import { FilterBar } from "../../ui/filters/FilterBar";
import type { FilterField } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { TabPanel, Tabs } from "../../ui/page/Tabs";
import { useToast } from "../../ui/page/Toast";
import { firstRowKey, openedKey, VirtualList } from "../../ui/page/VirtualList";
import { useOpenCard } from "../board/CardWindowHost";
import { NewIssueButton } from "../issues/NewIssueButton";
import { AssigneesProperty } from "../people/AssigneesProperty";
import { perform } from "./actions";
import { Decision, useDecisionBar } from "./Decision";
import { InboxRow } from "./InboxRow";
import { MentionsList } from "./MentionsList";
import { changeOf, type Filters, listOf, shown } from "./model";
import { FOLLOWING, isNoticeTab, MENTIONS, waiting } from "./notices";

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
  const toast = useToast();
  const openCard = useOpenCard();
  const mentions = useQuery({ queryKey: keys.notices, queryFn: readNotices });
  const [picked, setPicked] = useState<string | null>(null);
  const [unsent, setUnsent] = useState(false);
  const approving = useMutation({
    mutationFn: (card: number) => perform({ kind: "approve", card }),
    onError: (error) =>
      toast({
        kind: "error",
        text: isApiError(error) ? error.message : "The plan could not be approved. Try again.",
      }),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.cards });
    },
  });
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
  const onMentions = isNoticeTab(filters.kind);
  const tab = filters.kind === FOLLOWING ? FOLLOWING : onMentions ? MENTIONS : "";
  const items = snapshot === undefined || onMentions ? [] : shown(snapshot, filters);
  const choice = params.get("item");
  const chosen = items.find((each) => String(each.id) === choice) ?? null;
  const list = snapshot === undefined ? [] : listOf(snapshot, items, filters);
  const filtered = filters.repository !== null || filters.change != null;

  /** Opens a row: its card in the window, or what to decide about it in a window. */
  const open = (key: string) => {
    const entry = list.find((each) => each.key === key);
    if (entry?.type !== "row") return;
    const { item, card } = entry.data;
    if (card !== null) openCard(card);
    else if (item !== null) setQuery({ item: String(item.id) });
  };
  const asked = params.get("card");
  const selected = openedKey(
    list,
    picked ?? (chosen === null ? null : `item-${chosen.id}`),
    (entry) => asked !== null && entry.card !== null && String(entry.card) === asked,
  );

  return (
    <div className="flex h-full min-h-0 flex-col [&>header]:mb-0">
      <PageHeader
        title="Inbox"
        tabs={
          <Tabs
            label="Kind"
            idBase={tabs}
            value={tab}
            onChange={(kind) => setQuery({ kind, item: "" })}
            tabs={[
              {
                value: "",
                label: "Needs you",
                count: snapshot ? waiting(snapshot.inbox.length, mentions.data ?? []) : undefined,
              },
              { value: MENTIONS, label: "Mentions" },
              { value: FOLLOWING, label: "Following" },
            ]}
          />
        }
        actions={<NewIssueButton />}
      />
      {!onMentions && filtered && snapshot && (
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
      <TabPanel idBase={tabs} value={tab} className="flex min-h-0 flex-1 flex-col">
        {onMentions ? (
          snapshot && <MentionsList snapshot={snapshot} now={now} following={tab === FOLLOWING} />
        ) : (
          <VirtualList
            label="Inbox"
            className="-mx-4 -mb-4 min-h-0 flex-1 outline-none"
            items={list}
            loading={data.isPending}
            loadingMessage="Reading the inbox"
            selectedKey={selected}
            restKey={firstRowKey(list)}
            onSelect={setPicked}
            onOpen={open}
            renderRow={(entry, { at, selected: on, onClick }) => (
              <InboxRow
                entry={entry}
                now={now}
                at={at}
                selected={on}
                approving={approving.isPending}
                onClick={(event) => {
                  onClick(event);
                  open(entry.key);
                }}
                onApprove={(card) => approving.mutate(card)}
              />
            )}
            empty={
              filtered ? (
                <Empty
                  centered
                  title="Nothing here with those filters"
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
              ) : (
                <Empty
                  centered
                  title="Nothing needs you"
                  icon={<CheckCircleIcon className="size-6 text-complete-text" weight="fill" />}
                  action={
                    <Link to="/fleet" className="text-accent-text no-underline hover:underline">
                      Open Fleet
                    </Link>
                  }
                />
              )
            }
          />
        )}
      </TabPanel>
      {snapshot && (
        <Settled
          choice={choice}
          gone={choice !== null && !snapshot.inbox.some((each) => String(each.id) === choice)}
          onGone={() => setQuery({ item: "" })}
        />
      )}
      {chosen && (
        <ReadNoticesOf
          item={chosen.id}
          notices={mentions.data ?? []}
          onRead={(id) => {
            queryClient.setQueryData<Notice[]>(keys.notices, (before) =>
              before?.map((each) => (each.id === id ? { ...each, read: true } : each)),
            );
            void readNotice(id).finally(() =>
              queryClient.invalidateQueries({ queryKey: keys.notices }),
            );
          }}
        />
      )}
      {snapshot && chosen && (
        <DecideWindow
          item={chosen}
          snapshot={snapshot}
          unsent={unsent}
          onDirty={setUnsent}
          onClose={() => setQuery({ item: "" })}
        />
      )}
    </div>
  );
}

/** An item that is resolved (or never was) leaves the address, so it does not name what is gone. */
function Settled({
  choice,
  gone,
  onGone,
}: {
  choice: string | null;
  gone: boolean;
  onGone: () => void;
}) {
  // biome-ignore lint/correctness/useExhaustiveDependencies: the address changes only when the choice does
  useEffect(() => {
    if (choice !== null && gone) onGone();
  }, [choice, gone]);
  return null;
}

/** Opening an item reads the notices about it, which the server keeps for the person. */
function ReadNoticesOf({
  item,
  notices,
  onRead,
}: {
  item: number;
  notices: readonly Notice[];
  onRead: (id: number) => void;
}) {
  const unread = notices.filter((each) => each.inbox_item_id === item && !each.read);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a notice is read once; the list changes as it is
  useEffect(() => {
    for (const each of unread) onRead(each.id);
  }, [unread.map((each) => each.id).join(",")]);
  return null;
}

/**
 * The window of an item to decide: the item's kind and file in the top bar with the way to its own page, then what is
 * to be decided and its buttons, and the assignee below them. The window has no foot of its own; the buttons are the item's.
 */
function DecideWindow({
  item,
  snapshot,
  unsent,
  onDirty,
  onClose,
}: {
  item: InboxItem;
  snapshot: Snapshot;
  unsent: boolean;
  onDirty: (dirty: boolean) => void;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const { leading, href } = useDecisionBar(item, snapshot);
  return (
    <EditorWindow
      open
      title={item.title}
      leading={leading}
      trailing={
        href ? (
          <Button asChild variant="ghost" size="sm">
            <Link to={href}>
              <ArrowsOutIcon className="size-4" aria-hidden="true" />
              Open as page
            </Link>
          </Button>
        ) : undefined
      }
      dirty={unsent}
      height="fit"
      footer={false}
      onClose={onClose}
    >
      <section aria-label="Decide" className="flex flex-col gap-4">
        <Decision key={item.id} item={item} snapshot={snapshot} onDirty={onDirty} />
        <AssigneesProperty
          key={`assignees-${item.id}`}
          assignees={item.assignees}
          onSave={async (next) => {
            try {
              await changeInboxAssignees(item.id, item.assignees, next);
            } finally {
              void queryClient.invalidateQueries({ queryKey: keys.snapshot });
            }
          }}
        />
      </section>
    </EditorWindow>
  );
}
