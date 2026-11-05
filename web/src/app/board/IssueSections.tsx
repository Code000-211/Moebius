/**
 * The issue tracker of the dock: what a card is as an issue besides its text. Where it stands among other issues (its
 * parent and its sub-issues, whom it waits on, who claimed it, its labels and whether it is open), its comments, and
 * the edit of its title and body. Every change is a request to the server (`api/calls/board.ts`); the card shows it
 * when the event of the change makes the snapshot read again, and what the server refuses is said beside the control
 * that asked.
 */
import { XIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import {
  addBlocker,
  claim,
  closeIssue,
  comment,
  comments,
  editCard,
  release,
  removeBlocker,
  reopenIssue,
  setLabels,
} from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card } from "../../api/types/board";
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
import { KanbanIssueSection } from "../../ui/components/KanbanIssuePanel";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { descendants, isClosed, parentOf, stackWords } from "./model";
import { Row } from "./Row";

/** What the server said when it refused, or a sentence for a failure that said nothing. */
const said = (error: unknown, fallback: string) => (isApiError(error) ? error.message : fallback);

/**
 * One change of the issue: runs `change`, reads the snapshot again and keeps what the server refused, to show beside the
 * control that asked. Another request clears the refusal.
 */
function useChange<T>(change: (input: T) => Promise<unknown>, fallback: string) {
  const queryClient = useQueryClient();
  const send = useMutation({
    mutationFn: change,
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.cards });
    },
  });
  return {
    run: (input: T) => send.mutate(input),
    pending: send.isPending,
    error: send.isError ? said(send.error, fallback) : null,
  };
}

/** The small link-like button that opens another issue in the dock. */
function IssueLink({ card, onOpen }: { card: Card; onOpen: (card: Card) => void }) {
  return (
    <button
      type="button"
      onClick={() => onOpen(card)}
      className="min-h-10 cursor-pointer appearance-none rounded-control bg-transparent px-0 text-left text-base font-semibold text-text underline decoration-hairline-strong underline-offset-4 hover:decoration-current"
    >
      {`#${card.number} ${card.title}`}
    </button>
  );
}

/** Words typed into a field and sent with Enter or a button, such as a label or the number of a blocker. */
function AddField({
  label,
  button,
  value,
  onChange,
  onAdd,
  disabled,
  inputMode,
}: {
  label: string;
  button: string;
  value: string;
  onChange: (value: string) => void;
  onAdd: () => void;
  disabled: boolean;
  inputMode?: "numeric";
}) {
  const ready = value.trim() !== "" && !disabled;
  return (
    <form
      className="flex items-end gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) onAdd();
      }}
    >
      <Field label={label} className="flex-1">
        {(id) => (
          <Input
            id={id}
            value={value}
            inputMode={inputMode}
            onChange={(e) => onChange(e.target.value)}
          />
        )}
      </Field>
      <Button type="submit" size="sm" variant="secondary" disabled={!ready}>
        {button}
      </Button>
    </form>
  );
}

/** A refusal, under the control that asked, or nothing. */
function Refusal({ text }: { text: string | null }): ReactNode {
  return text ? <Notice tone="error">{text}</Notice> : null;
}

