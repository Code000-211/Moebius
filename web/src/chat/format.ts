/** How long ago a chat started, as the list says it: `just now`, `5 min ago`, `3 h ago`, `2 d ago`. */
export function ago(now: number, then: number): string {
  const seconds = Math.max(0, now - then);
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} h ago`;
  return `${Math.floor(seconds / 86_400)} d ago`;
}

/** The label of a chat's status for its tag, and its tone; none for a chat that waits without asking. */
export function statusTag(status: string, asks: boolean): { label: string; tone: Tone } | null {
  switch (status) {
    case "starting":
      return { label: "Starting", tone: "teal" };
    case "running":
      return { label: "Running", tone: "teal" };
    case "waiting":
      return asks ? { label: "Asks you", tone: "amber" } : null;
    case "succeeded":
      return { label: "Ended", tone: "plain" };
    case "failed":
      return { label: "Failed", tone: "red" };
    case "stopped":
      return { label: "Stopped", tone: "plain" };
    default:
      return { label: status, tone: "plain" };
  }
}

export type Tone = "teal" | "amber" | "red" | "plain";

/** Whether a chat can still be written to. */
export function isOpen(status: string): boolean {
  return status === "starting" || status === "running" || status === "waiting";
}

/** Whether the chat's agent is at work. */
export function isWorking(status: string): boolean {
  return status === "starting" || status === "running";
}
