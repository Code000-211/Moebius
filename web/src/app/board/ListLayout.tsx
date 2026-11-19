/**
 * The list layout of the board: the cards that pass the filters as rows under a heading for each group, in place of
 * the lanes. A row says the number, the title, what the card is doing in words, its labels and its agent, and opens
 * the card in the dock; there is no dragging here, since a group is not always a column. Which parts a row shows
 * follows Display's fields, as the card does.
 */
import type { Board, Card, InboxItem } from "../../api/types/board";
import { Empty } from "../../ui/page/Empty";
import { MarkLabel } from "../../ui/page/MarkLabel";
import { cardBadges, cardStatus, inboxOf } from "./model";
import type { ListGroup } from "./view";

function Row({
  board,
  inbox,
  card,
  hidden,
  selected,
  onSelect,
}: {
  board: Board;
  inbox: readonly InboxItem[];
  card: Card;
  hidden: ReadonlySet<string>;
  selected: boolean;
  onSelect: (card: Card) => void;
}) {
  const status = cardStatus(card, inboxOf(inbox, card), board);
  const badges = cardBadges(card, status, false);
  return (
    <li data-testid={`card-${card.number}`} className="min-w-0">
      <button
        type="button"
        onClick={() => onSelect(card)}
        aria-current={selected ? "true" : undefined}
        className={`flex min-h-9 w-full min-w-0 cursor-pointer appearance-none flex-wrap items-center gap-x-3 gap-y-1 rounded-control px-3 py-1 text-left text-base transition-colors duration-(--motion-routine) ease-out hover:bg-hover ${selected ? "bg-selected" : "bg-transparent"}`}
      >
        <span className="font-mono text-sm text-secondary tabular-nums">#{card.number}</span>
        <span
          className={`min-w-0 flex-1 font-medium [overflow-wrap:anywhere] ${card.closed ? "text-secondary" : "text-text"}`}
        >
          {card.title}
        </span>
        {badges.map((badge) => (
          <MarkLabel key={badge.name} mark={badge.mark} word={badge.name} detail={badge.detail} />
        ))}
        {!hidden.has("labels") &&
          card.labels.map((label) => (
            <span
              key={label}
              className="inline-flex items-center rounded-full bg-hover px-2 py-0.5 text-xs text-secondary"
            >
              {label}
            </span>
          ))}
        {!hidden.has("detail") && card.run && (
          <span className="text-sm text-secondary">{card.run.agent}</span>
        )}
        {!hidden.has("claim") && card.claimed_by && (
          <span className="text-sm text-secondary">Claimed by {card.claimed_by}</span>
        )}
      </button>
    </li>
  );
}

export function ListLayout({
  board,
  inbox,
  groups,
  hidden,
  inspectedId,
  onSelect,
}: {
  board: Board;
  inbox: readonly InboxItem[];
  groups: ListGroup[];
  hidden: ReadonlySet<string>;
  inspectedId: number | undefined;
  onSelect: (card: Card) => void;
}) {
  if (groups.length === 0) {
    return <Empty plain centered body="No card passes the filters." />;
  }
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-4 pb-3">
      {groups.map((group) => (
        <section key={group.key || group.name} aria-label={group.name} className="min-w-0">
          <h2 className="m-0 mb-1 flex items-center gap-2 px-3 text-base font-medium tracking-tight text-text">
            {group.name}
            <span className="text-sm font-normal text-secondary tabular-nums">
              {group.cards.length}
            </span>
          </h2>
          <ul className="m-0 flex list-none flex-col p-0">
            {group.cards.map((card) => (
              <Row
                key={card.id}
                board={board}
                inbox={inbox}
                card={card}
                hidden={hidden}
                selected={inspectedId === card.id}
                onSelect={onSelect}
              />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
