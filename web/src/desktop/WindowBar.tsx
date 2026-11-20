/**
 * A strip along the top of the window for the screens that have no shell around them (sign-in, the server not
 * answering, the loader): it moves the window when dragged and has the one control a person needs there, close. In a
 * browser nothing is drawn. Inside the shell the window's controls are `WindowControls`, in the panel.
 */
import { XIcon } from "@phosphor-icons/react";
import { IconButton } from "../ui/components/IconButton";
import { type DesktopBridge, desktopBridge } from "./bridge";

export function WindowBar({ bridge = desktopBridge() }: { bridge?: DesktopBridge }) {
  if (!bridge) return null;
  return (
    <div
      data-tauri-drag-region
      data-testid="window-bar"
      className="fixed inset-x-0 top-0 z-sheet flex h-9 items-center justify-end px-1.5"
    >
      <IconButton
        iconClassName="size-3.5"
        iconWeight="regular"
        icon={XIcon}
        aria-label="Close"
        onClick={() => bridge.close()}
      />
    </div>
  );
}
