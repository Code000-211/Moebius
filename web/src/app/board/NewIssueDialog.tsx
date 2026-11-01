/**
 * The form of a new issue, which opens from the board's "New issue" button. Create sends
 * `POST /v1/repositories/{id}/issues`; the card appears the way every card does, through the event stream and the
 * snapshot. What the server refused stays in the form, beside the field it belongs to. Asking the Moebius agent to
 * write the issue instead is the import panel's job, on the Skills and MCP servers pages.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
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
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { forgeNamed } from "../projects/model";

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
  const close = () => {
    // Leaving by the door of the dialog itself, so the next time it opens does not still hold the refusal.
    create.reset();
    onOpenChange(false);
  };
  const submit = () => {
    if (ready) create.mutate();
  };
  // A forge the server was never told how to reach is set up on the Settings page, not in this form, so the refusal
  // carries the way there along with what went wrong. A refusal about anything else carries no button.
  const refusal = create.isError
    ? isApiError(create.error)
      ? create.error.message
      : "The issue could not be created. Try again."
    : "";
  const setup = forgeNamed(refusal.includes("not set up") ? refusal : "");

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
          className="flex flex-col gap-4 p-6"
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
          <Field label="Title">
            {(id) => (
              <Input id={id} value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
            )}
          </Field>
          <Field label="Description" hint="What the issue asks for, in a few sentences.">
            {(id) => (
              <TextArea
                id={id}
                value={body}
                onChange={(e) => setBody(e.target.value)}
                rows={6}
                placeholder="What the issue asks for."
              />
            )}
          </Field>
          {create.isError && (
            <Notice
              tone="error"
              action={
                setup ? (
                  <Button asChild variant="ghost" size="sm" onClick={close}>
                    <Link to="/settings">Set up {setup}</Link>
                  </Button>
                ) : undefined
              }
            >
              {refusal}
            </Notice>
          )}
          <DialogFooter>
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button type="submit" disabled={!ready}>
              {create.isPending ? "Creating" : "Create"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
