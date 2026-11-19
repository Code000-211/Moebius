/**
 * Puts the card the dock shows in the palette's focus: `cardCommands` says which commands it has, this says what each
 * does. Requests of its own (assign, close) are sent here and read the board again; the decisions, and the questions
 * that need a dialog, are the page's, passed in.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { claim, closeIssue, release, reopenIssue, setAssignees } from "../../api/calls/board";
import { people as loadPeople } from "../../api/calls/people";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Card, InboxItem } from "../../api/types/board";
import { assignable } from "../people/AssigneesProperty";
import { PaletteFocusFor } from "../shell/palette/focus";
import { type CardActions, cardCommands } from "./cardCommands";
import type { Decision, Notice } from "./decisions";

export function CardFocus({
  card,
  item,
  decide,
  onNotice,
  onBlock,
  onAddSubIssue,
  onReject,
}: {
  card: Card;
  item: InboxItem | undefined;
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
    onSuccess: (_, { text }) => onNotice({ tone: "ok", text }),
    onError: (error, { text }) =>
      onNotice({
        tone: "error",
        text: `${text} failed: ${isApiError(error) ? error.message : "something went wrong"}`,
      }),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.cards });
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
          change: () =>
            setAssignees(
              c.id,
              on ? [...c.assignees, handle] : c.assignees.filter((h) => h !== handle),
            ),
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
      label: `#${card.number} ${card.title}`,
      commands: cardCommands(card, item, actions, people),
    };
  }, [card, item, people, decide, mutate, onBlock, onAddSubIssue, onReject]);

  return <PaletteFocusFor focus={focus} />;
}
