/**
 * Adding a card in place, at the foot of a column and from the "+" in its head. Both open one line to type in at the
 * foot of the column: Enter makes the card (`POST /v1/repositories/{id}/issues`, the same call as the dialog's), Escape
 * leaves, and what the server refused stays under the line. The server puts every new issue in the Backlog, so in any
 * other column the page says where the card went. The longer form, with a description and a parent, is the dialog on
 * `C`.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { createIssue } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import { Input } from "../../ui/components/Input";
import { Notice } from "../../ui/page/Notice";

/** The "+" in a column's head: it opens the line at the foot. */
export function ColumnAddButton({ column, onClick }: { column: string; onClick: () => void }) {
  return (
    <button
      type="button"
      aria-label={`Add a card to ${column}`}
      onClick={onClick}
      className="flex size-6 cursor-pointer appearance-none items-center justify-center rounded-control bg-transparent text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
    >
      <PlusIcon className="size-4" aria-hidden="true" />
    </button>
  );
}

/** The foot of a column: "+ Add", or the line it turns into while `open`. */
export function ColumnAdd({
  repository,
  column,
  open,
  onOpen,
  onClose,
  onCreated,
}: {
  repository: number;
  column: string;
  open: boolean;
  onOpen: () => void;
  onClose: () => void;
  onCreated: (card: Card, column: string) => void;
}) {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const field = useRef<HTMLInputElement>(null);
  const create = useMutation({
    mutationFn: () => createIssue(repository, title.trim(), ""),
    onSuccess: (card) => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      setTitle("");
      onClose();
      onCreated(card, column);
    },
  });
  useEffect(() => {
    if (open) field.current?.focus();
  }, [open]);
  const leave = () => {
    create.reset();
    setTitle("");
    onClose();
  };
  if (!open) {
    return (
      <button
        type="button"
        aria-label={`Add a card to ${column}, at the foot`}
        onClick={onOpen}
        className="mt-2 flex min-h-8 w-full shrink-0 cursor-pointer appearance-none items-center gap-2 rounded-control bg-transparent px-2 text-left text-sm text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
      >
        <PlusIcon className="size-4 shrink-0" aria-hidden="true" />
        Add
      </button>
    );
  }
  return (
    <div className="mt-2 flex shrink-0 flex-col gap-2">
      <Input
        ref={field}
        aria-label={`Title of the new card in ${column}`}
        placeholder="Title of the card"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            leave();
          } else if (e.key === "Enter" && !e.nativeEvent.isComposing) {
            e.preventDefault();
            if (title.trim() !== "" && !create.isPending) create.mutate();
          }
        }}
      />
      {create.isError && (
        <Notice tone="error">
          {isApiError(create.error)
            ? create.error.message
            : "The issue could not be created. Try again."}
        </Notice>
      )}
    </div>
  );
}
