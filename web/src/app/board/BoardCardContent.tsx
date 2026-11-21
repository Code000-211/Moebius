/**
 * What one card shows on the board and the drag overlay: its icon, number, title, owner and activity, its menu, and the
 * control that opens the stack of its sub-issues. `BoardColumn.tsx` draws it inside a draggable card.
 */
import { CaretDownIcon, CaretRightIcon } from "@phosphor-icons/react";
import type { ReactNode, SyntheticEvent } from "react";
import type { Card } from "../../api/types/board";
import { KanbanCardContent } from "../../ui/components/KanbanCardContent";
import { keepKey } from "../../ui/lib/shortcuts";
import { CardMenu } from "./CardMenu";
import { cardKey } from "./cardKey";
import { type CardStatus, cardActivity, cardIcon, type Stack, stackWords } from "./model";

/** What the menus of the cards of a board do. */
export type CardMenuActions = {
  onBlock: (card: Card) => void;
  onUnblock: (card: Card) => void;
  onStop: (card: Card) => void;
  onDelete: (card: Card) => void;
  onAddSubIssue: (card: Card) => void;
  onMoveParent: (card: Card) => void;
};

/**
 * What a card shows, on the board and on the drag overlay: its status icon, number, title, the person it is given to,
 * and one pale line while an agent works on it or it needs a person. It takes what is derived from the board (the
 * card's status) rather than the board itself, so that the cards the board holds can be memoised.
 */
export function CardContent({
  card,
  status,
  waiting,
  onOpen,
  menu,
  stack,
  open = false,
  onToggleStack,
  select,
}: {
  card: Card;
  /** What the card is doing (`cardStatus`). */
  status: CardStatus;
  /** What a card that needs a person waits for, in words (`waitingWords`). */
  waiting?: string;
  /** The box that chooses the card, in its corner; left out on the drag overlay. */
  select?: ReactNode;
  /** Opens the card; left out on the drag overlay, where the card is only a picture of itself. */
  onOpen?: () => void;
  /** What the card's menu does; left out on the drag overlay, with the menu itself. */
  menu?: CardMenuActions;
  /** The sub-issues of the card, when it has any: how many are done, and the control that opens them. */
  stack?: Stack;
  open?: boolean;
  onToggleStack?: () => void;
}) {
  return (
    <KanbanCardContent
      displayId={cardKey(card)}
      title={card.title}
      status={cardIcon(card, status)}
      activity={cardActivity(card, status, waiting)}
      assignees={card.assignees}
      stack={
        stack && onToggleStack ? (
          <StackToggle card={card} stack={stack} open={open} onToggle={onToggleStack} />
        ) : stack ? (
          <p className="m-0 text-sm text-secondary">{stackWords(stack)}</p>
        ) : undefined
      }
      muted={card.column === "Done" || card.closed}
      onOpen={onOpen}
      menu={menu && <CardMenu card={card} {...menu} />}
      select={select}
    />
  );
}

/** How many sub-issues of a card are done, and the control that opens the stack under it or shuts it. */
function StackToggle({
  card,
  stack,
  open,
  onToggle,
}: {
  card: Card;
  stack: Stack;
  open: boolean;
  onToggle: () => void;
}) {
  const Caret = open ? CaretDownIcon : CaretRightIcon;
  // The card is dragged by its whole body and opened by a click on it; this control is neither.
  const keep = (event: SyntheticEvent) => event.stopPropagation();
  return (
    <button
      type="button"
      aria-expanded={open}
      aria-label={`Sub-issues of #${card.number}`}
      onPointerDown={keep}
      onKeyDown={keepKey}
      onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }}
      className="-mx-2 flex min-h-8 cursor-pointer appearance-none items-center gap-2 rounded-control bg-transparent px-2 text-left text-sm text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
    >
      <Caret className="size-4 shrink-0" weight="bold" aria-hidden="true" />
      {stackWords(stack)}
    </button>
  );
}
