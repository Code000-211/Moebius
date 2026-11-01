/**
 * The Fleet page (the desktop app's `screens/fleet`): every run that is working, at a glance. One tile for each card
 * whose latest run is starting, running or waiting and for each chat that is, with who runs it, what for, how long,
 * and the last steps of the run. The tiles read their events when the page opens and follow the stream's `run_event`s
 * while it is open; a click on a tile opens the card's board or the chat.
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { chats as readChats } from "../../api/calls/chat";
import { run as readRun } from "../../api/calls/fleet";
import { keys } from "../../api/keys";
import { inputSummary, toolTitle } from "../../chat/conversation";
import { useThread } from "../../chat/useThread";
import { Tag } from "../../ui/chat/Tag";
import { ToolStatusDot } from "../../ui/chat/ToolStatusDot";
import { toolStatus } from "../../ui/chat/Turn";
import { Loader } from "../../ui/components/Loader";
import { Box } from "../../ui/page/Box";
import { Header } from "../shell/Page";
import { context, elapsed, latestSteps, tokensText, type Work, working } from "./model";

/** The time now in Unix seconds, kept current every 5 seconds for how long the runs have run. */
function useNow(): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 5_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

export function FleetPage() {
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const chats = useQuery({ queryKey: keys.chats, queryFn: readChats });
  const now = useNow();
  if (data.isPending || chats.isPending) {
    return <Loader message="Reading the fleet…" className="py-double" />;
  }
  const list = data.data ? working(data.data, chats.data ?? []) : [];
  return (
    <>
      <Header title="Fleet" />
      <p className="m-0 mb-[12px] font-mono text-[12.5px] text-normal">{context(list.length)}</p>
      {list.length === 0 ? (
        <Box>
          <p className="m-0 text-[13px] text-normal">Nothing is working.</p>
        </Box>
      ) : (
        <div className="grid gap-[12px] [grid-template-columns:repeat(auto-fill,minmax(min(340px,100%),1fr))]">
          {list.map((work) => (
            <Tile key={work.run} work={work} now={now} />
          ))}
        </div>
      )}
    </>
  );
}

function Tile({ work, now }: { work: Work; now: number }) {
  const navigate = useNavigate();
  const thread = useThread(work.run);
  // A card's run says when it started; a chat's start is in the list of chats.
  const run = useQuery({
    queryKey: ["run", work.run],
    queryFn: () => readRun(work.run),
    enabled: work.startedAt === 0,
  });
  const startedAt = work.startedAt || run.data?.started_at || 0;
  const steps = latestSteps(thread.entries).slice(-4);
  const open = () =>
    navigate(
      work.open.kind === "card"
        ? `/board/${work.open.repository}?card=${work.open.card}`
        : `/chat/${work.open.chat}`,
    );
  const about = [work.repository, work.tokens === null ? "" : tokensText(work.tokens)]
    .filter((part) => part !== "")
    .join(" · ");
  const first = thread.entries.find((entry) => entry.kind === "person" || entry.kind === "tool");
  const lastAt = thread.entries.at(-1)?.at ?? now;
  const stops = work.status === "waiting" ? lastAt : now;
  const worked = first ? elapsed(stops - first.at) : null;
  return (
    <button
      type="button"
      data-testid="fleet-tile"
      onClick={open}
      className="flex flex-col gap-[6px] rounded-xl border bg-surface p-[14px] text-left transition-colors hover:border-low"
    >
      <div className="flex items-center gap-base">
        <span className="text-[12.5px] font-bold text-high">{work.agent}</span>
        <Tag>{work.kind}</Tag>
        {work.asks && <Tag tone="amber">asks you</Tag>}
        <span className="flex-1" />
        {startedAt > 0 && (
          <span className="font-mono text-[11px] text-normal">{elapsed(now - startedAt)}</span>
        )}
      </div>
      <span className="break-words text-[13.5px] font-semibold text-high">{work.title}</span>
      {about && <span className="truncate font-mono text-[11px] text-low">{about}</span>}
      {steps.length === 0 ? (
        <span className="font-mono text-[11px] text-low">No steps yet.</span>
      ) : (
        <div className="flex flex-col gap-half rounded-md border bg-lane p-base">
          <span className="text-[12px] text-normal">
            {work.status === "waiting" ? `Worked for ${worked ?? "a moment"}` : "Agent is working"}
          </span>
          {steps.map((entry, index) => (
            <span
              // biome-ignore lint/suspicious/noArrayIndexKey: the lines are drawn once, in order
              key={index}
              className="flex items-center gap-base truncate text-[12px] text-low"
            >
              {entry.kind === "tool" ? (
                <>
                  <ToolStatusDot status={toolStatus(entry.call)} />
                  <span className="font-mono text-normal">{toolTitle(entry.call)}</span>
                  <span className="truncate">{inputSummary(entry.call.input)}</span>
                </>
              ) : (
                <span>Thinking</span>
              )}
            </span>
          ))}
        </div>
      )}
    </button>
  );
}
