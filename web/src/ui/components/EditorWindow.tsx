/**
 * The window every editor opens in: a centred solid window over the dimmed page (the same `Dialog` as the card window,
 * so scrim, radius and shadow are the standard's) with a title, the editor's own fields as the body, and Save and
 * Cancel at the foot. The draft is guarded: closing by Escape, the close icon, a press on the dimmed page or Cancel
 * while `dirty` asks "Discard changes?" inside the window first, and only "Discard" closes it. A clean window closes
 * at once. Save is the caller's `onSave`; the window stays open while `saving` and shows `error` beside the buttons,
 * and the caller closes it (`onClose`) once the save is done. `actions` puts a further control (Delete) at the left of the foot.
 */

import { XIcon } from "@phosphor-icons/react";
import { type ReactNode, useState } from "react";
import { Button } from "./Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./Dialog";
import { IconButton } from "./IconButton";

export function EditorWindow({
  open,
  title,
  description,
  dirty,
  saving = false,
  canSave = true,
  error,
  actions,
  onSave,
  onClose,
  children,
}: {
  open: boolean;
  title: string;
  /** One quiet line under the title, only where it says something the fields do not. */
  description?: string;
  /** Whether the draft differs from what is saved. */
  dirty: boolean;
  saving?: boolean;
  /** False while the draft cannot be saved (an invalid name); the button is then disabled. */
  canSave?: boolean;
  /** What the server refused, shown above the buttons. */
  error?: ReactNode;
  /** A control for the left of the foot, such as Delete. */
  actions?: ReactNode;
  onSave: () => void;
  onClose: () => void;
  children: ReactNode;
}) {
  const [asking, setAsking] = useState(false);
  const close = () => {
    if (saving) return;
    if (dirty) setAsking(true);
    else onClose();
  };
  const leave = () => {
    setAsking(false);
    onClose();
  };
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) close();
      }}
    >
      <DialogContent
        className="max-w-xl gap-0 p-0"
        data-audit="flush"
        data-testid="editor-window"
        hideCloseButton
      >
        <DialogHeader className="h-12 shrink-0 flex-row items-center gap-2 border-b border-hairline pr-2 pl-6">
          <DialogTitle className="min-w-0 flex-1 truncate text-md font-semibold">
            {title}
          </DialogTitle>
          <IconButton icon={XIcon} aria-label="Close" onClick={close} />
        </DialogHeader>
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto p-6">
          {description ? (
            <DialogDescription>{description}</DialogDescription>
          ) : (
            <DialogDescription className="sr-only">{title}</DialogDescription>
          )}
          {children}
          {error && (
            <p role="alert" className="m-0 break-words text-sm text-error-text">
              {error}
            </p>
          )}
        </div>
        {asking ? (
          <div
            role="alertdialog"
            aria-label="Discard changes?"
            className="flex shrink-0 flex-col gap-3 border-t border-hairline px-6 py-4"
          >
            <p className="m-0 text-base text-text">Discard changes? What you typed is lost.</p>
            <DialogFooter>
              <Button variant="secondary" size="sm" autoFocus onClick={() => setAsking(false)}>
                Keep editing
              </Button>
              <Button variant="danger" size="sm" onClick={leave}>
                Discard
              </Button>
            </DialogFooter>
          </div>
        ) : (
          <DialogFooter className="shrink-0 items-center border-t border-hairline px-6 py-3">
            {actions && <div className="flex gap-2 sm:mr-auto">{actions}</div>}
            <Button variant="secondary" size="sm" disabled={saving} onClick={close}>
              Cancel
            </Button>
            <Button size="sm" disabled={saving || !canSave || !dirty} onClick={onSave}>
              {saving ? "Saving" : "Save"}
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
