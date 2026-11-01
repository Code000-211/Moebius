/** What the Ports page says about a port: who it belongs to, and where that is. */
import type { Port } from "../../api/types/runs";

/** What a port belongs to, in a few words (the desktop app's `owner`). */
export function owner(port: Port): string {
  switch (port.kind) {
    case "run":
      return port.run === null ? "run" : `run ${port.run}`;
    case "chat":
      return port.run === null ? "chat" : `chat ${port.run}`;
    case "terminal":
      return port.run === null ? "server terminal" : `terminal of run ${port.run}`;
  }
}

/** The page a port's owner has: the run's, the chat's; none for the server's own terminal. */
export function ownerPath(port: Port): string | null {
  if (port.run === null) return null;
  return port.kind === "chat" ? `/chat/${port.run}` : `/runs/${port.run}`;
}

/**
 * The command that carries a port of the server to the same port on this machine, for opening it in a tab. The
 * server's own tunnel (`GET /v1/ports/{port}/tunnel`) speaks bytes over a WebSocket, which a page cannot open.
 */
export function forwardCommand(port: number, host: string): string {
  return `ssh -N -L ${port}:127.0.0.1:${port} ${host}`;
}
