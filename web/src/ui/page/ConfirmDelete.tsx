/**
 * The question before something is deleted: a dialog that says what goes (and, for a thing that is going, that it is
 * stopped first), with "Keep" and the danger button "Delete". It is asked once; the request is made when the person
 * presses Delete, and the server's own words stay in the dialog when it refuses. The dialog closes when the delete is
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
import { Notice } from "./Notice";

export function ConfirmDelete({
  open,
  onOpenChange,
  title,
  children,
  confirmLabel = "Delete",
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** What is asked: "Delete this chat?". */
  title: string;
  /** What is removed, in a sentence or two. */
  children: React.ReactNode;
  confirmLabel?: string;
  /** Makes the request; a rejection is shown in the dialog, which stays open. */
  onConfirm: () => Promise<void>;
}) {
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  async function confirm() {
    setPending(true);
    setFailed(null);
    try {
      await onConfirm();
      onOpenChange(false);
    } catch (error) {
      setFailed(sayError(error, "It could not be deleted. Try again."));
    }
    setPending(false);
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (pending) return;
        if (!next) setFailed(null);
        onOpenChange(next);
      }}
    >
      <DialogContent className="gap-4 p-6">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{children}</DialogDescription>
        </DialogHeader>
        {failed && <Notice tone="error">{failed}</Notice>}
        <DialogFooter>
          <Button variant="secondary" disabled={pending} onClick={() => onOpenChange(false)}>
            Keep
          </Button>
          <Button variant="danger" disabled={pending} onClick={() => void confirm()}>
            {pending ? "Deleting" : confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
