import type { TagTone } from "../ui/chat/Tag";

/**
 * The label of a chat's status for its tag, and the state the tag is coloured by; none for a chat that waits without
 * asking. Work under way takes the hue of work in progress, a question takes the one of attention, and a chat that is
 * over takes none — the word says it.
 */
export function statusTag(status: string, asks: boolean): { label: string; tone: TagTone } | null {
  switch (status) {
    case "starting":
      return { label: "Starting", tone: "progress" };
    case "running":
      return { label: "Running", tone: "progress" };
    case "waiting":
      return asks ? { label: "Asks you", tone: "attention" } : null;
    case "succeeded":
      return { label: "Ended", tone: "plain" };
    case "failed":
      return { label: "Failed", tone: "error" };
    case "stopped":
      return { label: "Agent disconnected", tone: "plain" };
    default:
      return { label: status, tone: "plain" };
  }
}

/**
 * What the field says for a chat that takes no message: why it takes none, and what starts one again. The tag in
 * the title row names the state; the field answers the question the person is actually asking — can I go on here?
 */
export function endedHint(status: string, agent: string): string {
  const gone: Record<string, string> = {
    stopped: `${agent} disconnected`,
    failed: `${agent} stopped answering`,
    succeeded: `${agent} ended this chat`,
  };
  return `${gone[status] ?? "This chat has ended"}. Start a new chat to continue.`;
}

/** Whether a chat can still be written to. */
export function isOpen(status: string): boolean {
  return status === "starting" || status === "running" || status === "waiting";
}

/** Whether the chat's agent is at work. */
export function isWorking(status: string): boolean {
  return status === "starting" || status === "running";
}
