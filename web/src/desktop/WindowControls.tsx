/**
 * The window's controls in the desktop app: a strip along the top of the window on the textured ground, which moves
 * the window when dragged (and maximizes it on a double click), with minimize, maximize and close at its right end.
 * The shell leaves the strip's height free above the sidebar and the page (`WINDOW_BAR_CLASS`); in a browser
 * nothing is drawn and nothing is left free.
 */
import { MinusIcon, SquareIcon, XIcon } from "@phosphor-icons/react";
import { IconButton } from "../ui/components/IconButton";
import { type DesktopBridge, desktopBridge } from "./bridge";

/** The top padding of the shell's ground in the desktop app: the height of the strip. */
export const WINDOW_BAR_CLASS = "pt-9";

export function WindowControls({ bridge = desktopBridge() }: { bridge?: DesktopBridge }) {
  if (!bridge) return null;
  return (
    <div
      data-tauri-drag-region
      data-testid="window-bar"
      className="fixed inset-x-0 top-0 z-sheet flex h-9 items-center justify-end gap-0.5 px-1.5"
    >
      <IconButton icon={MinusIcon} aria-label="Minimize" onClick={() => bridge.minimize()} />
      <IconButton icon={SquareIcon} aria-label="Maximize" onClick={() => bridge.toggleMaximize()} />
      <IconButton icon={XIcon} aria-label="Close" onClick={() => bridge.close()} />
    </div>
  );
}
