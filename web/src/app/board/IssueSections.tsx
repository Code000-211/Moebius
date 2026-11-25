/**
 * The issue in the card window besides what its menu says of it (`CardProperties.tsx`): its thread (the text of the
 * issue, comments, runs, moves and refusals as one timeline), the field to reply in, and the edit of its title and
 * body. Each is a request to the server (`api/calls/board.ts`); the card shows it when the event of the change makes
 * the snapshot read again, and what the server refuses is said as a toast.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, type RefObject, useEffect, useState } from "react";
import { Link } from "react-router";
import { comment, comments, editCard, history, snapshot } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Card, InboxItem } from "../../api/types/board";
import { putCard } from "../../live/cardCache";
import { CardScope } from "../../ui/chat/CardLinks";
import { Markdown } from "../../ui/chat/Markdown";
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
import { CaretRightIcon, ReturnIcon } from "../../ui/icons";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Avatar } from "../../ui/page/Avatar";
import { Field, TextArea } from "../../ui/page/Field";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { useToast } from "../../ui/page/Toast";
import { MentionTextArea } from "../people/MentionTextArea";
import { CardText } from "./CardText";
import { MoebiusMark } from "./NeedsYou";
import {
  foldLabel,
  isShortNote,
  moveLabel,
  runDuration,
  runLabel,
  type TimelineEvent,
  type TimelineRun,
  timelineOf,
} from "./timeline";

/** What the server said when it refused, or a sentence for a failure that said nothing. */
const said = (error: unknown, fallback: string) => (isApiError(error) ? error.message : fallback);

/** The line down the left of the thread, and the mark each of its items hangs on. */
const MARK = "relative z-[1] flex size-6 shrink-0 items-center justify-center bg-panel";

/** The list of the thread: a line runs down behind the marks of its items. */
const LINE =
  "relative m-0 flex list-none flex-col gap-4 p-0 before:absolute before:top-3 before:bottom-3 before:left-3 before:w-px before:bg-hairline";

/** A small dot on the line of the thread. */
function Dot({ tone = "quiet" }: { tone?: "quiet" | "refusal" | "live" }) {
  return (
    <span className={MARK}>
      <span
        aria-hidden="true"
        className={`size-2 rounded-full ${tone === "refusal" ? "bg-error" : tone === "live" ? "bg-running" : "bg-hairline-strong"}`}
      />
    </span>
  );
}

/** One line of the thread that is not a comment: a refusal of a rule or a change. */
function EventLine({ event }: { event: TimelineEvent }) {
  return (
    <div className="flex min-w-0 items-start gap-3 text-md text-secondary">
      <Dot tone={event.tone === "refusal" ? "refusal" : "quiet"} />
      <span className="min-w-0 flex-1 py-px [overflow-wrap:anywhere]">{event.text}</span>
    </div>
  );
}

/**
 * A move: "Moved to In progress". A short reason stands on the line; a long one, which is Markdown (a review's
 * findings, a list of files), is a part of the line that opens, closed at first, and is drawn as Markdown.
 */
function MoveLine({ event }: { event: TimelineEvent }) {
  const note = event.note ?? "";
  if (note === "" || isShortNote(note)) {
    return (
      <div className="flex min-w-0 items-start gap-3 text-md text-secondary">
        <Dot />
        <span className="min-w-0 flex-1 py-px [overflow-wrap:anywhere]">
          <span className="text-text">{moveLabel(event)}</span>
          {note === "" ? "" : `: ${note}`}
        </span>
      </div>
    );
  }
  return (
    <Collapsible label={moveLabel(event)} mark={<Dot />}>
      <Markdown content={note} keepNewlines className="max-w-prose text-md" />
    </Collapsible>
  );
}

/** A line that opens to what it holds: the chevron, its name, and what it holds below. */
function Collapsible({
  label,
  mark,
  children,
}: {
  label: string;
  mark: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="flex min-w-0 flex-col">
      <div className="flex items-start gap-3">
        {mark}
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="-ml-1 inline-flex min-h-6 cursor-pointer items-center gap-1 rounded-control bg-transparent px-1 text-md text-text hover:bg-hover"
        >
          <CaretRightIcon
            aria-hidden="true"
            className={`size-3.5 text-secondary transition-transform duration-(--motion-routine) ${open ? "rotate-90" : ""}`}
          />
          {label}
        </button>
      </div>
      {open && <div className="ml-9 flex min-w-0 flex-col gap-2 pt-1 pb-1">{children}</div>}
    </div>
  );
}

