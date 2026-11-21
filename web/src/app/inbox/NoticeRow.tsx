/**
 * One notice of the Mentions tab as a row of the shared list, like an item of the Inbox (`InboxRow.tsx`): the card
 * it is about, its title, who did what as the one pale reason, the avatar of who did it, and the age. One not read yet
 * has the needs-you icon; choosing it reads it.
 */
import type { MouseEvent } from "react";
import type { Notice } from "../../api/types/notices";
import { compactAge } from "../../ui/lib/format";
import { ListRow, type RowPosition } from "../../ui/page/ListRow";
import { said } from "./notices";

export function NoticeRow({
  notice,
  id,
  now,
  at,
  selected,
  onClick,
}: {
  notice: Notice;
  /** The key of the card the notice is about; empty for one that is about none. */
  id: string;
  now: number;
  at: RowPosition;
  selected: boolean;
  onClick: (event: MouseEvent<HTMLElement>) => void;
}) {
  return (
    <ListRow
      data-testid="notice"
      data-kind={notice.kind}
      at={at}
      selected={selected}
      onClick={onClick}
      status={notice.read ? "notStarted" : "needsYou"}
      id={id}
      title={notice.title}
      reason={said(notice)}
      owner={notice.by ?? undefined}
      age={compactAge(now, notice.at)}
    />
  );
}
