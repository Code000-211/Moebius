/**
 * The Mentions tab: the person's notices, newest first, as one list. Choosing a notice marks it read on the server
 * (for this person alone) and opens the card or the item it is about. It reads the notices, which `notice_added` and
 * `notice_read` keep current.
 */
import { AtIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import { readNotice, notices as readNotices } from "../../api/calls/notices";
import { keys } from "../../api/keys";
import type { Snapshot } from "../../api/types/board";
import type { Notice } from "../../api/types/notices";
import { Empty } from "../../ui/page/Empty";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { NoticeRow } from "./NoticeRow";
import { pathOf } from "./notices";

export function MentionsList({ snapshot, now }: { snapshot: Snapshot; now: number }) {
  const data = useQuery({ queryKey: keys.notices, queryFn: readNotices });
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const read = useMutation({
    mutationFn: (notice: Notice) => readNotice(notice.id),
    onSettled: () => void queryClient.invalidateQueries({ queryKey: keys.notices }),
  });
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
  if (data.data.length === 0) {
    return (
      <Empty
        title="Nothing mentions you"
        icon={<AtIcon className="size-6 text-secondary" />}
        body="A card given to you, a comment that names you with @ and a change to a card you follow will turn up here."
      />
    );
  }
  return (
    <div className="overflow-hidden rounded-card bg-surface shadow-card">
      {/* biome-ignore lint/a11y/noRedundantRoles: list-style: none drops the list in WebKit */}
      <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
        {data.data.map((notice) => (
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
