/**
 * One item of the Inbox as a quiet row without buttons: the kind's status icon, the key of what it is about, the title
 * with a grey line of context under it (the kind's word, where it comes from and why it is here), the time and a dot
 * while it is unread. Choosing the row shows what to decide in the right sidebar (`Decision.tsx`) and reads the item.
 */
import type { InboxItem, Snapshot } from "../../api/types/board";
import { cn } from "../../ui/lib/cn";
import { ago } from "../../ui/lib/format";
import { MarkIcon } from "../../ui/page/MarkLabel";
import { found, keyOf, kindTag, meta, why } from "./model";

export function InboxRow({
  item,
  snapshot,
  now,
  chosen,
  unread,
  onChoose,
}: {
  item: InboxItem;
  snapshot: Snapshot;
  now: number;
  chosen: boolean;
  unread: boolean;
  onChoose: () => void;
}) {
  const about = found(snapshot, item);
  const { label, mark } = kindTag(item);
  const rest = [meta(item, about), why(item, about) ?? ""]
    .filter((part) => part !== "")
    .join(" · ");
  const key = keyOf(item, about);
  return (
    <button
      type="button"
      data-testid="inbox-item"
      data-kind={item.kind}
      data-unread={unread ? "" : undefined}
      aria-pressed={chosen}
      onClick={onChoose}
      className={cn(
        "flex min-h-12 w-full cursor-pointer items-center gap-3 px-4 py-2 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover",
        chosen && "bg-selected",
      )}
    >
      <span className="flex w-4 shrink-0 justify-center">
        <MarkIcon mark={mark} />
      </span>
      {key !== "" && (
        <span className="w-20 shrink-0 truncate text-sm tabular-nums text-secondary">{key}</span>
      )}
      <span className="flex min-w-0 flex-1 flex-col">
        <span className={cn("truncate text-base text-text", unread && "font-semibold")}>
          {item.title}
        </span>
        <span className="flex min-w-0 gap-1 text-sm text-secondary">
          <span className="shrink-0">{label}</span>
          {rest !== "" && <span className="min-w-0 truncate">· {rest}</span>}
        </span>
      </span>
      <span className="shrink-0 text-sm tabular-nums text-secondary">{ago(now, item.at)}</span>
      <span className="flex w-2 shrink-0 justify-center">
        {unread && (
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
