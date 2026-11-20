/**
 * The issue in the dock besides what the right column says of it (`CardProperties.tsx`): its activity (comments and history as one timeline), and the edit of
 * its title and body. Each is a request to the server (`api/calls/board.ts`); the card shows it when the event of the
 * change makes the snapshot read again, and what the server refuses is said beside the control that asked.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { Link } from "react-router";
import { comment, comments, editCard, history } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import { CardScope } from "../../ui/chat/CardLinks";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Input } from "../../ui/components/Input";
import { KanbanIssueSection } from "../../ui/components/KanbanIssuePanel";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { MentionTextArea } from "../people/MentionTextArea";
import { CardText } from "./CardText";
import { foldLabel, type TimelineEvent, timelineOf } from "./timeline";

/** What the server said when it refused, or a sentence for a failure that said nothing. */
const said = (error: unknown, fallback: string) => (isApiError(error) ? error.message : fallback);

/** A refusal, under the control that asked, or nothing. */
function Refusal({ text }: { text: string | null }): ReactNode {
  return text ? <Notice tone="error">{text}</Notice> : null;
}

/** One line of the timeline: a run opens its page, a refusal is marked, a change is plain. */
function EventLine({ event, now }: { event: TimelineEvent; now: number }) {
  return (
    <div className="flex min-w-0 items-baseline gap-3 text-sm text-secondary">
      <span
        aria-hidden="true"
        className={`mt-1.5 size-1.5 shrink-0 rounded-full ${event.tone === "refusal" ? "bg-error" : event.tone === "run" ? "bg-text" : "bg-hairline-strong"}`}
      />
      <span className="min-w-0 flex-1 [overflow-wrap:anywhere]">
        {event.text}
        {event.run !== null && (
          <>
            {" · "}
            <Link
              to={`/runs/${event.run}`}
              className="text-text underline decoration-hairline-strong underline-offset-4 hover:decoration-current"
            >
              {`View run ${event.run}`}
            </Link>
          </>
        )}
      </span>
      <span className="shrink-0 tabular-nums">{ago(now, event.at)}</span>
    </div>
  );
}

/** The changes a fold holds, one line each. */
function FoldList({ events, now }: { events: TimelineEvent[]; now: number }) {
  return (
    // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none; WebKit then drops the list
    <ol role="list" className="m-0 ml-6 flex list-none flex-col gap-1 p-0">
      {events.map((event, index) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: the changes of a fold are a fixed, ordered list
        <li key={index} className="min-w-0">
          <EventLine event={event} now={now} />
        </li>
      ))}
    </ol>
  );
}

/** Small changes folded into one line that opens to the changes it holds. */
function Fold({ events, now }: { events: TimelineEvent[]; now: number }) {
  const [open, setOpen] = useState(false);
  const last = events[events.length - 1] as TimelineEvent;
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex min-h-7 w-full items-baseline gap-3 rounded-control text-left text-sm text-secondary hover:text-text"
      >
        <span aria-hidden="true" className="w-1.5 shrink-0 text-center">
          {open ? "−" : "+"}
        </span>
        <span className="min-w-0 flex-1">{foldLabel(events.length)}</span>
        <span className="shrink-0 tabular-nums">{ago(now, last.at)}</span>
      </button>
      {open && <FoldList events={events} now={now} />}
    </div>
  );
}

/**
 * What happened to the issue, in order: the comments of people in a frame of their own, between the runs, refusals
 * and small changes of the card, and the form that writes a comment.
 */
export function ActivitySection({ card }: { card: Card }) {
  const now = useNow();
  const [text, setText] = useState("");
  const list = useQuery({ queryKey: keys.comments(card.id), queryFn: () => comments(card.id) });
  const entries = useQuery({ queryKey: keys.history(card.id), queryFn: () => history(card.id) });
  const queryClient = useQueryClient();
  const write = useMutation({
    mutationFn: (body: string) => comment(card.id, body),
    onSuccess: () => {
      setText("");
      void queryClient.invalidateQueries({ queryKey: keys.comments(card.id) });
    },
  });
  const ready = text.trim() !== "" && !write.isPending;
  const items = timelineOf(entries.data ?? [], list.data ?? []);
  const reading = list.isPending || entries.isPending;
  const failed = list.error ?? entries.error;
  return (
    <KanbanIssueSection title="Activity">
      {reading && <SkeletonLines lines={1} message="Reading" />}
      {failed && <Notice tone="error">{said(failed, "Could not read the activity.")}</Notice>}
      {!reading && !failed && items.length === 0 && (
        <p className="m-0 mb-4 text-sm text-secondary">Nothing has happened yet.</p>
      )}
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
          no marker from the accessibility tree entirely */}
      <ol role="list" className="m-0 mb-4 flex list-none flex-col gap-2 p-0">
        {items.map((item, index) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: the timeline is rebuilt whole, in order, from what was read
          <li key={index} className="min-w-0">
            {item.type === "comment" ? (
              <div className="flex min-w-0 flex-col gap-1 rounded-control border border-hairline p-3">
                <span className="text-sm text-secondary">
                  {item.comment.author} · {ago(now, item.comment.at)}
                </span>
                <CardScope.Provider value={card.repository_id}>
                  <CardText
                    content={item.comment.body}
                    projectKey={card.project_key}
                    className="max-w-prose"
                  />
                </CardScope.Provider>
              </div>
            ) : item.type === "fold" ? (
              <Fold events={item.events} now={now} />
            ) : (
              <EventLine event={item.event} now={now} />
            )}
          </li>
        ))}
      </ol>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready) write.mutate(text.trim());
        }}
      >
        <Field label="Write a comment">
          {(id) => <MentionTextArea id={id} value={text} onValueChange={setText} rows={3} />}
        </Field>
        <Refusal text={write.isError ? said(write.error, "Could not write the comment.") : null} />
        <Button type="submit" size="sm" className="w-fit" disabled={!ready}>
          {write.isPending ? "Commenting" : "Comment"}
        </Button>
      </form>
    </KanbanIssueSection>
  );
}

/** The title and body of an issue, written anew; on a forge the issue is changed there. */
export function EditIssueDialog({
  card,
  open,
  onOpenChange,
}: {
  card: Card;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState(card.title);
  const [body, setBody] = useState(card.body);
  const save = useMutation({
    mutationFn: () => editCard(card.id, title.trim(), body),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      onOpenChange(false);
    },
  });
  // Opened again: what the card says now — its title may have been changed in its properties since — not what was
  // typed and left.
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) {
      setTitle(card.title);
      setBody(card.body);
    }
  }
  const leave = (next: boolean) => {
    save.reset();
    onOpenChange(next);
  };
  return (
    <Dialog open={open} onOpenChange={leave}>
      <DialogContent>
        <form
          className="flex flex-col gap-4 p-6"
          onSubmit={(e) => {
            e.preventDefault();
            if (title.trim() !== "" && !save.isPending) save.mutate();
          }}
        >
          <DialogHeader>
            <DialogTitle>{`Edit #${card.number}`}</DialogTitle>
            <DialogDescription>On a forge the issue is changed there too.</DialogDescription>
          </DialogHeader>
          <Field label="Title">
            {(id) => (
              <Input id={id} value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
            )}
          </Field>
          <Field label="Description">
            {(id) => (
              <TextArea id={id} value={body} onChange={(e) => setBody(e.target.value)} rows={6} />
            )}
          </Field>
          <Refusal text={save.isError ? said(save.error, "Could not save the issue.") : null} />
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => leave(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={title.trim() === "" || save.isPending}>
              {save.isPending ? "Saving" : "Save"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
