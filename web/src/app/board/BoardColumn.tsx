/**
 * One column of the board with its cards and their stacks of sub-issues, and one card as a draggable thing in it. What a
 * card shows is `BoardCardContent.tsx`; `BoardPage.tsx` lays the columns out and decides what a drop does.
 *
 * A column and a card are memoised. A column works out for each card what the card shows that depends on the board (its
 * status, its change, whether it is the next to start) and hands the card those, with callbacks that keep their identity,
 * so a card draws again only when its own data, one of those or whether it is chosen or open changes: a read of the
 * snapshot that changes one card, or opening another, no longer draws every card of the board.
 */
import { type MouseEvent, memo } from "react";
import type { Board, Card, Change, InboxItem, Repository } from "../../api/types/board";
import {
  KanbanBoard,
  KanbanCard,
  KanbanCards,
  KanbanHeader,
} from "../../ui/components/KanbanBoard";
import { Empty } from "../../ui/page/Empty";
import { SelectBox } from "../../ui/page/SelectBox";
import { CardContent, type CardMenuActions } from "./BoardCardContent";
import { ColumnAdd, ColumnAddButton } from "./ColumnAdd";
import { ColumnMark } from "./ColumnMark";
import { cardKey } from "./cardKey";
import { type CardStatus, cardStatus, changeOf, inboxOf, nextCard, type Stack } from "./model";

/**
 * What a column holds when it is empty: one sentence for how a card comes to it. There is no title, because the
 * column's header is already the name of the place, and a heading repeating it is a second label on the same thing.
 */
const COLUMN_EMPTY: Record<string, string> = {
  Backlog: "Issues you create wait here until they are approved.",
  "Not started": "Approved issues wait here until an agent takes one.",
  "In progress": "A card arrives here when an agent starts a run on it.",
  "In review": "A card arrives here when an agent has a change to review.",
  "Ready to merge": "A card arrives here when its change passes its checks.",
  Done: "A card arrives here once its change is merged.",
};

/**
 * What chooses cards: the choice and the right click of the page's `useBulk`, by what a card needs of it. The object keeps
 * its identity until the choice changes.
 */
export interface ColumnBulk {
  has: (id: number) => boolean;
  anyChosen: boolean;
  toggle: (id: number, range?: boolean) => void;
  onContextMenu: (card: Card) => (event: MouseEvent) => void;
}

const ANY_COLUMN = "A card arrives when the column's script moves one in.";

export const BoardColumn = memo(function BoardColumn({
  board,
  inbox,
  name,
  cards,
  total,
  stacks,
  opened,
  inspectedTopId,
  onToggleStack,
  busy,
  inspectedId,
  onSelect,
  menu,
  bulk,
  hidden,
  adding,
  onAdding,
  onCreated,
}: {
  board: Board;
  inbox: InboxItem[];
  name: string;
  cards: Card[];
  total: number;
  /** The stack of each card of the board that has sub-issues, by card id. */
  stacks: Map<number, Stack>;
  /** The stacks a person opened, by the id of the card at the top. */
  opened: ReadonlySet<number>;
  /** The card at the top of the stack the dock shows a card of: that stack is open while the dock is. */
  inspectedTopId: number | undefined;
  onToggleStack: (card: Card) => void;
  /** While a decision is on its way, when no card can be moved. */
  busy: boolean;
  inspectedId: number | undefined;
  onSelect: (card: Card) => void;
  menu: CardMenuActions;
  bulk: ColumnBulk;
  /** The fields of a card Display switched off. */
  hidden: ReadonlySet<string>;
  /** The line for a new card is open in this column. */
  adding: boolean;
  /** Opens or closes the line for a new card in the column of this name. */
  onAdding: (column: string, open: boolean) => void;
  onCreated: (card: Card, column: string) => void;
}) {
  const next = name === "Not started" ? nextCard(cards, board.cards) : undefined;
  // A sub-issue in a stack is the next to start when it is the first the whole board could start; worked out only when a
  // stack is open.
  let nextAnywhere: number | undefined | null = null;
  const nextOfBoard = () => {
    if (nextAnywhere === null) nextAnywhere = nextCard(board.cards, board.cards);
    return nextAnywhere;
  };
  const shared = {
    repository: board.repository,
    roundLimit: board.round_limit,
    busy,
    onSelect,
    menu,
    hidden,
  };
  /** What a card of this column shows that depends on the board. */
  const derived = (card: Card) => ({
    status: cardStatus(card, inboxOf(inbox, card), board),
    change: changeOf(board, card),
  });
  return (
    <KanbanBoard name={name}>
      <ColumnHead name={name} count={cards.length} total={total} onAdding={onAdding} />
      <KanbanCards className="min-h-9">
        {cards.map((card) => {
          const stack = stacks.get(card.id);
          const open = stack !== undefined && (opened.has(card.id) || inspectedTopId === card.id);
          return (
            <div
              key={card.id}
              data-testid={stack ? `stack-${card.number}` : undefined}
              className="flex min-w-0 flex-col gap-3"
            >
              <BoardCard
                {...shared}
                {...derived(card)}
                card={card}
                selected={inspectedId === card.id}
                column={name}
                next={next === card.id}
                bulk={bulk}
                chosen={bulk.has(card.id)}
                stack={stack}
                open={open}
                onToggleStack={onToggleStack}
              />
              {open && stack.shown.length > 0 && (
                // The sub-issues lie under their parent, in a stack of their own: each is a card that moves, blocks and
                // deletes like any other, and says which column it is in, since it is not in this one.
                <div className="ml-3 flex min-w-0 flex-col gap-3 border-l-2 border-hairline-strong pl-3">
                  {stack.shown.map((child) => (
                    <BoardCard
                      key={child.id}
                      {...shared}
                      {...derived(child)}
                      card={child}
                      selected={inspectedId === child.id}
                      column={child.column}
                      next={child.column === "Not started" && nextOfBoard() === child.id}
                      inStack
                    />
                  ))}
                </div>
              )}
            </div>
          );
        })}
        {/* The column is already the surface under these words; a card of its own inside it is a second one, and one
            that reads as a card which has lost its words. */}
        {cards.length === 0 && <Empty plain centered body={COLUMN_EMPTY[name] ?? ANY_COLUMN} />}
      </KanbanCards>
      <ColumnFoot
        repository={board.repository.id}
        name={name}
        adding={adding}
        onAdding={onAdding}
        onCreated={onCreated}
      />
    </KanbanBoard>
  );
});

