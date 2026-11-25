/**
 * Choosing several cards on the board: the choice (`useSelection`, in the order the page shows the cards), the commands
 * for all of them (`bulkCommands`), what each does — one request per card, all sent at once, and one sentence that
 * says how it went — and the two places the commands appear: the bar at the foot and the right-click menu. The page
 * calls `useBulk` with the cards in view and draws `bulk.surface`; a card or a row takes its box and its right click
 * from `bulk.boxFor` and `bulk.onContextMenu`. A card that a request refused stays chosen, so it can be tried again;
 * the rest are let go.
 */
import { useQueryClient } from "@tanstack/react-query";
import { type MouseEvent, type ReactNode, useCallback, useMemo } from "react";
import { approve, claim, merge, release, retry, unblock } from "../../api/calls/board";
import { stopRun } from "../../api/calls/chat";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { putCards } from "../../live/cardCache";
import { ContextMenu, useContextMenu } from "../../ui/components/ContextMenu";
import { useSelection } from "../../ui/lib/useSelection";
import { SelectBox } from "../../ui/page/SelectBox";
import { SelectionBar } from "../../ui/page/SelectionBar";
import { type BulkActions, bulkCommands } from "./bulkCommands";
import type { Notice } from "./decisions";
import { dropRule, inboxOf } from "./model";

const numbers = (cards: readonly Card[]) => {
  const list = cards.map((card) => `#${card.number}`);
  return list.length < 2 ? list.join("") : `${list.slice(0, -1).join(", ")} and ${list.at(-1)}`;
};

export function useBulk({
  board,
  inbox,
  order,
  onNotice,
  onBlock,
}: {
  board: Board | undefined;
  inbox: readonly InboxItem[];
  /** The ids of the cards in view that can be chosen, in the order they are drawn. */
  order: readonly number[];
  onNotice: (notice: Notice) => void;
  /** Asks why the cards are to be blocked; the page opens its dialog, which blocks them all with one reason. */
  onBlock: (cards: Card[]) => void;
}) {
  const queryClient = useQueryClient();
  const choice = useSelection(order);
  const menu = useContextMenu();
  const { clear } = choice;
  const chosen = useMemo(
    () =>
      choice.keys.flatMap((id) => {
        const found = board?.cards.find((card) => card.id === id);
        return found ? [found] : [];
      }),
    [board, choice.keys],
  );

  const send = useCallback(
    async (
      cards: Card[],
      call: (card: Card) => Promise<unknown>,
      said: { done: string; failed: string },
    ) => {
      const results = await Promise.allSettled(cards.map(call));
      putCards(
        queryClient,
        results.flatMap((result) => (result.status === "fulfilled" ? [result.value] : [])),
      );
      const failed = results.flatMap((result, at) =>
        result.status === "rejected" && cards[at]
          ? [{ card: cards[at], error: result.reason }]
          : [],
      );
      if (failed.length > 0) {
        void queryClient.invalidateQueries({ queryKey: keys.snapshot });
        void queryClient.invalidateQueries({ queryKey: keys.cards });
      }
      if (failed.length === 0) {
        onNotice({ tone: "ok", text: `${said.done} ${numbers(cards)}.` });
        clear();
        return;
      }
      const first = failed[0];
      const reason = isApiError(first?.error) ? first.error.message : "something went wrong";
      onNotice({
        tone: "error",
        text: `Could not ${said.failed} ${numbers(failed.map((one) => one.card))}: ${reason}`,
      });
    },
    [queryClient, onNotice, clear],
  );

  const actions = useMemo<BulkActions>(
    () => ({
      claim: (cards) =>
        void send(cards, (c) => claim(c.id), { done: "Assigned", failed: "assign" }),
      release: (cards) =>
        void send(cards, (c) => release(c.id), { done: "Unassigned", failed: "unassign" }),
      block: onBlock,
      unblock: (cards) =>
        void send(cards, (c) => unblock(c.id), { done: "Unblocked", failed: "unblock" }),
      stop: (cards) =>
        void send(cards, (c) => (c.run ? stopRun(c.run.id) : Promise.resolve()), {
          done: "Stopping",
          failed: "stop",
        }),
      move: (cards, column) =>
        void send(
          cards,
          (c) => {
            const drop = dropRule(c, column, board ? inboxOf(inbox, c) : undefined, board);
            if (drop.kind === "approve") return approve(c.id);
            if (drop.kind === "retry") return retry(c.id);
            if (drop.kind === "merge") return merge(drop.change);
            return Promise.reject(new Error("Nobody decides this card now."));
          },
          { done: `Moved to ${column}:`, failed: `move to ${column}` },
        ),
    }),
    [send, onBlock, board, inbox],
  );
  const commands = useMemo(
    () =>
      bulkCommands(chosen, (card) => (board ? inboxOf(inbox, card) : undefined), actions, board),
    [chosen, board, inbox, actions],
  );
  const anyChosen = chosen.length > 0;
  const { ensure } = choice;
  const { open: openMenu } = menu;
  const onContextMenu = useCallback(
    (card: Card) => (event: MouseEvent) => {
      ensure(card.id);
      openMenu(event);
    },
    [ensure, openMenu],
  );
  const title = `${chosen.length} card${chosen.length === 1 ? "" : "s"}`;

  return {
    choice,
    /** The box of a card or a row, to put in its corner. */
    boxFor: (card: Card): ReactNode => (
      <SelectBox
        label={`Choose #${card.number}`}
        checked={choice.has(card.id)}
        anyChosen={anyChosen}
        onToggle={(range) => choice.toggle(card.id, range)}
      />
    ),
    /** Whether anything is chosen: every box shows. */
    anyChosen,
    /** Opens the menu for a right click on a card: it joins the choice it is in, or becomes the choice. */
    onContextMenu,
    /** The bar and the menu; drawn once by the page. */
    surface: (
      <>
        <SelectionBar
          noun="card"
          count={chosen.length}
          commands={commands}
          onClear={choice.clear}
        />
        <ContextMenu point={menu.point} title={title} commands={commands} onClose={menu.close} />
      </>
    ),
  };
}
