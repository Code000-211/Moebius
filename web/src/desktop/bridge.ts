/**
 * What the desktop app (`crates/moebius-desktop`) gives the web app when it shows it in its window:
 * `window.moebiusDesktop`, set before the app's own scripts run. In a browser it is missing, and the web app draws
 * nothing of the desktop app's.
 */

/** One server of the rail, as the app reports it. */
export interface RailServer {
  id: string;
  name: string;
  /** The server shown in the window. */
  open: boolean;
  /** False when it did not answer with its token: the rail draws a grey dot. */
  reachable: boolean;
  /** The count of its Inbox. */
  inbox: number;
}

/** What an error of `connectSsh` starts with when the host has no server set up, so the rail can ask to set one up. */
export const NO_SERVER = "no-server:";

/**
 * The window of the desktop app, which has no system frame of its own, and its rail of servers. The commands of the
 * rail answer with a sentence for the person when they fail; none returns a token. Opening a server replaces the page,
 * so the promise of `switchServer`, `connect` and `connectSsh` is mostly not seen.
 */
export interface DesktopBridge {
  minimize(): void;
  toggleMaximize(): void;
  close(): void;
  servers(): Promise<RailServer[]>;
  switchServer(id: string): Promise<void>;
  removeServer(id: string): Promise<void>;
  /** Adds the server at `address` with its access token, and opens it. */
  connect(address: string, token: string): Promise<void>;
  /** Adds the server of an SSH host and opens it, setting it up there first when `setUp`. */
  connectSsh(host: string, setUp: boolean): Promise<void>;
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
