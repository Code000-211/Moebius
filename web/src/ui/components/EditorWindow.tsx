/**
 * The window every editor opens in: a centred solid window over the dimmed page (the same `Dialog` as the card window,
 * so scrim, radius and shadow are the standard's) with a title, the editor's own fields as the body, and Save and
 * Cancel at the foot. The draft is guarded: closing by Escape, the close icon, a press on the dimmed page or Cancel
 * while `dirty` asks "Discard changes?" inside the window first, and only "Discard" closes it. A clean window closes
 * at once. Save is the caller's `onSave`; the window stays open while `saving` and shows `error` beside the buttons,
 * and the caller closes it (`onClose`) once the save is done. `actions` puts a further control (Delete) at the left of the foot.
 *
 * The window keeps its place and size: it is as tall as the standard's editor (or, with `height="fit"`, as tall as
 * a short read-only body) and does not change when the "Discard changes?" question takes the foot's place, which
 * keeps the foot's height; the body scrolls between the fixed head and foot. The focus goes to the first field when
 * the window opens, or to the window itself, never to the close icon. Without `onSave` the window is for reading: its
 * foot has Close and no Save. `DeleteButton` is the Delete of the standard: outlined, the label in `--danger-text`.
 */

import { XIcon } from "@phosphor-icons/react";
import { type ComponentProps, type ReactNode, useRef, useState } from "react";
import { cn } from "../lib/cn";
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

/** The Delete of an editor window: an outlined button with the label in `--danger-text`, never a filled one. */
export function DeleteButton(props: ComponentProps<typeof Button>) {
  return <Button variant="danger" size="sm" {...props} />;
}

const FIELDS = [
  "input:not([type=hidden]):not(:disabled)",
  "textarea:not(:disabled)",
  "select:not(:disabled)",
  "[role=combobox]:not(:disabled)",
  "button[aria-haspopup]:not(:disabled)",
  "[contenteditable=true]",
].join(", ");

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
  height = "fixed",
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
  /** Saves the draft; without it the window only shows its content and has Close in place of Cancel and Save. */
  onSave?: () => void;
  onClose: () => void;
  /** `fixed` (the default) is the editor's height; `fit` is as tall as a short body, for read-only content. */
  height?: "fixed" | "fit";
  children: ReactNode;
}) {
  const [asking, setAsking] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
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
        className={cn(
          "max-w-xl gap-0 overflow-hidden p-0",
          height === "fixed" && "h-[32rem] max-h-[calc(100%_-_2rem)]",
        )}
        ref={content}
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          // A field that took the focus itself (autoFocus) keeps it; otherwise the first field, or the window.
          const active = document.activeElement;
          if (active && active !== content.current && content.current?.contains(active)) return;
          (body.current?.querySelector<HTMLElement>(FIELDS) ?? content.current)?.focus();
        }}
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
        <div
          ref={body}
          className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto p-6 [&_label>span:first-child]:font-medium [&_textarea]:resize-none"
        >
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
            className="flex h-14 shrink-0 items-center gap-3 border-t border-hairline px-6"
          >
            <p className="m-0 min-w-0 flex-1 text-base text-text">
              Discard changes? What you typed is lost.
            </p>
            <Button variant="secondary" size="sm" autoFocus onClick={() => setAsking(false)}>
              Keep editing
            </Button>
            <DeleteButton onClick={leave}>Discard</DeleteButton>
          </div>
        ) : (
          <DialogFooter className="h-14 shrink-0 items-center border-t border-hairline px-6">
            {actions && <div className="flex gap-2 sm:mr-auto">{actions}</div>}
            {onSave ? (
              <>
                <Button variant="secondary" size="sm" disabled={saving} onClick={close}>
                  Cancel
                </Button>
                <Button size="sm" disabled={saving || !canSave || !dirty} onClick={onSave}>
                  {saving ? "Saving" : "Save"}
                </Button>
              </>
            ) : (
              <Button variant="secondary" size="sm" onClick={close}>
                Close
              </Button>
            )}
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
