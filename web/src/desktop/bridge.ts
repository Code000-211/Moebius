/**
 * What the desktop app (`crates/moebius-desktop`) gives the web app when it shows it in its window:
 * `window.moebiusDesktop`, set before the app's own scripts run. In a browser it is missing, and the web app draws
 * nothing of the desktop app's.
 *
 * The same object also arrives in the app's small window of the global shortcut (`/quick`, see `QuickWindow.tsx`),
 * where it is only `quick`; the names of the events the app sends a page are the ones the Rust side writes in
 * `crates/moebius-desktop/src/quick.rs`.
 */

/** One server of the rail, as the app reports it. */
export interface RailServer {
  id: string;
  /** What the square is called: the server's own name or its address, and with the address added when another server
   * has the same name; the person can change it. */
  name: string;
  /** The address the server is reached at (of one over SSH, the host). */
  address: string;
  /** The server shown in the window. */
  open: boolean;
  /** False when it did not answer with its token: the rail draws a grey dot. */
  reachable: boolean;
  /** The count of its Inbox as the sidebar counts it: `items` and `unread` together. */
  inbox: number;
  /** The items of its inbox that are not resolved. */
  items: number;
  /** Its notices that wait for the person (given to them or mentioning them). */
  unread: number;
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
  /** Names a server; an empty name gives back the server's own. */
  renameServer(id: string, name: string): Promise<void>;
  /** Adds the server at `address` with its access token, and opens it. */
  connect(address: string, token: string): Promise<void>;
  /** Adds the server of an SSH host and opens it, setting it up there first when `setUp`. */
  connectSsh(host: string, setUp: boolean): Promise<void>;
}

/** What the small window of the global shortcut gets as `window.moebiusDesktop`: only `quick`, not the main window's. */
export interface QuickDesktop {
  quick: QuickBridge;
}

/** What the small window of the global shortcut asks of the app. */
export interface QuickBridge {
  /** The shortcut that opens the window, as the app writes it (`Ctrl+Shift+Space`), and why it is not in force. */
  shortcut(): Promise<{ shortcut: string; error: string | null }>;
  /** Changes the shortcut and keeps it for the next start; the answer is the shortcut as the app writes it. */
  setShortcut(shortcut: string): Promise<string>;
  /** Closes the small window, and shows `path` (`/chat/5`) in the app's main window when a path is given. */
  finish(path?: string): Promise<void>;
}

/** The shortcut the app starts with; the app's own default is the one in `src/quick.rs`. */
export const DEFAULT_SHORTCUT = "Ctrl+Shift+Space";

/** Sent to the main window's page: `detail` is the path of the web app to open. */
export const OPEN_EVENT = "moebius:open";

/** Sent to the small window's page each time it is shown again. */
export const SHOWN_EVENT = "moebius:quick-shown";

declare global {
  interface Window {
    moebiusDesktop?: DesktopBridge;
  }
}

/** The bridge of the desktop app, or undefined in a browser. */
export function desktopBridge(): DesktopBridge | undefined {
  return typeof window === "undefined" ? undefined : window.moebiusDesktop;
}

/** The small window's calls to the app, or undefined outside that window. */
export function quickBridge(): QuickBridge | undefined {
  return (desktopBridge() as Partial<QuickDesktop> | undefined)?.quick;
}
