/**
 * The form of a new issue, which opens from the board's "New issue" button, or from a card's "Add a sub-issue" with that
 * card as its parent, which the form offers in a field of its own. Create sends `POST /v1/repositories/{id}/issues`; the card appears the way every card does, through the event stream and the
 * snapshot. What the server refused stays in the form, beside the field it belongs to. Asking the Moebius agent to
 * write the issue instead is the import panel's job, on the Skills and MCP servers pages.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { createIssue } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card } from "../../api/types/board";
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
import { Field, Select, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { forgeNamed } from "../projects/model";

export function NewIssueDialog({
  repository,
  name,
  board,
  parent: initialParent = null,
  open,
  onOpenChange,
  onCreated,
}: {
  repository: number;
  name: string;
  /** The board, whose issues the parent is chosen among. */
  board: Board;
  /** The issue number the form starts with as the parent; none for a new issue of its own. */
  parent?: number | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The server made the card: the card itself, so the page can say what it made. */
  onCreated: (card: Card) => void;
}) {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  // The chosen parent's number, "none" for no parent; the one the form was opened with until a choice is made.
  const [chosen, setChosen] = useState<string | null>(null);
  const parent = chosen ?? (initialParent === null ? "none" : String(initialParent));
  const create = useMutation({
    mutationFn: () =>
      createIssue(repository, title.trim(), body, parent === "none" ? null : Number(parent)),
    onSuccess: (card) => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      setTitle("");
      setBody("");
      setChosen(null);
      onOpenChange(false);
      onCreated(card);
    },
  });
  const ready = title.trim() !== "" && !create.isPending;
  const close = () => {
    // Leaving by the door of the dialog itself, so the next time it opens does not still hold the refusal.
    create.reset();
    setChosen(null);
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
        if (!next) {
          create.reset();
          setChosen(null);
        }
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
          {board.cards.length > 0 && (
            <Field label="Parent" hint="Makes it a sub-issue, in the stack of its parent.">
              {(id) => (
                <Select
                  id={id}
                  value={parent}
                  options={[
                    ["none", "No parent"],
                    ...[...board.cards]
                      .sort((a, b) => a.number - b.number)
                      .map((c): [string, string] => [String(c.number), `#${c.number} ${c.title}`]),
                  ]}
                  onChange={setChosen}
                />
              )}
            </Field>
          )}
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