/** Small changes folded into one line that opens to the changes it holds. */
function Fold({ events }: { events: TimelineEvent[] }) {
  return (
    <Collapsible label={foldLabel(events.length)} mark={<Dot />}>
      {events.map((event, index) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: the changes of a fold are a fixed, ordered list
        <p key={index} className="m-0 text-sm text-secondary [overflow-wrap:anywhere]">
          {event.text}
        </p>
      ))}
    </Collapsible>
  );
}

/** A run as one collapsed line, "Code run" or "Review"; open, it says who ran, how it ended and for how long, and leads to the run. */
function RunLine({
  run,
  current,
  live,
  busy,
  onStop,
}: {
  run: TimelineRun;
  /** Whether this is the card's own current run, which can be watched and stopped. */
  current: boolean;
  live: boolean;
  busy: boolean;
  onStop: () => void;
}) {
  const running = run.endedAt === null;
  const duration = runDuration(run);
  const summary = [run.agent, running ? "running" : run.status, duration]
    .filter((part) => part !== null && part !== "")
    .join(" · ");
  return (
    <Collapsible label={runLabel(run)} mark={<Dot tone={running ? "live" : "quiet"} />}>
      <p className="m-0 text-sm text-secondary">{summary}</p>
      <div className="flex flex-wrap gap-2">
        <Button asChild variant="secondary" size="sm" className="w-fit">
          <Link to={`/runs/${run.run}`}>{current ? "Open live view" : `View run ${run.run}`}</Link>
        </Button>
        {current && live && (
          <Button variant="secondary" size="sm" className="w-fit" disabled={busy} onClick={onStop}>
            Stop run
          </Button>
        )}
      </div>
    </Collapsible>
  );
}

/** A comment of the thread: who, when, and the text below. */
function CommentItem({
  card,
  author,
  at,
  body,
  now,
}: {
  card: Card;
  author: string;
  at: number | null;
  body: string;
  now: number;
}) {
  return (
    <div className="flex min-w-0 items-start gap-3">
      <span className={MARK}>
        <Avatar handle={author.replace(/^agent:/, "")} className="size-6 text-xs" />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="flex items-baseline gap-2 text-md">
          <span className="font-semibold text-text">{author}</span>
          {at !== null && <span className="text-secondary">{ago(now, at)}</span>}
        </span>
        <CardScope.Provider value={card.repository_id}>
          <CardText content={body} projectKey={card.project_key} className="max-w-prose" />
        </CardScope.Provider>
      </div>
    </div>
  );
}

/**
 * What happened to the issue, in order, as a timeline with a line down its left: the issue's own text as the first
 * entry, the comments of people, the moves as small dots, a coding run and a review as one collapsed line each, the
 * refusals and small changes, and last the block of what waits for a person (`needsYou`, drawn by the card).
 */
