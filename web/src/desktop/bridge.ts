/**
 * What the desktop app (`crates/moebius-desktop`) gives the web app when it shows it in its window:
 * `window.moebiusDesktop`, set before the app's own scripts run. In a browser it is missing, and the web app draws
 * nothing of the desktop app's.
 */

/** The window of the desktop app, which has no system frame of its own. */
export interface DesktopBridge {
  minimize(): void;
  toggleMaximize(): void;
  close(): void;
}

declare global {
  interface Window {
    moebiusDesktop?: DesktopBridge;
  }
}

/** The bridge of the desktop app, or undefined in a browser. */
export function desktopBridge(): DesktopBridge | undefined {
  return typeof window === "undefined" ? undefined : window.moebiusDesktop;
}
