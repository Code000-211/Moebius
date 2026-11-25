/**
 * The cards listed under a cycle or a milestone, as the same rows the lists use (`ListRow`): the status icon, the
 * number, the title, what is said about the card, and a click opens the card window.
 */

import { cardClass } from "../../api/columns";
import { ListRow } from "../../ui/page/ListRow";
import type { StatusState } from "../../ui/page/StatusIcon";
import { useOpenCard } from "./CardWindowHost";

export interface CardRowData {
  id: number;
  number: number;
  title: string;
  column: string;
  column_class?: string;
  done: boolean;
  /** Said after the title, such as the estimate. */
  note?: string;
}

function stateOf(card: CardRowData): StatusState {
  if (card.done) return "done";
  if (cardClass(card) === "In review") return "inReview";
  if (cardClass(card) === "In progress") return "inProgress";
  return "notStarted";
}

export function CardRows({ label, cards }: { label: string; cards: readonly CardRowData[] }) {
  const open = useOpenCard();
  return (
    // biome-ignore lint/a11y/useSemanticElements: the rows are divs of the shared row, so the list says its role
    <div role="list" aria-label={label} className="flex flex-col">
      {cards.map((card, index) => (
        <ListRow
          key={card.id}
          at={{ index: index + 1, size: cards.length }}
          status={stateOf(card)}
          id={`#${card.number}`}
          title={card.title}
          reason={[card.note, card.done ? "Done" : card.column].filter(Boolean).join(" · ")}
          onClick={() => open(card.id)}
        />
      ))}
    </div>
  );
}