export function CardThread({
  card,
  item,
  live,
  busy,
  onStop,
  needsYou,
}: {
  card: Card;
  /** What the card waits for from a person; the thread ends with its block. */
  item?: InboxItem;
  /** Whether the card's run is live: it can be stopped from its line. */
  live: boolean;
  busy: boolean;
  onStop: () => void;
  needsYou: (item: InboxItem) => ReactNode;
}) {
  const now = useNow();
  const toast = useToast();
  const list = useQuery({ queryKey: keys.comments(card.id), queryFn: () => comments(card.id) });
  const entries = useQuery({ queryKey: keys.history(card.id), queryFn: () => history(card.id) });
  // The columns of the card's project, to read a move between custom columns by their classes.
  const columns = useQuery({
    queryKey: keys.snapshot,
    queryFn: () => snapshot(),
    select: (all) => all.boards.find((b) => b.cards.some((c) => c.id === card.id))?.columns,
  }).data;
  const items = timelineOf(entries.data ?? [], list.data ?? [], item, columns);
  const reading = list.isPending || entries.isPending;
  const failed = list.error ?? entries.error;
  // A failed read is said once, as a toast; the thread keeps what it has.
  useEffect(() => {
    if (failed) toast({ kind: "error", text: said(failed, "Could not read the activity.") });
  }, [failed, toast]);
  const hasBody = card.body.trim() !== "";
  return (
    <>
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
        no marker from the accessibility tree entirely */}
      <ol role="list" data-testid="thread" className={LINE}>
        {hasBody && (
          <li className="min-w-0">
            <CommentItem
              card={card}
              author={card.author}
              at={card.created_at > 0 ? card.created_at : null}
              body={card.body}
              now={now}
            />
          </li>
        )}
        {reading && (
          <li className="min-w-0">
            <SkeletonLines lines={1} message="Reading" />
          </li>
        )}
        {items.map((each, index) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: the timeline is rebuilt whole, in order, from what was read
          <li key={index} className="min-w-0">
            {each.type === "comment" ? (
              <CommentItem
                card={card}
                author={each.comment.author}
                at={each.comment.at}
                body={each.comment.body}
                now={now}
              />
            ) : each.type === "run" ? (
              <RunLine
                run={each.run}
                current={card.run?.id === each.run.run}
                live={live}
                busy={busy}
                onStop={onStop}
              />
            ) : each.type === "move" ? (
              <MoveLine event={each.event} />
            ) : each.type === "fold" ? (
              <Fold events={each.events} />
            ) : each.type === "needs-you" ? (
              <div className="flex min-w-0 items-start gap-3">
                {/* The line of the thread ends at Moebius's mark: its column is the panel's colour, down to the block's foot. */}
                <span className="relative z-[1] flex shrink-0 items-start self-stretch bg-panel">
                  <MoebiusMark />
                </span>
                <div className="min-w-0 flex-1">{needsYou(each.item)}</div>
              </div>
            ) : (
              <EventLine event={each.event} />
            )}
          </li>
        ))}
      </ol>
    </>
  );
}

/**
 * The field a person replies in, pinned at the foot of the window: Enter sends it as a comment, Shift+Enter starts
 * a new line, and the arrow at its end sends it too. What the server refuses is said as a toast and the text stays.
 */
export function ReplyField({
  card,
  fieldRef,
}: {
  card: Card;
  fieldRef: RefObject<HTMLFormElement | null>;
}) {
  const [text, setText] = useState("");
  const toast = useToast();
  const queryClient = useQueryClient();
  const write = useMutation({
    mutationFn: (body: string) => comment(card.id, body),
    onSuccess: () => {
      setText("");
      void queryClient.invalidateQueries({ queryKey: keys.comments(card.id) });
    },
    onError: (error) => toast({ kind: "error", text: said(error, "Could not write the comment.") }),
  });
  const ready = text.trim() !== "" && !write.isPending;
  const send = () => {
    if (ready) write.mutate(text.trim());
  };
  return (
    <form
      ref={fieldRef}
      className="relative"
      onSubmit={(e) => {
        e.preventDefault();
        send();
      }}
    >
      <MentionTextArea
        aria-label="Write a comment"
        placeholder="Reply to Moebius…"
        rows={1}
        value={text}
        onValueChange={setText}
        onKeyDown={(e) => {
          // Enter that picks a person from the list is the list's.
          if (
            e.key === "Enter" &&
            !e.shiftKey &&
            e.currentTarget.getAttribute("aria-expanded") !== "true"
          ) {
            e.preventDefault();
            send();
          }
        }}
        className="min-h-12 resize-none py-3 pr-14"
      />
      <Button
        type="submit"
        variant="icon"
        size="sm"
        aria-label="Comment"
        disabled={!ready}
        className="absolute top-1/2 right-2 -translate-y-1/2 bg-control-fill! text-text disabled:text-secondary disabled:opacity-100"
      >
        <ReturnIcon className="size-4" aria-hidden="true" />
      </Button>
    </form>
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
  const toast = useToast();
  const [title, setTitle] = useState(card.title);
  const [body, setBody] = useState(card.body);
  const save = useMutation({
    mutationFn: () => editCard(card.id, title.trim(), body),
    onSuccess: (saved) => {
      putCard(queryClient, saved);
      onOpenChange(false);
    },
    onError: (error) => toast({ kind: "error", text: said(error, "Could not save the issue.") }),
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
          className="flex flex-col gap-4"
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
