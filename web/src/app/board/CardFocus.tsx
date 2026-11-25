/**
 * Puts the card the dock shows in the palette's focus: `cardCommands` says which commands it has, this says what each
 * does. Requests of its own (assign, close) are sent here and read the board again; the decisions, and the questions
 * that need a dialog, are the page's, passed in.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { assignTo, claim, closeIssue, release, reopenIssue } from "../../api/calls/board";
import { people as loadPeople } from "../../api/calls/people";
import type { HasColumns } from "../../api/columns";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Card, InboxItem } from "../../api/types/board";
import { putCards } from "../../live/cardCache";
import { assignable } from "../people/AssigneesProperty";
import { PaletteFocusFor } from "../shell/palette/focus";
import { type CardActions, cardCommands } from "./cardCommands";
import { cardKey } from "./cardKey";
import type { Decision, Notice } from "./decisions";

export function CardFocus({
  card,
  item,
  columns,
  decide,
  onNotice,
  onBlock,
  onAddSubIssue,
  onReject,
}: {
  card: Card;
  item: InboxItem | undefined;
  /** The columns of the card's project. */
  columns?: HasColumns;
  decide: (decision: Decision) => void;
  onNotice: (notice: Notice) => void;
  onBlock: (card: Card) => void;
  onAddSubIssue: (card: Card) => void;
  /** Opens the form that asks why a change is sent back. */
  onReject: (card: Card) => void;
}) {
  const queryClient = useQueryClient();
  const send = useMutation({
    mutationFn: ({ change }: { change: () => Promise<unknown>; text: string }) => change(),
    onSuccess: (answer, { text }) => {
      putCards(queryClient, [answer]);
      onNotice({ tone: "ok", text });
    },
    onError: (error, { text }) => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.cards });
      onNotice({
        tone: "error",
        text: `${text} failed: ${isApiError(error) ? error.message : "something went wrong"}`,
      });
    },
  });
  const { mutate } = send;
  const list = useQuery({ queryKey: keys.people, queryFn: loadPeople });
  const people = useMemo(() => assignable(list.data).map(([handle]) => handle), [list.data]);

  const focus = useMemo(() => {
    const n = (c: Card) => `#${c.number}`;
    const actions: CardActions = {
      claim: (c) => mutate({ change: () => claim(c.id), text: `Assigning ${n(c)}` }),
      release: (c) => mutate({ change: () => release(c.id), text: `Unassigning ${n(c)}` }),
      close: (c) => mutate({ change: () => closeIssue(c.id), text: `Closing ${n(c)}` }),
      reopen: (c) => mutate({ change: () => reopenIssue(c.id), text: `Reopening ${n(c)}` }),
      assignTo: (c, handle, on) =>
        mutate({
          change: () => assignTo(c.id, handle, on),
          text: `${on ? "Assigning" : "Taking off"} @${handle} on ${n(c)}`,
        }),
      block: onBlock,
      unblock: (c) => decide({ kind: "unblock", card: c.id, number: c.number }),
      stop: (c) => {
        if (c.run) decide({ kind: "stop", run: c.run.id, number: c.number });
      },
      addSubIssue: onAddSubIssue,
      approve: (c) => decide({ kind: "approve", card: c.id, number: c.number }),
      retry: (c) => decide({ kind: "retry", card: c.id, number: c.number }),
      merge: (c, change) => decide({ kind: "merge", change, number: c.number }),
      reject: (c) => onReject(c),
    };
    return {
      label: `${cardKey(card)} ${card.title}`,
      commands: cardCommands(card, item, actions, people, columns),
    };
  }, [card, item, columns, people, decide, mutate, onBlock, onAddSubIssue, onReject]);

  return <PaletteFocusFor focus={focus} />;
}
