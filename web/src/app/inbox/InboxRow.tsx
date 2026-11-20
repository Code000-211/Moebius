/**
 * One row of the Inbox list: the shared 36px `ListRow` drawn from an `Entry` of `model.ts`. The status icon, the key,
 * the title, one pale reason, and on the right the owner's avatar (a person only) and the age. A row that waits for a
 * plan to be approved carries the one action, Approve plan, which approves without opening the card. Choosing the row
 * is the list's: it selects it, and the page opens the card window or shows what to decide.
 */
import type { MouseEvent } from "react";
import { Button } from "../../ui/components/Button";
import { compactAge } from "../../ui/lib/format";
import { ListRow, type RowPosition } from "../../ui/page/ListRow";
import type { Entry } from "./model";

export function InboxRow({
  entry,
  now,
  at,
  selected,
  unread,
  approving,
  onClick,
  onApprove,
}: {
  entry: Entry;
  now: number;
  at: RowPosition;
  selected: boolean;
  unread: boolean;
  approving: boolean;
  onClick: (event: MouseEvent<HTMLElement>) => void;
  onApprove: (card: number) => void;
}) {
  const card = entry.approves;
  return (
    <ListRow
      data-testid="inbox-item"
      data-kind={entry.item?.kind ?? "card"}
      at={at}
      selected={selected}
      unread={unread}
      onClick={onClick}
      status={entry.status}
      id={entry.id}
      title={entry.title}
      reason={entry.reason}
      owner={entry.owner}
      age={entry.at === null ? undefined : compactAge(now, entry.at)}
      action={
        card === null ? undefined : (
          <Button
            variant="secondary"
            size="sm"
            disabled={approving}
            className="text-accent-text"
            onClick={(event) => {
              event.stopPropagation();
              onApprove(card);
            }}
          >
            Approve plan
          </Button>
        )
      }
    />
  );
}
