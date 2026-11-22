/**
 * The pane a chosen step is edited in, at the right of the canvas: its title and a close icon, the editor's own fields
 * as the body, and Save and Cancel at the foot, with `actions` (Delete) at the left of the foot. It sits beside the
 * graph and covers none of it. The draft is guarded the way the centred window guards it: closing while `dirty` asks
 * "Discard changes?" in the foot first. Without `onSave` the pane is for reading and has no Save.
 */
import { XIcon } from "@phosphor-icons/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { Button } from "../../../ui/components/Button";
import { DeleteButton } from "../../../ui/components/EditorWindow";
import { IconButton } from "../../../ui/components/IconButton";

export function InspectorPane({
  title,
  description,
  dirty,
  canSave = true,
  actions,
  onSave,
  onClose,
  children,
}: {
  title: string;
  /** One quiet line under the title, only where it says something the fields do not. */
  description?: string;
  /** Whether the draft differs from what the graph holds. */
  dirty: boolean;
  canSave?: boolean;
  /** A control for the left of the foot, such as Delete. */
  actions?: ReactNode;
  onSave?: () => void;
  onClose: () => void;
  children: ReactNode;
}) {
  const [asking, setAsking] = useState(false);
  const close = () => (dirty ? setAsking(true) : onClose());
  // Escape closes the pane wherever the focus is, unless a window or menu over the page has taken the key.
  const closing = useRef(close);
  closing.current = close;
  useEffect(() => {
    const listen = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) closing.current();
    };
    window.addEventListener("keydown", listen);
    return () => window.removeEventListener("keydown", listen);
  }, []);
  return (
    <aside
      aria-label={title}
      data-testid="editor-window"
      className="flex w-80 shrink-0 flex-col border-l border-hairline bg-panel"
    >
      <div className="flex min-h-12 shrink-0 items-center gap-2 border-b border-hairline pr-2 pl-4">
        <div className="flex min-w-0 flex-1 flex-col">
          <h2 className="m-0 truncate text-md font-semibold text-text">{title}</h2>
          {description ? (
            <p className="m-0 truncate text-xs text-secondary" title={description}>
              {description}
            </p>
          ) : null}
        </div>
        <IconButton icon={XIcon} aria-label="Close" title="Close" onClick={close} />
      </div>
      <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto p-4 [&_label>span:first-child]:font-medium [&_textarea]:resize-none">
        {children}
      </div>
      {asking ? (
        <div
          role="alertdialog"
          aria-label="Discard changes?"
          className="flex shrink-0 flex-wrap items-center gap-2 border-t border-hairline px-4 py-2"
        >
          <p className="m-0 min-w-0 flex-1 text-sm text-text">Discard changes?</p>
          <Button variant="secondary" size="sm" autoFocus onClick={() => setAsking(false)}>
            Keep editing
          </Button>
          <DeleteButton
            onClick={() => {
              setAsking(false);
              onClose();
            }}
          >
            Discard
          </DeleteButton>
        </div>
      ) : (
        <div className="flex min-h-12 shrink-0 flex-wrap items-center gap-2 border-t border-hairline px-4 py-2">
          {actions ? <div className="mr-auto flex gap-2">{actions}</div> : null}
          {onSave ? (
            <div className="ml-auto flex gap-2">
              <Button variant="secondary" size="sm" onClick={close}>
                Cancel
              </Button>
              <Button variant="secondary" size="sm" disabled={!canSave || !dirty} onClick={onSave}>
                Save
              </Button>
            </div>
          ) : null}
        </div>
      )}
    </aside>
  );
}
