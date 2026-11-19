/**
 * The issue in the dock besides what the right column says of it (`CardProperties.tsx`): its comments, and the edit of
 * its title and body. Each is a request to the server (`api/calls/board.ts`); the card shows it when the event of the
 * change makes the snapshot read again, and what the server refuses is said beside the control that asked.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { comment, comments, editCard } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
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
import { KanbanIssueSection } from "../../ui/components/KanbanIssuePanel";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";

/** What the server said when it refused, or a sentence for a failure that said nothing. */
const said = (error: unknown, fallback: string) => (isApiError(error) ? error.message : fallback);

/** A refusal, under the control that asked, or nothing. */
function Refusal({ text }: { text: string | null }): ReactNode {
  return text ? <Notice tone="error">{text}</Notice> : null;
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
            <CardScope.Provider value={card.repository_id}>
              <Markdown content={c.body} keepNewlines className="max-w-prose" />
            </CardScope.Provider>
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
