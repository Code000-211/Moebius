/**
 * The question before something is deleted: a dialog that says what goes (and, for a thing that is going, that it is
 * stopped first), with "Keep" and the danger button "Delete". It is asked once; the request is made when the person
 * presses Delete, and the server's own words are a toast when it refuses, with the dialog left open to try again. The dialog closes when the delete is
 * done, and what the page does then (leaving the deleted thing's page) is the caller's `onConfirm`.
 */
import { useState } from "react";
import { sayError } from "../../api/errorText";
import { Button } from "../components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../components/Dialog";
import { useToast } from "./Toast";

export function ConfirmDelete({
  open,
  onOpenChange,
  title,
  children,
  confirmLabel = "Delete",
  pendingLabel = "Deleting",
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** What is asked: "Delete this chat?". */
  title: string;
  /** What is removed, in a sentence or two. */
  children: React.ReactNode;
  confirmLabel?: string;
  /** The button's word while the request is made. */
  pendingLabel?: string;
  /** Makes the request; a rejection is a toast, and the dialog stays open. */
  onConfirm: () => Promise<void>;
}) {
  const [pending, setPending] = useState(false);
  const toast = useToast();

  async function confirm() {
    setPending(true);
    try {
      await onConfirm();
      onOpenChange(false);
    } catch (error) {
      toast({ kind: "error", text: sayError(error, "It could not be deleted. Try again.") });
    }
    setPending(false);
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (pending) return;
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{children}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="secondary" disabled={pending} onClick={() => onOpenChange(false)}>
            Keep
          </Button>
          <Button variant="danger" disabled={pending} onClick={() => void confirm()}>
            {pending ? pendingLabel : confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
