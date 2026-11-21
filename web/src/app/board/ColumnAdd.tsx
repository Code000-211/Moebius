/**
 * Adding a card in place. A column has no button for it: the column's context menu (a right click on its name, or the
 * menu key and Enter on it) has "Add a card", and opens one line to type in under the name: Enter makes the card (`POST /v1/repositories/{id}/issues`, the same call as the dialog's), Escape
 * leaves, and what the server refused is a toast. The server puts every new issue in the Backlog, so in any
 * other column the page says where the card went. The longer form, with a description and a parent, is the dialog on
 * `C`.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { createIssue } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import { Input } from "../../ui/components/Input";
import { useToast } from "../../ui/page/Toast";

/** The line for a new card under a column's name, while `open`; nothing while it is closed. */
export function ColumnAdd({
  repository,
  column,
  open,
  onClose,
  onCreated,
}: {
  repository: number;
  column: string;
  open: boolean;
  onClose: () => void;
  onCreated: (card: Card, column: string) => void;
}) {
  const queryClient = useQueryClient();
  const toast = useToast();
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
    onError: (error) =>
      toast({
        kind: "error",
        text: isApiError(error) ? error.message : "The issue could not be created. Try again.",
      }),
  });
  useEffect(() => {
    if (open) field.current?.focus();
  }, [open]);
  const leave = () => {
    create.reset();
    setTitle("");
    onClose();
  };
  if (!open) return null;
  return (
    <div className="mb-3 flex shrink-0 flex-col gap-2">
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
    </div>
  );
}
