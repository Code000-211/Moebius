/**
 * The window of the desktop app is transparent and has no frame of the system, so its page draws the corners
 * (`tailwind.css`, `html[data-desktop]`). This marks the page as the app's, the moment it starts; whether the window is
 * maximised or full screen is `data-maximized`, which the app sets (`crates/moebius-desktop/src/frame.rs`). In a browser
 * nothing is marked.
 */
import { desktopBridge } from "./bridge";

export function start(root: HTMLElement = document.documentElement): void {
  if (desktopBridge()) root.setAttribute("data-desktop", "");
}