/** The head of a column, which draws again only when the column's counts change. */
const ColumnHead = memo(function ColumnHead({
  name,
  count,
  total,
  onAdding,
}: {
  name: string;
  count: number;
  total: number;
  onAdding: (column: string, open: boolean) => void;
}) {
  return (
    <KanbanHeader
      name={name}
      count={count}
      total={total}
      mark={<ColumnMark name={name} />}
      actions={<ColumnAddButton column={name} onClick={() => onAdding(name, true)} />}
    />
  );
});

/** The foot of a column: "+ Add", or the line it turns into while it is open. */
const ColumnFoot = memo(function ColumnFoot({
  repository,
  name,
  adding,
  onAdding,
  onCreated,
}: {
  repository: number;
  name: string;
  adding: boolean;
  onAdding: (column: string, open: boolean) => void;
  onCreated: (card: Card, column: string) => void;
}) {
  return (
    <ColumnAdd
      repository={repository}
      column={name}
      open={adding}
      onOpen={() => onAdding(name, true)}
      onClose={() => onAdding(name, false)}
      onCreated={onCreated}
    />
  );
});

type BoardCardProps = {
  repository: Repository;
  roundLimit: number;
  card: Card;
  /** What the card is doing, and its change: worked out by the column from the board. */
  status: CardStatus;
  change: Change | undefined;
  /** The card is the one the board starts next. */
  next: boolean;
  hidden: ReadonlySet<string>;
  /** What chooses the card: left out for a sub-issue in a stack, which is not chosen with the cards of the column. */
  bulk?: ColumnBulk;
  /** Whether the card is chosen; only a card the column can choose says. */
  chosen?: boolean;
  /** The column the card is in: the one it is drawn in, or its own when it lies in the stack of its parent. */
  column: string;
  busy: boolean;
  /** The dock shows this card. */
  selected: boolean;
  onSelect: (card: Card) => void;
  menu: CardMenuActions;
  stack?: Stack;
  open?: boolean;
  onToggleStack?: (card: Card) => void;
  inStack?: boolean;
};

/** Whether a stack reads the same: the page makes a new one at every read of the board, with the same cards in it. */
function sameStack(a: Stack | undefined, b: Stack | undefined): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return (
    a.done === b.done &&
    a.total === b.total &&
    a.shown.length === b.shown.length &&
    a.shown.every((card, at) => card === b.shown[at])
  );
}

/**
 * Whether a card draws the same. Every prop is compared by identity, but for the stack, which is rebuilt with the same
 * content whenever the board is read again, and the choice, of which the card takes only whether it is in it.
 */
function sameCard(was: BoardCardProps, now: BoardCardProps): boolean {
  for (const key of Object.keys(now) as (keyof BoardCardProps)[]) {
    if (key === "stack" || key === "bulk") continue;
    if (!Object.is(was[key], now[key])) {
      return false;
    }
  }
  if (!sameStack(was.stack, now.stack)) return false;
  // The box shows on every card once any is chosen; its handlers do not change what a card draws.
  return (
    was.bulk?.anyChosen === now.bulk?.anyChosen &&
    (was.bulk === undefined) === (now.bulk === undefined)
  );
}

/** One card of a column, or of the stack under its parent. */
const BoardCard = memo(function BoardCard({
  repository,
  roundLimit,
  card,
  status,
  change,
  next,
  column,
  busy,
  selected,
  onSelect,
  menu,
  bulk,
  chosen = false,
  stack,
  open = false,
  onToggleStack,
  inStack = false,
  hidden,
}: BoardCardProps) {
  return (
    <KanbanCard
      id={String(card.id)}
      name={`${cardKey(card)} ${card.title}`}
      testId={`card-${card.number}`}
      column={column}
      dragDisabled={busy}
      isSelected={selected}
      isLive={status === "live"}
      onOpen={() => onSelect(card)}
      onContextMenu={bulk?.onContextMenu(card)}
    >
      <CardContent
        repository={repository}
        roundLimit={roundLimit}
        card={card}
        status={status}
        change={change}
        next={next}
        onOpen={() => onSelect(card)}
        menu={menu}
        select={
          bulk && (
            <SelectBox
              label={`Choose #${card.number}`}
              checked={chosen}
              anyChosen={bulk.anyChosen}
              onToggle={(range) => bulk.toggle(card.id, range)}
            />
          )
        }
        stack={stack}
        open={open}
        onToggleStack={onToggleStack && (() => onToggleStack(card))}
        inStack={inStack}
        hidden={hidden}
      />
    </KanbanCard>
  );
}, sameCard);
