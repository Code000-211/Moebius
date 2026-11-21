/**
 * A strip along the top of the window for the screens that have no shell around them (sign-in, the server not
 * answering, the loader): it moves the window when dragged and has the same three controls as the shell: minimize, maximize and close. In a
 * browser nothing is drawn. Inside the shell the window's controls are `WindowControls`, in the page.
 */
import { type DesktopBridge, desktopBridge } from "./bridge";
import { WindowButtons } from "./WindowControls";

export function WindowBar({ bridge = desktopBridge() }: { bridge?: DesktopBridge }) {
  if (!bridge) return null;
  return (
    <div
      data-tauri-drag-region
      data-testid="window-bar"
      className="fixed inset-x-0 top-0 z-sheet flex h-12 items-center justify-end gap-0.5 px-2"
    >
      <WindowButtons bridge={bridge} />
    </div>
  );
}
