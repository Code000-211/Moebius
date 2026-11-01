/**
 * The form of a new issue, which opens from the board's "New issue" button. Create sends
 * `POST /v1/repositories/{id}/issues`; the card appears the way every card does, through the event stream and the
 * snapshot. What the server refused is shown in the form. (The desktop app can also let the Moebius agent write the
 * issue; that waits for the chat page.)
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { createIssue } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
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

export function NewIssueDialog({
  repository,
  name,
  open,
  onOpenChange,
  onCreated,
}: {
  repository: number;
  name: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The server made the card: its number. */
  onCreated: (number: number) => void;
}) {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const titleId = useId();
  const bodyId = useId();
  const create = useMutation({
    mutationFn: () => createIssue(repository, title.trim(), body),
    onSuccess: (card) => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      setTitle("");
      setBody("");
      onOpenChange(false);
      onCreated(card.number);
    },
  });
  const ready = title.trim() !== "" && !create.isPending;
  const submit = () => {
    if (ready) create.mutate();
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) create.reset();
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <form
          className="flex flex-col gap-base p-double"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <DialogHeader>
            <DialogTitle>New issue</DialogTitle>
            <DialogDescription>
              Makes a card in the Backlog of {name}. On a forge the issue is opened there too.
            </DialogDescription>
          </DialogHeader>
          <label htmlFor={titleId} className="text-xs text-low">
            Title
          </label>
          <Input id={titleId} value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
          <label htmlFor={bodyId} className="text-xs text-low">
            Description
          </label>
          <textarea
            id={bodyId}
            value={body}
            onChange={(e) => setBody(e.target.value)}
            rows={6}
            className="rounded-md border border-line-strong bg-recessed px-3 py-2 text-sm text-high placeholder:text-low focus-visible:border-accent focus-visible:outline-none"
          />
          {create.isError && (
            <p role="alert" className="m-0 text-sm text-error">
              {isApiError(create.error) ? create.error.message : "The issue could not be created."}
            </p>
          )}
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!ready}>
              {create.isPending ? "Creating…" : "Create"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
