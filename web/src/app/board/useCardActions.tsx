/**
 * What a card's buttons and menu ask of the page, shared by the window over any page (`CardWindowHost.tsx`) and the
 * card's own page (`CardKeyPage.tsx`): the decisions, the questions of blocking, deleting and moving to another
 * parent, the form of a sub-issue, and the card in the palette's focus. `dialogs` and `focus` are drawn beside the card.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocation } from "react-router";
import type { HasColumns } from "../../api/columns";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { BlockDialog, DeleteDialog, ParentDialog } from "./CardDialogs";
import { CardFocus } from "./CardFocus";
import { useDecisions } from "./decisions";
import { useNewIssue } from "./NewIssueHost";

export function useCardActions(onDeleted: () => void) {
  const { decide, pending, notify } = useDecisions();
  const newIssue = useNewIssue();
  // The card a send-back needs a reason for; the card asks for it, for that card alone.
  const [rejectFor, setRejectFor] = useState<number | null>(null);
  // The card a menu asked to block, delete or move below another, until the question is answered.
  const [blocking, setBlocking] = useState<Card | null>(null);
  const [deleting, setDeleting] = useState<Card | null>(null);
  const [moving, setMoving] = useState<Card | null>(null);

  // A drop that needs a reason opened the card with the question: it is asked once the card is on screen.
  const rejectCard = (useLocation().state as { rejectCard?: number } | null)?.rejectCard ?? null;
  useEffect(() => {
    if (rejectCard !== null) setRejectFor(rejectCard);
  }, [rejectCard]);

  const clearReject = useCallback(() => setRejectFor(null), []);
  const addSubIssue = useCallback(
    (parent: Card) =>
      newIssue({
        repository: parent.repository_id,
        parent: parent.number,
        onCreated: (made) => notify({ tone: "ok", text: `Created "${made.title}"` }),
      }),
    [newIssue, notify],
  );
  const menu = useMemo(
    () => ({
      onBlock: setBlocking,
      onUnblock: (c: Card) => decide({ kind: "unblock", card: c.id, number: c.number }),
      onStop: (c: Card) => {
        if (c.run) decide({ kind: "stop", run: c.run.id, number: c.number });
      },
      onDelete: setDeleting,
      onAddSubIssue: addSubIssue,
      onMoveParent: setMoving,
    }),
    [decide, addSubIssue],
  );

  return {
    menu,
    decide,
    busy: pending,
    askFor: rejectFor,
    clearReject,
    /** Forgets a pending question, such as when the window closes. */
    resetReject: clearReject,
    /** The key handling of the card in focus. */
    focus: (card: Card, item: InboxItem | undefined, columns?: HasColumns) => (
      <CardFocus
        card={card}
        item={item}
        columns={columns}
        decide={decide}
        onNotice={notify}
        onBlock={setBlocking}
        onAddSubIssue={addSubIssue}
        onReject={(c) => setRejectFor(c.id)}
      />
    ),
    dialogs: (board: Board) => (
      <>
        <BlockDialog
          card={blocking}
          onClose={() => setBlocking(null)}
          onDone={(text, undo) => notify({ tone: "ok", text, undo })}
        />
        <DeleteDialog
          card={deleting}
          repository={board.repository}
          onClose={() => setDeleting(null)}
          onDone={(_, text) => {
            onDeleted();
            notify({ tone: "ok", text });
          }}
        />
        <ParentDialog
          board={board}
          card={moving}
          onClose={() => setMoving(null)}
          onDone={(text) => notify({ tone: "ok", text })}
        />
      </>
    ),
  };
}
