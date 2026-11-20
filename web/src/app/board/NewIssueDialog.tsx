/**
 * The form of a new issue, which opens on `C` from any page, from the board's "New issue" button, or from a card's "Add
 * a sub-issue" with that card as its parent (`NewIssueHost.tsx` holds it for the whole app). Its properties are chips
 * over the title and the description: the repository, when there is more than one, and the parent. Create sends `POST
 * /v1/repositories/{id}/issues`; the card appears the way every card does, through the event stream and the snapshot.
 * "Create more" keeps the form open for the next card, with the properties as they were. What the server refused stays
 * in the form, beside the field it belongs to. "Ask Moebius" starts a chat with the Moebius agent, which writes the
 * issue out from what is typed so far.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router";
import { createIssue } from "../../api/calls/board";
import { startChat } from "../../api/calls/chat";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card } from "../../api/types/board";
import { ASSISTANT_AGENT } from "../../chat/AgentChat";
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
  repositories = [],
  onRepository,
  board,
  parent: initialParent = null,
  open,
  onOpenChange,
  onCreated,
}: {
  repository: number;
  name: string;
  /** Every repository there is; the form offers the choice among them as a chip when there are two or more. */
  repositories?: { id: number; name: string }[];
  onRepository?: (repository: number) => void;
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
  const navigate = useNavigate();
  const titleField = useRef<HTMLInputElement>(null);
  const [more, setMore] = useState(false);
  // What "Create more" made last, said in the form while it stays open for the next card.
  const [made, setMade] = useState<Card | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  // The chosen parent's number, "none" for no parent; the one the form was opened with until a choice is made.
  const [chosen, setChosen] = useState<string | null>(null);
  const wanted = chosen ?? (initialParent === null ? "none" : String(initialParent));
  // A parent of another repository's board is no parent here.
  const parent = board.cards.some((c) => String(c.number) === wanted) ? wanted : "none";
  const create = useMutation({
    mutationFn: () =>
      createIssue(repository, title.trim(), body, parent === "none" ? null : Number(parent)),
    onSuccess: (card) => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      setTitle("");
      setBody("");
      onCreated(card);
      if (more) {
        // Open for the next card, with the properties as they were.
        setMade(card);
        create.reset();
        return;
      }
      setChosen(null);
      onOpenChange(false);
    },
  });
  const ask = useMutation({
    mutationFn: () =>
      startChat({
        agent: ASSISTANT_AGENT,
        origin: "assistant",
        repository,
        message: `Write out and create this issue in ${name} (repository ${repository})${
          parent === "none" ? "" : `, as a sub-issue of #${parent}`
        }.\n\nTitle: ${title.trim()}\n\n${body}`.trim(),
      }),
    onSuccess: (run) => {
      void queryClient.invalidateQueries({ queryKey: keys.chats });
      setTitle("");
      setBody("");
      setChosen(null);
      onOpenChange(false);
      navigate(`/chat/${run.id}`);
    },
  });
  // The title takes the cursor again once a card is made and the form stays open.
  useEffect(() => {
    if (made) titleField.current?.focus();
  }, [made]);
  const ready = title.trim() !== "" && !create.isPending;
  const close = () => {
    // Leaving by the door of the dialog itself, so the next time it opens does not still hold the refusal.
    create.reset();
    ask.reset();
    setMade(null);
    setChosen(null);
    onOpenChange(false);
  };
  const submit = () => {
    if (ready) {
      setMade(null);
      create.mutate();
    }
  };
  // A forge the server was never told how to reach is set up on the Settings page, not in this form, so the refusal
  // carries the way there along with what went wrong. A refusal about anything else carries no button.
  const refusal = create.isError
    ? isApiError(create.error)
      ? create.error.message
      : "The issue could not be created. Try again."
    : "";
  const setup = forgeNamed(refusal.includes("not set up") ? refusal : "");
  // The refusal stands under the field it is about: the title, the parent, or at the foot when it is about neither.
  const lower = refusal.toLowerCase();
  const refusedAt =
    refusal === "" || setup
      ? "foot"
      : lower.includes("parent")
        ? "parent"
        : lower.includes("title")
          ? "title"
          : "foot";
  const refusalNotice = (
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
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          create.reset();
          ask.reset();
          setMade(null);
          setChosen(null);
        }
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <form
          className="flex flex-col gap-4"
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
          {(repositories.length > 1 || board.cards.length > 0) && (
            <div className="flex flex-wrap items-center gap-2">
              {repositories.length > 1 && onRepository && (
                <Select
                  aria-label="Repository"
                  value={String(repository)}
                  options={repositories.map((r): [string, string] => [String(r.id), r.name])}
                  onChange={(value) => onRepository(Number(value))}
                  className="w-auto min-w-32 max-w-56 rounded-full"
                />
              )}
              {board.cards.length > 0 && (
                <Select
                  aria-label="Parent"
                  value={parent}
                  options={[
                    ["none", "No parent"],
                    ...[...board.cards]
                      .sort((a, b) => a.number - b.number)
                      .map((c): [string, string] => [String(c.number), `#${c.number} ${c.title}`]),
                  ]}
                  onChange={setChosen}
                  className="w-auto min-w-32 max-w-72 rounded-full"
                />
              )}
            </div>
          )}
          {refusedAt === "parent" && create.isError && refusalNotice}
          <Field label="Title">
            {(id) => (
              <Input
                id={id}
                ref={titleField}
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                autoFocus
              />
            )}
          </Field>
          {refusedAt === "title" && create.isError && refusalNotice}
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
          {made && (
            <Notice>
              Created #{made.number} {made.title}
            </Notice>
          )}
          {refusedAt === "foot" && create.isError && refusalNotice}
          {ask.isError && (
            <Notice tone="error">
              {isApiError(ask.error) ? ask.error.message : "Moebius could not be asked. Try again."}
            </Notice>
          )}
          <DialogFooter className="items-center">
            <label className="mr-auto flex cursor-pointer items-center gap-2 text-sm text-secondary">
              <input
                type="checkbox"
                checked={more}
                onChange={(e) => setMore(e.target.checked)}
                className="size-4"
              />
              Create more
            </label>
            <Button
              variant="ghost"
              disabled={title.trim() === "" || ask.isPending || create.isPending}
              onClick={() => ask.mutate()}
            >
              {ask.isPending ? "Asking" : "Ask Moebius to write it"}
            </Button>
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
