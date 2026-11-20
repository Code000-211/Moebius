/**
 * The window's controls in the desktop app, which has no system frame of its own: minimize, maximize and close, drawn
 * once at the top right of the shell's panel, level with the page header, after a hairline that sets them apart from
 * the header's own buttons. The header is the drag region that moves the window (and maximizes it on a double click);
 * the shell reserves the controls' width in `--window-buttons` for it (`WINDOW_BUTTONS_WIDTH`). In a browser nothing is
 * drawn and nothing is reserved. The screens outside the shell have `WindowBar` instead.
 */
import { MinusIcon, SquareIcon, XIcon } from "@phosphor-icons/react";
import { IconButton } from "../ui/components/IconButton";
import { type DesktopBridge, desktopBridge } from "./bridge";

/** The room the controls take at the right of the panel's header, hairline and edge included. */
export const WINDOW_BUTTONS_WIDTH = "8rem";

export function WindowControls({ bridge = desktopBridge() }: { bridge?: DesktopBridge }) {
  if (!bridge) return null;
  return (
    <div
      data-testid="window-controls"
      className="absolute top-0 right-1.5 z-sheet flex h-12 items-center gap-0.5"
    >
      <span aria-hidden="true" className="mr-2 h-5 w-px bg-hairline" />
      <IconButton icon={MinusIcon} aria-label="Minimize" onClick={() => bridge.minimize()} />
      <IconButton icon={SquareIcon} aria-label="Maximize" onClick={() => bridge.toggleMaximize()} />
      <IconButton icon={XIcon} aria-label="Close" onClick={() => bridge.close()} />
    </div>
  );
}
