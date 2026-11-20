/**
 * What one card shows on the board, the list and the drag overlay: its words, badges and menu, and the control that opens
 * the stack of its sub-issues. `BoardColumn.tsx` draws it inside a draggable card.
 */
import { CaretDownIcon, CaretRightIcon } from "@phosphor-icons/react";
import type { ReactNode, SyntheticEvent } from "react";
import type { Card, Change, Repository } from "../../api/types/board";
import { KanbanCardContent } from "../../ui/components/KanbanCardContent";
import { CardMenu } from "./CardMenu";
import { cardName } from "./cardKey";
import {
  type CardStatus,
  cardBadges,
  cardDetail,
  isClosed,
  isPlanningLabel,
  issueUrl,
  pullRequestOf,
  type Stack,
  stackWords,
  todayIso,
} from "./model";

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
 * What a card shows, on the board and on the drag overlay. It takes what is derived from the board (the card's status and
 * change, whether it is the next to start) rather than the board itself, so that the cards the board holds can be
 * memoised: a card draws again only when one of these changes.
 */
export function CardContent({
  repository,
  roundLimit,
  card,
  status,
  change,
  next,
  onOpen,
  menu,
  stack,
  open = false,
  onToggleStack,
  inStack = false,
  select,
  hidden,
}: {
  repository: Repository;
  /** How many rounds a card may take. */
  roundLimit: number;
  card: Card;
  /** What the card is doing (`cardStatus`). */
  status: CardStatus;
  /** The card's change, if it has one (`changeOf`). */
  change: Change | undefined;
  /** The card is the one the board starts next. */
  next: boolean;
  /** The box that chooses the card, in its corner; left out on the drag overlay. */
  select?: ReactNode;
  /** The fields Display switched off. */
  hidden: ReadonlySet<string>;
  /** Opens the card; left out on the drag overlay, where the card is only a picture of itself. */
  onOpen?: () => void;
  /** What the card's menu does; left out on the drag overlay, with the menu itself. */
  menu?: CardMenuActions;
  /** The sub-issues of the card, when it has any: how many are done, and the control that opens them. */
  stack?: Stack;
  open?: boolean;
  onToggleStack?: () => void;
  /** The card lies in the stack of its parent, in a column that is not the one it is drawn in: it says which. */
  inStack?: boolean;
}) {
  const coding = card.column === "In progress" || card.column === "In review";
  const detail = cardDetail(card, roundLimit);
  return (
    <KanbanCardContent
      displayId={cardName(card, repository.forge !== "none")}
      issueUrl={issueUrl(repository, card)}
      title={card.title}
      preview={hidden.has("description") ? null : card.body}
      detail={
        hidden.has("detail")
          ? null
          : inStack
            ? [card.column, detail].filter(Boolean).join(" · ")
            : detail
      }
      labels={hidden.has("labels") ? [] : card.labels.filter((l) => !isPlanningLabel(l))}
      priority={hidden.has("priority") ? "none" : card.priority}
      due={hidden.has("due") ? null : card.due}
      overdue={card.due !== null && card.due < todayIso() && !isClosed(card)}
      claimedBy={hidden.has("claim") ? null : card.claimed_by}
      assignees={card.assignees}
      stack={
        stack && onToggleStack ? (
          <StackToggle card={card} stack={stack} open={open} onToggle={onToggleStack} />
        ) : stack ? (
          <p className="m-0 text-sm text-secondary">{stackWords(stack)}</p>
        ) : undefined
      }
      badges={cardBadges(card, status, next)}
      pullRequest={hidden.has("pullRequest") ? null : pullRequestOf(repository, change)}
      rounds={coding && !hidden.has("rounds") ? { done: card.rounds, limit: roundLimit } : null}
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
      onKeyDown={keep}
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
