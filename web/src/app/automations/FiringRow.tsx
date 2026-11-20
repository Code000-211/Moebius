/**
 * One past run of an automation, a row of the "Past runs" box of its editor: what came of it and its status word, then
 * when it ran and what asked for it. A run that made a chat links to it.
 */
import { Link } from "react-router";
import type { Firing } from "../../api/types/automations";
import type { TagTone } from "../../ui/chat/Tag";
import { Tag } from "../../ui/chat/Tag";
import { ago } from "../../ui/lib/format";

/** A firing: what came of it and its status word, then when it ran and what asked for it. */
export function FiringRow({ firing, now }: { firing: Firing; now: number }) {
  // A run that started is told by what its chat became: over and well is done, not "started".
  const over = firing.status === "started" && firing.chat_status === "succeeded";
  const lost =
    firing.status === "started" &&
    (firing.chat_status === "failed" || firing.chat_status === "stopped");
  const word = over ? "done" : lost ? firing.chat_status : firing.status;
  const tone: TagTone = over
    ? "complete"
    : lost
      ? "error"
      : firing.status === "started"
        ? "progress"
        : firing.status === "skipped"
          ? "attention"
          : "error";
  const what =
    firing.run_id === null
      ? firing.reason
      : firing.chat_status
        ? `Chat ${firing.run_id} · ${firing.chat_status}`
        : `Chat ${firing.run_id}`;
  const body = (
    <>
      <span className="min-w-0 flex-1 truncate text-text">{what}</span>
      <Tag tone={tone}>{word}</Tag>
      <span className="shrink-0 text-xs text-muted tabular-nums">{ago(now, firing.at)}</span>
      <span className="w-16 shrink-0 text-right text-xs text-muted">
        {firing.manual ? "Run now" : "Schedule"}
      </span>
    </>
  );
  const row = "flex min-h-9 items-center gap-2.5 border-b border-hairline py-1.5 text-base";
  return firing.run_id === null ? (
    <div className={row}>{body}</div>
  ) : (
    <Link to={`/chat/${firing.run_id}`} className={`${row} hover:bg-row-hover`}>
      {body}
    </Link>
  );
}
