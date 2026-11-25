/**
 * The card window for the whole app. The shell mounts it, and it shows the card the address names — `?card=<id>` on any
 * route: over the board, Inbox, Fleet or any page — as a centred window (`CardWindow.tsx`) over a dimmed page. A page
 * opens a card with `useOpenCard()`, which sets `?card` on the page it is on and leaves the rest of the address as it
 * was, so the page behind keeps its place and its width, and a reload or a link opens the same card again. Closing drops
 * the parameter. The host owns what the window's menu and buttons ask of the page: the decisions, the questions of
 * blocking, deleting and moving to another parent, the form of a sub-issue, and the card in the palette's focus.
 */
import { useQuery } from "@tanstack/react-query";
import { useCallback } from "react";
import { useSearchParams } from "react-router";
import { snapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import { CardWindow } from "./CardWindow";
import { withCard } from "./cardAddress";
import { inboxOf, mergeStoppedOf } from "./model";
import { useCardActions } from "./useCardActions";

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
  const open = useOpenCard();
  // The actions ask for the window to close when a card they deleted was the one shown.
  const actions = useCardActions(() => close());
  const { resetReject } = actions;
  const close = useCallback(() => {
    setParams((now) => withCard(now, null), { replace: true });
    resetReject();
  }, [setParams, resetReject]);

  const asked = params.get("card");
  const id = asked === null ? null : Number(asked);
  const boards = data.data?.boards;
  const inbox = data.data?.inbox;
  const board = boards?.find((b) => b.cards.some((c) => c.id === id));
  const card = board?.cards.find((c) => c.id === id);
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
            busy={actions.busy}
            askFor={actions.askFor}
            onAskAnswered={actions.clearReject}
            onDecide={actions.decide}
            menu={actions.menu}
            onOpenCard={onOpenCard}
            onClose={close}
          />
          {actions.focus(card, inboxOf(inbox, card), board)}
        </>
      )}
      {board && actions.dialogs(board)}
    </>
  );
}
