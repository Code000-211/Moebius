/**
 * The questions a card's menu asks before it acts: why to block it (the reason is optional), whether to delete it, with
 * what that takes and, on a forge, what it leaves, and which issue is to be its parent. Each sends its own request and, as the new-issue form does,
 * keeps what the server refused in the dialog until the person leaves it; the card changes the way every card does,
 * through the event stream and the snapshot. The page holds the dialogs, outside the cards, so that one stays open when
 * the card under it is redrawn or goes.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { memo, useRef, useState } from "react";
import { block, deleteCard, setParent, unblock } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card, Repository } from "../../api/types/board";
import { putCards, removeCard } from "../../live/cardCache";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Field, Select, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { useNotify } from "./decisions";
import { deleteWords, descendants, parentOf } from "./model";

/**
 * The card a dialog is about, kept for as long as the dialog fades out: the page clears its card the moment the
 * dialog closes, and a sheet that empties while it is still on screen flashes.
 */
function useShown(card: Card | null): Card | null {
  const last = useRef(card);
  if (card) last.current = card;
  return card ?? last.current;
}

/** What the server said when it refused, or a sentence for a failure that said nothing. */
function refusal(error: unknown, fallback: string): string {
  return isApiError(error) ? error.message : fallback;
}

export const BlockDialog = memo(function BlockDialog({
  card,
  more = [],
  onClose,
  onDone,
}: {
  /** The card to block; the dialog is open while there is one. */
  card: Card | null;
  /** Cards blocked with it, for one reason, when several were chosen. */
  more?: readonly Card[];
  onClose: () => void;
  /** The server blocked the card: the sentence that says so, and what takes the block off again. */
  onDone: (text: string, undo: () => void) => void;
}) {
  const queryClient = useQueryClient();
  const notify = useNotify();
  const shown = useShown(card);
  const [reason, setReason] = useState("");
  const send = useMutation({
    mutationFn: async (target: Card) => {
      const all = [target, ...more];
      const results = await Promise.allSettled(all.map((one) => block(one.id, reason.trim())));
      putCards(
        queryClient,
        results.flatMap((result) => (result.status === "fulfilled" ? [result.value] : [])),
      );
      const refused = results.find((result) => result.status === "rejected");
      if (refused) {
        void queryClient.invalidateQueries({ queryKey: keys.snapshot });
        throw (refused as PromiseRejectedResult).reason;
      }
    },
    onSuccess: (_, target) => {
      const all = [target, ...more];
      setReason("");
      onClose();
      onDone(
        more.length === 0
          ? `Blocked #${target.number}. Its runs are stopped.`
          : `Blocked ${more.length + 1} cards. Their runs are stopped.`,
        () => {
          // Undo takes the block off again. The runs it stopped stay stopped.
          Promise.all(all.map((one) => unblock(one.id))).then(
            (cards) => putCards(queryClient, cards),
            (error) => {
              void queryClient.invalidateQueries({ queryKey: keys.snapshot });
              notify({ tone: "error", text: refusal(error, "Could not unblock the card.") });
            },
          );
        },
      );
    },
  });
  const leave = () => {
    // Leaving by any door, so the next card does not open on this one's reason or refusal.
    send.reset();
    setReason("");
    onClose();
  };
  return (
    <Dialog open={card !== null} onOpenChange={(open) => !open && leave()}>
      <DialogContent>
        {shown && (
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              if (!send.isPending) send.mutate(shown);
            }}
          >
            <DialogHeader>
              <DialogTitle>
                {more.length === 0 ? `Block #${shown.number}` : `Block ${more.length + 1} cards`}
              </DialogTitle>
              <DialogDescription>
                {more.length === 0
                  ? `Its runs are stopped, and the board does nothing with it until you unblock it. It stays in ${shown.column}.`
                  : "Their runs are stopped, and the board does nothing with them until you unblock them. They stay in their columns."}
              </DialogDescription>
            </DialogHeader>
            <Field label="Reason" hint="Optional. It shows on the card, for whoever opens it.">
              {(id) => (
                <TextArea
                  id={id}
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                  rows={3}
                  autoFocus
                />
              )}
            </Field>
            {send.isError && (
              <Notice tone="error">
                {refusal(send.error, "Could not block the card. Try again.")}
              </Notice>
            )}
            <DialogFooter>
              <Button type="button" variant="ghost" onClick={leave}>
                Cancel
              </Button>
              <Button type="submit" disabled={send.isPending}>
                {send.isPending ? "Blocking" : "Block"}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
});

export const DeleteDialog = memo(function DeleteDialog({
  card,
  repository,
  onClose,
  onDone,
}: {
  /** The card to delete; the dialog is open while there is one. */
  card: Card | null;
  repository: Repository;
  onClose: () => void;
  /** The server deleted the card: the card, and the sentence that says so. */
  onDone: (card: Card, text: string) => void;
}) {
  const queryClient = useQueryClient();
  const shown = useShown(card);
  const send = useMutation({
    mutationFn: (target: Card) => deleteCard(target.id),
    onSuccess: (_, target) => {
      removeCard(queryClient, target.id);
      void queryClient.invalidateQueries({ queryKey: keys.cards });
      onClose();
      onDone(target, `Deleted #${target.number}.`);
    },
  });
  const leave = () => {
    send.reset();
    onClose();
  };
  const words = deleteWords(repository);
  return (
    <Dialog open={card !== null} onOpenChange={(open) => !open && leave()}>
      <DialogContent>
        {shown && (
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              if (!send.isPending) send.mutate(shown);
            }}
          >
            <DialogHeader>
              <DialogTitle>{`Delete #${shown.number}`}</DialogTitle>
              <DialogDescription>{shown.title}</DialogDescription>
            </DialogHeader>
            <p className="m-0 text-base text-text">{words.removes}</p>
            {words.keeps && <p className="m-0 text-base text-text">{words.keeps}</p>}
            {send.isError && (
              <Notice tone="error">
                {refusal(send.error, "Could not delete the card. Try again.")}
              </Notice>
            )}
            <DialogFooter>
              <Button type="button" variant="ghost" onClick={leave} autoFocus>
                Cancel
              </Button>
              <Button type="submit" variant="danger" disabled={send.isPending}>
                {send.isPending ? "Deleting" : "Delete card"}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
});

/** The issues `card` may be moved below: every other issue of the board but its own sub-issues, which would make a circle. */
export function parentChoices(board: Board, card: Card): Card[] {
  const below = new Set(descendants(board, card).map((c) => c.id));
  return board.cards
    .filter((c) => c.id !== card.id && !below.has(c.id))
    .sort((a, b) => a.number - b.number);
}

/** The words of an issue in a list of choices. */
export const issueWords = (c: Card) => `#${c.number} ${c.title}`;

export const ParentDialog = memo(function ParentDialog({
  board,
  card,
  onClose,
  onDone,
}: {
  board: Board;
  /** The card to move; the dialog is open while there is one. */
  card: Card | null;
  onClose: () => void;
  /** The server moved the card: the sentence that says so. */
  onDone: (text: string) => void;
}) {
  const queryClient = useQueryClient();
  const shown = useShown(card);
  // The number of the chosen parent, "none" for no parent; empty until a choice, which is the parent it has now.
  const [chosen, setChosen] = useState<string | null>(null);
  const send = useMutation({
    mutationFn: ({ target, parent }: { target: Card; parent: number | null }) =>
      setParent(target.id, parent),
    onSuccess: (answer, { target, parent }) => {
      putCards(queryClient, [answer]);
      setChosen(null);
      onClose();
      onDone(
        parent === null
          ? `#${target.number} is not a sub-issue any more.`
          : `#${target.number} is a sub-issue of #${parent}.`,
      );
    },
  });
  const leave = () => {
    send.reset();
    setChosen(null);
    onClose();
  };
  const current = shown ? parentOf(board, shown) : undefined;
  const value = chosen ?? (current ? String(current.number) : "none");
  return (
    <Dialog open={card !== null} onOpenChange={(open) => !open && leave()}>
      <DialogContent>
        {shown && (
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              if (!send.isPending) {
                send.mutate({ target: shown, parent: value === "none" ? null : Number(value) });
              }
            }}
          >
            <DialogHeader>
              <DialogTitle>{`Move #${shown.number} to another parent`}</DialogTitle>
              <DialogDescription>
                {shown.title}. It lies in its parent's stack on the board.
              </DialogDescription>
            </DialogHeader>
            <Field label="Parent">
              {(id) => (
                <Select
                  id={id}
                  value={value}
                  options={[
                    ["none", "No parent"],
                    ...parentChoices(board, shown).map((c): [string, string] => [
                      String(c.number),
                      issueWords(c),
                    ]),
                  ]}
                  onChange={setChosen}
                />
              )}
            </Field>
            {send.isError && (
              <Notice tone="error">
                {refusal(send.error, "Could not move the issue. Try again.")}
              </Notice>
            )}
            <DialogFooter>
              <Button type="button" variant="ghost" onClick={leave}>
                Cancel
              </Button>
              <Button type="submit" disabled={send.isPending}>
                {send.isPending ? "Moving" : "Move"}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
});
