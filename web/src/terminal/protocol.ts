/**
 * The WebSocket protocol of the server's terminals (docs/reference/desktop-api.md, `GET /v1/runs/{id}/terminal`): binary
 * messages carry bytes both ways, the keys to the shell and what the shell prints back, and a text message
 * `{"type":"resize","cols":N,"rows":M}` sets the size in cells (80 by 24 until then). The same for `/v1/chats/{id}/tui`,
 * `/v1/terminal` and `/v1/repositories/{id}/terminal`. These are the pure parts of that, written to match
 * `crates/moebius-desktop/src/client/terminal.rs`; `Terminal.tsx` draws them.
 */

/** The most cells a side of the terminal has: the server takes `u16`. */
export const MAX_CELLS = 65_535;

/** The WebSocket address of `path` (such as `/v1/terminal`) on the server that served the page. */
export function terminalUrl(location: Pick<Location, "protocol" | "host">, path: string): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${location.host}${path}`;
}

/** What a person typed or pasted, as the bytes the shell reads (UTF-8). */
export function inputBytes(text: string): Uint8Array {
  return new TextEncoder().encode(text);
}

/** What xterm calls binary data (a string of characters 0 to 255, such as a mouse report) as the bytes it stands for. */
export function binaryBytes(binary: string): Uint8Array {
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i) & 0xff;
  return bytes;
}

/** The text message that sets the size of the terminal; none for a size the server cannot take. */
export function resizeMessage(cols: number, rows: number): string | null {
  const fits = (n: number) => Number.isInteger(n) && n >= 1 && n <= MAX_CELLS;
  if (!fits(cols) || !fits(rows)) return null;
  return JSON.stringify({ type: "resize", cols, rows });
}

/** What the shell printed, from the data of a WebSocket message; a text message carries none. */
export function outputBytes(data: unknown): Uint8Array | null {
  return data instanceof ArrayBuffer ? new Uint8Array(data) : null;
}

/**
 * What to tell a person when a terminal ended. A browser is not told why a WebSocket was refused, only that it did
 * not open; the server refuses the run token of an agent (403), a run or chat that has no worktree (422 or 409), and
 * a fifth terminal (422).
 */
export function endMessage(opened: boolean): string {
  return opened
    ? "The shell ended."
    : "The server did not open the terminal. Only a person can do this, so it is refused for a run token; " +
        "a run or chat needs its worktree (it is gone when the run has ended) and a chat must be waiting; " +
        "and at most 4 terminals are open at a time.";
}
