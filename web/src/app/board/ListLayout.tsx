/**
 * The list layout of the board: the cards that pass the filters as rows (`ListRow`) in one virtual list, a group row
 * naming each column (or status, or label) above its cards. A row says the status icon, the number, the title, what
 * the card is doing in words and its age, and opens the card window; there is no dragging here, since a group is not
 * always a column. Which parts a row shows follows Display's fields. The box that chooses a row shows on hover only
 * (`select`), and a right click on a row opens the menu of the choice (`onContextMenu`).
 */
import { type MouseEvent, type ReactNode, useMemo, useState } from "react";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { Empty } from "../../ui/page/Empty";
import { ListRow } from "../../ui/page/ListRow";
import { useFillHeight } from "../../ui/page/useFillHeight";
import { firstRowKey, type ListItem, VirtualList } from "../../ui/page/VirtualList";
import { iconOf } from "./CardWindow";
import { cardKey } from "./cardKey";
import { cardStatus, inboxOf, STATUS_LABELS } from "./model";
import { rowAge } from "./rowAge";
import type { ListGroup } from "./view";

export function ListLayout({
  board,
  inbox,
  groups,
  hidden,
  inspectedId,
  onSelect,
  select,
  onContextMenu,
  initialItemCount,
}: {
  board: Board;
  inbox: readonly InboxItem[];
  groups: ListGroup[];
  hidden: ReadonlySet<string>;
  inspectedId: number | undefined;
  /** Opens the card: a click on its row, or Enter on the selected one. */
  onSelect: (card: Card) => void;
  select?: (card: Card) => ReactNode;
  onContextMenu?: (card: Card) => (event: MouseEvent) => void;
  /** For tests: a DOM with no size draws no rows. */
  initialItemCount?: number;
}) {
  const fill = useFillHeight();
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const { items, byKey } = useMemo(() => {
    const list: ListItem<Card>[] = [];
    const cards = new Map<string, Card>();
    for (const group of groups) {
      list.push({ type: "group", key: `group:${group.key || group.name}`, name: group.name });
      for (const card of group.cards) {
        // A card with several labels is under each of them, so its place in the list is told by the group too.
        const key = `${group.key || group.name}:${card.id}`;
        cards.set(key, card);
        list.push({ type: "row", key, data: card });
      }
    }
    return { items: list, byKey: cards };
  }, [groups]);
  if (groups.length === 0) {
    return <Empty plain centered body="No card passes the filters." />;
  }
  const now = Date.now() / 1000;
  // The right click is heard once for the whole list and finds its row by the key the row carries.
  const rightClick = (event: MouseEvent<HTMLDivElement>) => {
    const key = (event.target as HTMLElement).closest("[data-key]")?.getAttribute("data-key");
    const card = key ? byKey.get(key) : undefined;
    if (card) onContextMenu?.(card)(event);
  };
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: the right click is the pointer's way to the menu; the list owns the keys
    <div ref={fill.ref} onContextMenu={rightClick} style={{ height: fill.height }}>
      <VirtualList
        label="Cards"
        items={items}
        initialItemCount={initialItemCount}
        selectedKey={selectedKey}
        restKey={inspectedId === undefined ? firstRowKey(items) : null}
        onSelect={setSelectedKey}
        className="h-full outline-none"
        onOpen={(key) => {
          const card = byKey.get(key);
          if (card) onSelect(card);
        }}
        renderRow={(card, { key, at, onClick, selected }) => {
          const status = cardStatus(card, inboxOf(inbox, card), board);
          const reason = !hidden.has("detail")
            ? card.run
              ? card.run.agent
              : status === "idle"
                ? undefined
                : STATUS_LABELS[status]
            : undefined;
          return (
            <ListRow
              at={at}
              data-key={key}
              status={iconOf(card, status)}
              id={cardKey(card)}
              title={card.title}
              reason={reason}
              action={select?.(card)}
              age={rowAge(card.created_at, now)}
              selected={selected || inspectedId === card.id}
              onClick={(event) => {
                onClick(event);
                onSelect(card);
              }}
            />
          );
        }}
      />
    </div>
  );
}
