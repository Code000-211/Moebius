/**
 * The card window for the whole app. The shell mounts it, and it shows the card the address names — `?card=<id>` on any
 * route: over the board, Inbox, Fleet or any page — as a centred window (`CardWindow.tsx`) over a dimmed page. A page
 * opens a card with `useOpenCard()`, which sets `?card` on the page it is on and leaves the rest of the address as it
 * was, so the page behind keeps its place and its width, and a reload or a link opens the same card again. Closing drops
 * the parameter. The host owns what the window's menu and buttons ask of the page: the decisions, the questions of
 * blocking, deleting and moving to another parent, the form of a sub-issue, and the card in the palette's focus.
 */
import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocation, useSearchParams } from "react-router";
import { snapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import { BlockDialog, DeleteDialog, ParentDialog } from "./CardDialogs";
import { CardFocus } from "./CardFocus";
import { CardWindow } from "./CardWindow";
import { withCard } from "./cardAddress";
import { useDecisions } from "./decisions";
import { inboxOf, mergeStoppedOf } from "./model";
import { useNewIssue } from "./NewIssueHost";

/**
 * Opens a card as the window over the page on screen: a new history entry, so Back closes it. With `askToReject` the
 * window opens with the question why a change is sent back already asked (a drop of the card on Coding).
 */
export function useOpenCard(): (id: number, options?: { askToReject?: boolean }) => void {
  const [, setParams] = useSearchParams();
  return useCallback(
    (id: number, options?: { askToReject?: boolean }) =>
      setParams(
        (now) => withCard(now, id),
        options?.askToReject ? { state: { rejectCard: id } } : undefined,
      ),
    [setParams],
  );
}

export function CardWindowHost() {
  const [params, setParams] = useSearchParams();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const { decide, pending, notify } = useDecisions();
  const newIssue = useNewIssue();
  const open = useOpenCard();
  // The card a send-back needs a reason for; the window asks for it, for that card alone.
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

  const asked = params.get("card");
  const id = asked === null ? null : Number(asked);
  const boards = data.data?.boards;
  const inbox = data.data?.inbox;
  const board = boards?.find((b) => b.cards.some((c) => c.id === id));
  const card = board?.cards.find((c) => c.id === id);

  const close = useCallback(() => {
    setParams((now) => withCard(now, null), { replace: true });
    setRejectFor(null);
  }, [setParams]);
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
  const onOpenCard = useCallback((next: Card) => open(next.id), [open]);

  return (
    <>
      {board && card && inbox && (
        <>
          <CardWindow
            key={card.id}
            board={board}
            card={card}
            item={inboxOf(inbox, card)}
            stopped={mergeStoppedOf(inbox, card)}
            boards={boards}
            busy={pending}
            askFor={rejectFor}
            onAskAnswered={clearReject}
            onDecide={decide}
            menu={menu}
            onOpenCard={onOpenCard}
            onClose={close}
          />
          <CardFocus
            card={card}
            item={inboxOf(inbox, card)}
            decide={decide}
            onNotice={notify}
            onBlock={setBlocking}
            onAddSubIssue={addSubIssue}
            onReject={(c) => setRejectFor(c.id)}
          />
        </>
      )}
      {board && (
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
              close();
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
      )}
    </>
  );
}
