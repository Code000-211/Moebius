/**
 * The Mentions tab: the person's notices, newest first, as one list. Choosing a notice marks it read on the server
 * (for this person alone) and opens the card or the item it is about. It reads the notices, which `notice_added` and
 * `notice_read` keep current. `?notice=<id>` opens that notice as choosing it does (the desktop app's notification
 * goes there), and then leaves the address as the tab's own.
 */
import { AtIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { readNotice, notices as readNotices } from "../../api/calls/notices";
import { keys } from "../../api/keys";
import type { Snapshot } from "../../api/types/board";
import type { Notice } from "../../api/types/notices";
import { Empty } from "../../ui/page/Empty";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { NoticeRow } from "./NoticeRow";
import { inTab, MENTIONS, pathOf } from "./notices";

const EMPTY: Record<string, { title: string; body: string }> = {
  mentions: {
    title: "Nothing mentions you",
    body: "A comment that names you with @ will turn up here.",
  },
  assigned: { title: "Nothing is given to you", body: "A card given to you will turn up here." },
  following: {
    title: "Nothing changed",
    body: "A change to a card you follow will turn up here.",
  },
};

export function MentionsList({
  snapshot,
  now,
  tab = MENTIONS,
}: {
  snapshot: Snapshot;
  now: number;
  tab?: string;
}) {
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
  if (data.isPending) return <SkeletonList rows={3} message="Reading your mentions" />;
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
  const shown = inTab(data.data, tab);
  if (shown.length === 0) {
    return (
      <Empty
        title={EMPTY[tab]?.title ?? "Nothing here"}
        icon={<AtIcon className="size-6 text-secondary" />}
        body={EMPTY[tab]?.body ?? ""}
      />
    );
  }
  return (
    <div className="overflow-hidden rounded-card bg-surface shadow-card">
      {/* biome-ignore lint/a11y/noRedundantRoles: list-style: none drops the list in WebKit */}
      <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
        {shown.map((notice) => (
          <li key={notice.id}>
            <NoticeRow
              notice={notice}
              now={now}
              onChoose={() => {
                if (!notice.read) read.mutate(notice);
                const path = pathOf(snapshot, notice);
                if (path !== null) void navigate(path);
              }}
            />
          </li>
        ))}
      </ul>
    </div>
  );
}
