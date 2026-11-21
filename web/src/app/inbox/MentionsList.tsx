/**
 * The Mentions and Following tabs: the person's notices, newest first, as one virtualised list each. Mentions holds
 * the mentions and what was given to them; Following what changed on a card they follow. Choosing a notice marks it read on the server
 * (for this person alone) and opens the card or the item it is about. It reads the notices, which `notice_added` and
 * `notice_read` keep current. `?notice=<id>` opens that notice as choosing it does (the desktop app's notification
 * goes there), and then leaves the address as the tab's own.
 */
import { AtIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { readNotice, notices as readNotices } from "../../api/calls/notices";
import { keys } from "../../api/keys";
import type { Snapshot } from "../../api/types/board";
import type { Notice } from "../../api/types/notices";
import { Empty } from "../../ui/page/Empty";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { useToast } from "../../ui/page/Toast";
import { type ListItem, VirtualList } from "../../ui/page/VirtualList";
import { cardKey } from "./model";
import { NoticeRow } from "./NoticeRow";
import { isFollowing, MENTIONS, pathOf } from "./notices";

export function MentionsList({
  snapshot,
  now,
  following = false,
}: {
  snapshot: Snapshot;
  now: number;
  /** Shows the notices of the cards the person follows (the Following tab) instead of their mentions. */
  following?: boolean;
}) {
  const toast = useToast();
  const [chosen, setChosen] = useState<string | null>(null);
  const data = useQuery({ queryKey: keys.notices, queryFn: readNotices });
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const read = useMutation({
    mutationFn: (notice: Notice) => readNotice(notice.id),
    // The row shows it is read at once; the list is read again to be sure.
    onMutate: (notice: Notice) =>
      queryClient.setQueryData<Notice[]>(keys.notices, (before) =>
        before?.map((each) => (each.id === notice.id ? { ...each, read: true } : each)),
      ),
    onError: () => toast({ kind: "error", text: "That mention could not be marked read." }),
    onSettled: () => void queryClient.invalidateQueries({ queryKey: keys.notices }),
  });
  const wanted = useSearchParams()[0].get("notice");
  const opened = useRef<string | null>(null);
  const notices = data.data;
  // biome-ignore lint/correctness/useExhaustiveDependencies: a read that ends with the same list must run it again
  useEffect(() => {
    if (wanted === null || notices === undefined || opened.current === wanted || data.isFetching)
      return;
    const notice = notices.find((each) => String(each.id) === wanted);
    // A notice the list does not hold yet is looked for again once the list is read again.
    if (notice === undefined && opened.current !== `${wanted}?`) {
      opened.current = `${wanted}?`;
      void data.refetch();
      return;
    }
    opened.current = wanted;
    if (notice?.read === false) read.mutate(notice);
    const path = notice === undefined ? null : pathOf(snapshot, notice);
    void navigate(path ?? `/inbox?kind=${MENTIONS}`, { replace: true });
  }, [
    wanted,
    notices,
    snapshot,
    navigate,
    read.mutate,
    data.isFetching,
    data.refetch,
    data.dataUpdatedAt,
  ]);
  if (data.isError) {
    return (
      <ReadFailed
        error={data.error}
        fallback="Your mentions could not be read. Try again."
        busy={data.isFetching}
        onRetry={() => void data.refetch()}
      />
    );
  }
  const items: ListItem<Notice>[] = (data.data ?? [])
    .filter((notice) => isFollowing(notice) === following)
    .map((notice) => ({
      type: "row",
      key: String(notice.id),
      data: notice,
    }));
  const open = (key: string) => {
    const found = items.find((each) => each.key === key);
    const notice = found?.type === "row" ? found.data : undefined;
    if (notice === undefined) return;
    if (!notice.read) read.mutate(notice);
    const path = pathOf(snapshot, notice);
    if (path !== null) void navigate(path);
  };
  return (
    <VirtualList
      label={following ? "Following" : "Mentions"}
      className="-mx-6 -mb-6 min-h-0 flex-1 outline-none"
      items={items}
      loading={data.isPending}
      loadingMessage={following ? "Reading what you follow" : "Reading your mentions"}
      selectedKey={chosen}
      onSelect={setChosen}
      onOpen={open}
      renderRow={(notice, { at, selected, onClick }) => (
        <NoticeRow
          notice={notice}
          id={cardKeyOf(snapshot, notice)}
          now={now}
          at={at}
          selected={selected}
          onClick={(event) => {
            onClick(event);
            open(String(notice.id));
          }}
        />
      )}
      empty={
        <Empty
          centered
          title={following ? "Nothing has changed" : "Nothing mentions you"}
          icon={<AtIcon className="size-6 text-secondary" />}
        />
      }
    />
  );
}

/** The key of the card a notice is about, or nothing for one that is about none. */
function cardKeyOf(snapshot: Snapshot, notice: Notice): string {
  const card = snapshot.boards
    .flatMap((board) => board.cards)
    .find((each) => each.id === notice.card_id);
  return card ? cardKey(card) : "";
}
