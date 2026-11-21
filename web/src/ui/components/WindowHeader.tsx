/**
 * The top bar every centred window has: 48px high with a hairline below, what the window is about at the left (an
 * id, or a kind with the file it concerns), the title where the window does not show it below, a trailing slot for
 * what leads to the item's own page ("Open as page"), and the close icon, always last. The OS window buttons stay in
 * the shell; the right of a dialog is free of them. It draws and decides nothing: the window gives it the titles
 * and the way out (`EditorWindow`, the card window, the new-agent choice).
 */

import { XIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";
import { IconButton } from "./IconButton";

export function WindowHeader({
  leading,
  title,
  trailing,
  onClose,
  closeLabel = "Close",
  className,
}: {
  /** What the window is about, in a word or an id; a mono part (a file name) goes in `<span className="font-mono">`. */
  leading?: ReactNode;
  /** The window's title, when it is not shown under the bar; the caller makes it the dialog's title. */
  title?: ReactNode;
  /** Controls before the close icon, such as "Open as page". */
  trailing?: ReactNode;
  /** The way out; a window shown as a page has none and so no close icon. */
  onClose?: () => void;
  closeLabel?: string;
  className?: string;
}) {
  return (
    <div
      data-testid="window-header"
      className={cn(
        "flex h-12 shrink-0 items-center gap-2 border-b border-hairline pr-2 pl-6",
        className,
      )}
    >
      {leading !== undefined && (
        <div className="flex min-w-0 shrink items-center gap-2 truncate text-sm text-secondary">
          {leading}
        </div>
      )}
      {title !== undefined ? (
        <div className="min-w-0 flex-1 truncate text-md font-semibold">{title}</div>
      ) : (
        <div className="flex-1" />
      )}
      {trailing !== undefined && <div className="flex shrink-0 items-center gap-1">{trailing}</div>}
      {onClose && (
        <IconButton icon={XIcon} iconWeight="regular" aria-label={closeLabel} onClick={onClose} />
      )}
    </div>
  );
}
