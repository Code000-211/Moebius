/**
 * One column of the board with its cards and their stacks of sub-issues, and one card as a draggable thing in it. What a
 * card shows is `BoardCardContent.tsx`; `BoardPage.tsx` lays the columns out and decides what a drop does.
 */
import type { Board, Card, InboxItem } from "../../api/types/board";
import {
  KanbanBoard,
  KanbanCard,
  KanbanCards,
  KanbanHeader,
} from "../../ui/components/KanbanBoard";
import { Empty } from "../../ui/page/Empty";
import { CardContent, type CardMenuActions } from "./BoardCardContent";
import type { useBulk } from "./bulk";
import { ColumnAdd, ColumnAddButton } from "./ColumnAdd";
import { ColumnMark } from "./ColumnMark";
import { cardKey } from "./cardKey";
import { cardStatus, inboxOf, type Stack } from "./model";

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

/** What chooses cards (`useBulk`): the box of a card and the menu of a right click. */
type Bulk = Pick<ReturnType<typeof useBulk>, "boxFor" | "onContextMenu">;

const ANY_COLUMN = "A card arrives when the column's script moves one in.";

export function BoardColumn({
  board,
  inbox,
  name,
  cards,
  total,
  stacks,
  stackOpen,
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
  stackOpen: (card: Card) => boolean;
  onToggleStack: (card: Card) => void;
  /** While a decision is on its way, when no card can be moved. */
  busy: boolean;
  inspectedId: number | undefined;
  onSelect: (card: Card) => void;
  menu: CardMenuActions;
  bulk: Bulk;
  /** The fields of a card Display switched off. */
  hidden: ReadonlySet<string>;
  /** The line for a new card is open in this column. */
  adding: boolean;
  onAdding: (open: boolean) => void;
  onCreated: (card: Card, column: string) => void;
}) {
  return (
    <KanbanBoard name={name}>
      <KanbanHeader
        name={name}
        count={cards.length}
        total={total}
        mark={<ColumnMark name={name} />}
        actions={<ColumnAddButton column={name} onClick={() => onAdding(true)} />}
      />
      <KanbanCards className="min-h-9">
        {cards.map((card) => {
          const stack = stacks.get(card.id);
          const open = stack !== undefined && stackOpen(card);
          return (
            <div
              key={card.id}
              data-testid={stack ? `stack-${card.number}` : undefined}
              className="flex min-w-0 flex-col gap-3"
            >
              <BoardCard
                board={board}
                inbox={inbox}
                card={card}
                column={name}
                cards={cards}
                busy={busy}
                inspectedId={inspectedId}
                onSelect={onSelect}
                menu={menu}
                bulk={bulk}
                stack={stack}
                open={open}
                onToggleStack={() => onToggleStack(card)}
                hidden={hidden}
              />
              {open && stack.shown.length > 0 && (
                // The sub-issues lie under their parent, in a stack of their own: each is a card that moves, blocks and
                // deletes like any other, and says which column it is in, since it is not in this one.
                <div className="ml-3 flex min-w-0 flex-col gap-3 border-l-2 border-hairline-strong pl-3">
                  {stack.shown.map((child) => (
                    <BoardCard
                      key={child.id}
                      board={board}
                      inbox={inbox}
                      card={child}
                      column={child.column}
                      cards={board.cards}
                      busy={busy}
                      inspectedId={inspectedId}
                      onSelect={onSelect}
                      menu={menu}
                      inStack
                      hidden={hidden}
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
      <ColumnAdd
        repository={board.repository.id}
        column={name}
        open={adding}
        onOpen={() => onAdding(true)}
        onClose={() => onAdding(false)}
        onCreated={onCreated}
      />
    </KanbanBoard>
  );
}

/** One card of a column, or of the stack under its parent. */
function BoardCard({
  board,
  inbox,
  card,
  column,
  cards,
  busy,
  inspectedId,
  onSelect,
  menu,
  bulk,
  stack,
  open = false,
  onToggleStack,
  inStack = false,
  hidden,
}: {
  board: Board;
  inbox: InboxItem[];
  card: Card;
  hidden: ReadonlySet<string>;
  /** What chooses the card: left out for a sub-issue in a stack, which is not chosen with the cards of the column. */
  bulk?: Bulk;
  /** The column the card is in: the one it is drawn in, or its own when it lies in the stack of its parent. */
  column: string;
  cards: Card[];
  busy: boolean;
  inspectedId: number | undefined;
  onSelect: (card: Card) => void;
  menu: CardMenuActions;
  stack?: Stack;
  open?: boolean;
  onToggleStack?: () => void;
  inStack?: boolean;
}) {
  const status = cardStatus(card, inboxOf(inbox, card), board);
  return (
    <KanbanCard
      id={String(card.id)}
      name={`${cardKey(card)} ${card.title}`}
      testId={`card-${card.number}`}
      column={column}
      dragDisabled={busy}
      isSelected={inspectedId === card.id}
      isLive={status === "live"}
      onOpen={() => onSelect(card)}
      onContextMenu={bulk?.onContextMenu(card)}
    >
      <CardContent
        board={board}
        inbox={inbox}
        card={card}
        cards={cards}
        onOpen={() => onSelect(card)}
        menu={menu}
        select={bulk?.boxFor(card)}
        stack={stack}
        open={open}
        onToggleStack={onToggleStack}
        inStack={inStack}
        hidden={hidden}
      />
    </KanbanCard>
  );
}
