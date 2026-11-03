/**
 * The Fleet page (the desktop app's `screens/fleet`): which runners are up and what they are running. One row for each
 * card whose latest run is starting, running or waiting and for each chat that is, led by the status word and the hue
 * that goes with it, then who runs it, what for, how long it has run, and the last steps of the run. The rows read
 * their events when the page opens and follow the stream's `run_event`s while it is open; a row opens the card's board
 * or the chat. A read that fails says what could not be read and offers to try again, rather than saying that nothing
 * is working.
 */
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { chats as readChats } from "../../api/calls/chat";
import { run as readRun } from "../../api/calls/fleet";
import { keys } from "../../api/keys";
import { inputPath, inputSummary, toolTitle } from "../../chat/conversation";
import { useThread } from "../../chat/useThread";
import { thinkingLabel } from "../../ui/chat/ChatCollapsedThinking";
import { ToolStatusDot } from "../../ui/chat/ToolStatusDot";
import { toolStatus } from "../../ui/chat/Turn";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { Status } from "../../ui/page/Status";
import {
  afterSteps,
  context,
  elapsed,
  latestSteps,
  statusWord,
  thinkingNow,
  thoughtSeconds,
  tokensText,
  type Work,
  working,
} from "./model";

export function FleetPage() {
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const chats = useQuery({ queryKey: keys.chats, queryFn: readChats });
  // How long a run has been going is said in seconds, so this page's clock ticks faster than the rest.
  const now = useNow(5_000);
  if (data.isPending || chats.isPending) {
    return (
      <>
        <PageHeader title="Fleet" />
        <SkeletonList rows={2} message="Reading the fleet" />
      </>
    );
  }
  // A page that could not be read says so. Falling through to the empty state would claim that no agent is working,
  // which is the opposite of what happened, so the failure comes first and carries the way back.
  if (data.isError || chats.isError) {
    return (
      <>
        <PageHeader title="Fleet" />
        <ReadFailed
          error={data.isError ? data.error : chats.error}
          fallback="Could not read what the agents are doing. Try again."
          busy={data.isFetching || chats.isFetching}
          onRetry={() => {
            void data.refetch();
            void chats.refetch();
          }}
        />
      </>
    );
  }
  const list = data.data ? working(data.data, chats.data ?? []) : [];
  return (
    <>
      <PageHeader title="Fleet" context={context(list.length)} />
      {list.length === 0 ? (
        <Empty
          title="No agent is working"
          body="A row appears here while a card's run or a chat has its agent at work, with what each is doing and how long it has been."
          action={
            <Button asChild variant="secondary">
              <Link to="/chat">Start a chat</Link>
            </Button>
          }
        />
      ) : (
        // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
        <ul role="list" className="m-0 flex list-none flex-col gap-3 p-0">
          {list.map((work) => (
            <li key={work.run}>
              <Row work={work} now={now} />
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function Row({ work, now }: { work: Work; now: number }) {
  const navigate = useNavigate();
  const thread = useThread(work.run);
  // A card's run says when it started; a chat's start is in the list of chats.
  const run = useQuery({
    queryKey: ["run", work.run],
    queryFn: () => readRun(work.run),
    enabled: work.startedAt === 0,
  });
  const startedAt = work.startedAt || run.data?.started_at || 0;
  // A thought says how long it went on, which only the step after it can say. The lines drawn are the last few of the
  // run, so each is measured against the whole of them.
  const recent = latestSteps(thread.entries);
  const steps = recent.slice(-4);
  const atWork = thinkingNow(work.status);
  // What came after the last step — the answer, usually — is what says the last of them stopped.
  const ended = afterSteps(thread.entries);
  const status = statusWord(work.status, work.asks);
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
      className="flex w-full flex-col gap-3 rounded-card bg-surface p-4 text-left shadow-card transition-colors duration-(--motion-routine) ease-out hover:bg-hover"
    >
      <span className="flex flex-wrap items-center gap-3">
        <Status tone={status.tone}>{status.word}</Status>
        <span className="text-base font-semibold text-text">{work.agent}</span>
        <span className="text-sm text-secondary">{work.kind}</span>
        {startedAt > 0 && (
          <span className="ml-auto text-sm tabular-nums text-secondary">
            {elapsed(now - startedAt)}
          </span>
        )}
      </span>
      <span className="break-words text-lg font-semibold text-text">{work.title}</span>
      {about && <span className="break-words text-sm text-secondary">{about}</span>}
      {steps.length === 0 ? (
        <span className="text-sm text-secondary">The agent has not made a step yet.</span>
      ) : (
        <span className="flex flex-col gap-2 border-t border-hairline pt-3">
          <span className="text-sm font-semibold text-secondary">
            {work.status === "waiting" ? `Worked for ${worked ?? "a moment"}` : "Agent is working"}
          </span>
          {steps.map((entry, shown) => {
            const index = recent.length - steps.length + shown;
            // Only the last line of a run that is at work is still arriving; every thought before it has stopped.
            const ongoing = atWork && index === recent.length - 1;
            return (
              // The lines are drawn once, in order, and `index` counts them from the start of the run.
              <span key={index} className="flex items-center gap-2 text-sm text-secondary">
                {entry.kind === "tool" ? (
                  <>
                    <ToolStatusDot status={toolStatus(entry.call)} className="shrink-0" />
                    <span className="shrink-0 font-mono text-text">{toolTitle(entry.call)}</span>
                    <span className="break-words" title={inputPath(entry.call.input) ?? undefined}>
                      {inputSummary(entry.call.input)}
                    </span>
                  </>
                ) : (
                  <>
                    <span
                      aria-hidden="true"
                      className={cn(
                        "size-2 shrink-0 rounded-full",
                        ongoing ? "bg-progress" : "bg-track",
                      )}
                    />
                    <span>{thinkingLabel(thoughtSeconds(recent, index, ended), ongoing)}</span>
                  </>
                )}
              </span>
            );
          })}
        </span>
      )}
    </button>
  );
}