/** Where the issue stands among the others: its place in a stack, what blocks it, who holds it, its labels and state. */
export function TrackerSection({
  board,
  card,
  onOpen,
}: {
  board: Board;
  card: Card;
  /** Opens another issue of the board in the dock. */
  onOpen: (card: Card) => void;
}) {
  const parent = parentOf(board, card);
  const below = descendants(board, card);
  const [label, setLabel] = useState("");
  const [blocker, setBlocker] = useState("");
  const labels = useChange(
    (next: string[]) => setLabels(card.id, next),
    "Could not set the labels.",
  );
  const blockers = useChange(
    ({ number, add }: { number: number; add: boolean }) =>
      add ? addBlocker(card.id, number) : removeBlocker(card.id, number),
    "Could not change the blockers.",
  );
  const claiming = useChange(
    (take: boolean) => (take ? claim(card.id) : release(card.id)),
    "Could not change the claim.",
  );
  const closing = useChange(
    (close: boolean) => (close ? closeIssue(card.id) : reopenIssue(card.id)),
    "Could not change the issue.",
  );
  const closed = isClosed(card);
  const number = Number(blocker.trim().replace(/^#/, ""));

  return (
    <KanbanIssueSection title="The issue tracker">
      <dl className="m-0 flex flex-col gap-3">
        <Row label="State">
          <span className="inline-flex flex-wrap items-center gap-3">
            {closed ? "Closed" : "Open"}
            <Button
              size="sm"
              variant="secondary"
              disabled={closing.pending}
              onClick={() => closing.run(!closed)}
            >
              {closed ? "Reopen issue" : "Close issue"}
            </Button>
          </span>
        </Row>
        {card.parent !== null && (
          <Row label="Parent">
            {parent ? <IssueLink card={parent} onOpen={onOpen} /> : `#${card.parent}`}
          </Row>
        )}
        {below.length > 0 && (
          <Row label="Sub-issues">
            <span className="flex flex-col gap-1">
              <span className="text-sm text-secondary">
                {stackWords({
                  shown: below,
                  total: below.length,
                  done: below.filter(isClosed).length,
                })}
              </span>
              {below.map((child) => (
                <span key={child.id} className="flex flex-wrap items-baseline gap-x-3">
                  <IssueLink card={child} onOpen={onOpen} />
                  <span className="text-sm text-secondary">
                    {isClosed(child) ? "Closed" : child.column}
                  </span>
                </span>
              ))}
            </span>
          </Row>
        )}
        <Row label="Claim">
          <span className="inline-flex flex-wrap items-center gap-3">
            {card.claimed_by ? `Claimed by ${card.claimed_by}` : "Nobody"}
            <Button
              size="sm"
              variant="secondary"
              disabled={claiming.pending}
              onClick={() => claiming.run(card.claimed_by === null)}
            >
              {card.claimed_by ? "Release" : "Claim"}
            </Button>
          </span>
        </Row>
      </dl>
      <Refusal text={closing.error ?? claiming.error} />

      <div className="mt-5 flex flex-col gap-3">
        <h3 className="m-0 text-sm font-semibold text-secondary">Blockers</h3>
        {card.blocked_by.length === 0 && (
          <p className="m-0 text-sm text-secondary">No issue that is not done blocks this one.</p>
        )}
        {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
            no marker from the accessibility tree entirely */}
        <ul role="list" className="m-0 flex list-none flex-col gap-1 p-0">
          {card.blocked_by.map((n) => (
            <li key={n} className="flex items-center gap-2">
              <span className="text-base text-text">{`#${n}`}</span>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Remove blocker #${n}`}
                disabled={blockers.pending}
                onClick={() => blockers.run({ number: n, add: false })}
              >
                <XIcon className="size-4" aria-hidden="true" />
                Remove
              </Button>
            </li>
          ))}
        </ul>
        <AddField
          label="Blocked by issue number"
          button="Add blocker"
          value={blocker}
          inputMode="numeric"
          onChange={setBlocker}
          disabled={blockers.pending || !Number.isInteger(number) || number <= 0}
          onAdd={() => {
            blockers.run({ number, add: true });
            setBlocker("");
          }}
        />
        <Refusal text={blockers.error} />
      </div>

      <div className="mt-5 flex flex-col gap-3">
        <h3 className="m-0 text-sm font-semibold text-secondary">Labels</h3>
        {card.labels.length === 0 && <p className="m-0 text-sm text-secondary">No labels.</p>}
        <div className="flex flex-wrap gap-2">
          {card.labels.map((name) => (
            <span
              key={name}
              className="inline-flex items-center gap-1 rounded-full bg-hover py-0.5 pr-1 pl-3 text-sm text-text"
            >
              {name}
              <button
                type="button"
                aria-label={`Remove label ${name}`}
                disabled={labels.pending}
                onClick={() => labels.run(card.labels.filter((l) => l !== name))}
                className="inline-flex size-6 cursor-pointer items-center justify-center rounded-full bg-transparent text-secondary hover:text-text"
              >
                <XIcon className="size-3" aria-hidden="true" />
              </button>
            </span>
          ))}
        </div>
        <AddField
          label="New label"
          button="Add label"
          value={label}
          onChange={setLabel}
          disabled={labels.pending}
          onAdd={() => {
            labels.run([...card.labels, label.trim()]);
            setLabel("");
          }}
        />
        <Refusal text={labels.error} />
      </div>
    </KanbanIssueSection>
  );
}

/** The comments of the issue, oldest first, and the form that writes one. */
export function CommentsSection({ card }: { card: Card }) {
  const now = useNow();
  const [text, setText] = useState("");
  const list = useQuery({ queryKey: keys.comments(card.id), queryFn: () => comments(card.id) });
  const queryClient = useQueryClient();
  const write = useMutation({
    mutationFn: (body: string) => comment(card.id, body),
    onSuccess: () => {
      setText("");
      void queryClient.invalidateQueries({ queryKey: keys.comments(card.id) });
    },
  });
  const ready = text.trim() !== "" && !write.isPending;
  return (
    <KanbanIssueSection title="Comments">
      {list.isPending && <SkeletonLines lines={1} message="Reading" />}
      {list.isError && (
        <Notice tone="error">{said(list.error, "Could not read the comments.")}</Notice>
      )}
      {list.data?.length === 0 && (
        <p className="m-0 mb-4 text-sm text-secondary">No comments yet.</p>
      )}
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
          no marker from the accessibility tree entirely */}
      <ol role="list" className="m-0 mb-4 flex list-none flex-col gap-4 p-0">
        {(list.data ?? []).map((c) => (
          <li key={c.id} className="flex min-w-0 flex-col gap-1">
            <span className="text-sm text-secondary">
              {c.author} · {ago(now, c.at)}
            </span>
            <Markdown content={c.body} keepNewlines className="max-w-prose" />
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
          {(id) => (
            <TextArea id={id} value={text} onChange={(e) => setText(e.target.value)} rows={3} />
          )}
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
  const leave = (next: boolean) => {
    if (next) {
      // Opened again: what the card says now, not what was typed and left.
      setTitle(card.title);
      setBody(card.body);
    }
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
