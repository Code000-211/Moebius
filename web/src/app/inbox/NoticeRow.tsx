/**
 * One notice of the Mentions tab as a quiet row, like an item of the Inbox (`InboxRow.tsx`): the card or item it is
 * about as the title, who did what under it, the time, and a dot while it is unread. Choosing it reads it.
 */
import type { Notice } from "../../api/types/notices";
import { cn } from "../../ui/lib/cn";
import { ago } from "../../ui/lib/format";
import { kindWord, said } from "./notices";

export function NoticeRow({
  notice,
  now,
  onChoose,
}: {
  notice: Notice;
  now: number;
  onChoose: () => void;
}) {
  return (
    <button
      type="button"
      data-testid="notice"
      data-kind={notice.kind}
      data-unread={notice.read ? undefined : ""}
      onClick={onChoose}
      className="flex min-h-12 w-full cursor-pointer items-center gap-3 px-4 py-2 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover"
    >
      <span className="flex min-w-0 flex-1 flex-col">
        <span className={cn("truncate text-base text-text", !notice.read && "font-semibold")}>
          {notice.title}
        </span>
        <span className="flex min-w-0 gap-1 text-sm text-secondary">
          <span className="shrink-0">{kindWord(notice)}</span>
          <span className="min-w-0 truncate">· {said(notice)}</span>
        </span>
      </span>
      <span className="shrink-0 text-sm tabular-nums text-secondary">{ago(now, notice.at)}</span>
      <span className="flex w-2 shrink-0 justify-center">
        {!notice.read && (
          <>
            <span
              aria-hidden="true"
              data-testid="unread-dot"
              className="size-2 rounded-full bg-accent"
            />
            <span className="sr-only">Unread</span>
          </>
        )}
      </span>
    </button>
  );
}
