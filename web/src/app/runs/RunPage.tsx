/**
 * The page of one run (`/runs/:id`): what the run is doing, then the log of what it has done. The head says who runs,
 * where it stands and which card it works for; the conversation is the run's events drawn as the chat draws them
 * (`chat/useThread.ts` follows the event stream, the entries are `ui/chat/`), in a box of its own that scrolls rather
 * than pushing the page wide, with a box under it to steer a running run (`POST /v1/runs/{id}/steer`); the terminal is
 * a shell in the run's worktree while it has one; the changes are what the run changed so far, as a diff. The terminal
 * stays open while another view is shown, so it is not lost by a glance at the conversation. Under the head a bar
 * spans the run in time with a tick for each tool step; the steps themselves are the small lines of the conversation.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { Link, useParams } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { run as readRun, steer } from "../../api/calls/runs";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Snapshot } from "../../api/types/board";
import { Composer } from "../../chat/Composer";
import type { Entry } from "../../chat/conversation";
import { isOpen, isWorking } from "../../chat/format";
import { ThreadView } from "../../chat/ThreadView";
import { useThread } from "../../chat/useThread";
import { Changes } from "../../diff/Changes";
import { useServerEvents } from "../../live/LiveProvider";
import { Terminal } from "../../terminal/LazyTerminal";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { elapsed, plural } from "../../ui/lib/format";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonCard } from "../../ui/page/Skeleton";
import type { Tone } from "../../ui/page/StateLabel";
import { StateLabel } from "../../ui/page/StateLabel";
import { TabPanel, Tabs } from "../../ui/page/Tabs";
import { TimeBar } from "../../ui/page/TimeBar";
import { repositoryBoardPath } from "../pages";

type Tab = "conversation" | "terminal" | "changes";

/** Where a run stands, in a word, with the tone that goes with it. */
const STANDING: Record<string, { word: string; tone: Tone }> = {
  starting: { word: "Starting", tone: "progress" },
  running: { word: "Running", tone: "progress" },
  waiting: { word: "Your turn", tone: "attention" },
  succeeded: { word: "Ended", tone: "complete" },
  failed: { word: "Failed", tone: "error" },
  stopped: { word: "Stopped", tone: "neutral" },
};

/** Where a run lies in time: the days it starts and ends on (for the bar), and its length in seconds. */
function runSpan(startedAt: number, endedAt: number | null, now: number) {
  const end = Math.max(endedAt ?? now, startedAt);
  const day = (t: number) => new Date(t * 1000).toISOString().slice(0, 10);
  return {
    start: startedAt,
    end,
    startDay: day(startedAt),
    endDay: day(end),
    seconds: end - startedAt,
  };
}

/** The places of the thread's tool steps along the run, as percent of its length. */
function stepMarks(entries: Entry[], span: { start: number; seconds: number }): number[] {
  return entries
    .filter((entry) => entry.kind === "tool")
    .map((entry) => (span.seconds > 0 ? ((entry.at - span.start) / span.seconds) * 100 : 0));
}

/** The card a run works for, with the board it is on, found in the snapshot. */
function cardOf(snapshot: Snapshot | undefined, cardId: number | null) {
  if (!snapshot || cardId === null) return null;
  for (const board of snapshot.boards) {
    const card = board.cards.find((each) => each.id === cardId);
    if (card) return { card, repository: board.repository };
  }
  return null;
}

export function RunPage() {
  const params = useParams();
  const id = params.id !== undefined && /^\d+$/.test(params.id) ? Number(params.id) : null;
  if (id === null) {
    return (
      <>
        <PageHeader title="Run" />
        <Empty
          title="There is no such run"
          body="The server has no run with that number. Open one from its card or from the Fleet page."
        />
      </>
    );
  }
  return <OpenRun key={id} id={id} />;
}

function OpenRun({ id }: { id: number }) {
  const run = useQuery({ queryKey: keys.run(id), queryFn: () => readRun(id) });
  const snapshot = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const thread = useThread(id);
  const [tab, setTab] = useState<Tab>("conversation");
  // The id that binds the row of tabs to the view it chooses.
  const views = useId();
  const [terminalSeen, setTerminalSeen] = useState(false);
  const [message, setMessage] = useState("");
  const [sent, setSent] = useState(0);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refresh = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const queryClient = useQueryClient();

  // What the run changed moves while its agent works: read the diff again soon after each tool ran.
  useServerEvents((event) => {
    if (event.kind !== "run_event" || event.data.run !== id) return;
    if (event.data.kind !== "tool_execution_end") return;
    clearTimeout(refresh.current);
    refresh.current = setTimeout(
      () => void queryClient.invalidateQueries({ queryKey: keys.runDiff(id) }),
      500,
    );
  });
  useEffect(() => () => clearTimeout(refresh.current), []);

  // One header for every state of the page; the tabs join it once the run is read.
  if (run.isPending || run.isError || !run.data) {
    const missing = isApiError(run.error) && run.error.status === 404;
    return (
      <>
        <PageHeader title={`Run ${id}`} />
        {run.isPending ? (
          <SkeletonCard lines={5} message="Reading the run" />
        ) : missing ? (
          <Empty
            title="There is no such run"
            body="It may have been removed. Open a run from the Fleet page."
          />
        ) : (
          <Empty
            title="Cannot read the run"
            body="The server did not answer for this run."
            action={
              <span className="flex flex-wrap items-center gap-3">
                {/* The way back is to read it again, so the page offers that before anywhere else to go. */}
                <Button disabled={run.isFetching} onClick={() => void run.refetch()}>
                  {run.isFetching ? "Reading" : "Try again"}
                </Button>
                <Button asChild variant="secondary">
                  <Link to="/fleet">Back to the fleet</Link>
                </Button>
              </span>
            }
          />
        )}
      </>
    );
  }
  const data = run.data;
  const open = isOpen(data.status);
  const running = data.status === "running";
  const standing = STANDING[data.status] ?? {
    word: data.status,
    tone: "neutral" as const,
  };
  const now = Math.floor(Date.now() / 1000);
  const found = cardOf(snapshot.data, data.card_id);

  async function send() {
    const text = message.trim();
    if (text === "" || sending) return;
    setSending(true);
    setError(null);
    setSent((n) => n + 1);
    try {
      await steer(id, text);
      setMessage("");
    } catch (e) {
      setError(e instanceof Error ? e.message : "cannot steer the run");
    }
    setSending(false);
  }

  const tabs: Array<{ value: Tab; label: string }> = [
    { value: "conversation", label: "Conversation" },
    ...(open ? [{ value: "terminal" as const, label: "Terminal" }] : []),
    { value: "changes", label: "Changes" },
  ];
  const shown: Tab = tab === "terminal" && !open ? "conversation" : tab;

  const tabsRow = (
    <Tabs
      idBase={views}
      tabs={tabs}
      value={shown}
      label="Views of the run"
      onChange={(next) => {
        setTab(next);
        if (next === "terminal") setTerminalSeen(true);
      }}
    />
  );
  const span = runSpan(data.started_at, data.ended_at, now);
  const marks = stepMarks(thread.entries, span);

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="run-page">
      <PageHeader title={`Run ${id}`} tabs={tabsRow} />
      <div className="mb-3 flex flex-wrap items-center gap-3" data-testid="run-info">
        <StateLabel tone={standing.tone}>{standing.word}</StateLabel>
        <span className="text-base text-text">
          <span className="font-semibold">{data.agent}</span>
          <span className="text-secondary"> · {data.purpose}</span>
        </span>
        {found ? (
          <Link
            to={repositoryBoardPath(found.repository, found.card.id)}
            className="min-w-0 break-words text-base text-text underline underline-offset-4 hover:text-secondary"
          >
            #{found.card.number} {found.card.title} · {found.repository.name}
          </Link>
        ) : (
          data.card_id !== null && (
            <span className="font-mono text-sm text-secondary">card {data.card_id}</span>
          )
        )}
        {data.purpose === "chat" && (
          <Link
            to={`/chat/${id}`}
            className="text-base text-text underline underline-offset-4 hover:text-secondary"
          >
            Open as chat
          </Link>
        )}
      </div>
      <TimeBar
        className="mb-4"
        start={span.startDay}
        end={span.endDay}
        axis={{ start: span.startDay, end: span.endDay }}
        progress={100}
        label={`${standing.word}, ${elapsed(span.seconds)}, ${plural(marks.length, "step", "steps")}`}
        emphasis={running}
        marks={marks}
      />

      {shown === "conversation" && (
        <TabPanel idBase={views} value="conversation" className="flex min-h-0 flex-1 flex-col">
          {/* The log keeps to its own box: a long line scrolls inside it instead of widening the page. */}
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden" data-testid="conversation">
            {thread.error ? (
              <p
                role="alert"
                className={`m-0 text-center text-base text-error-text ${thread.loaded ? "pb-3" : "py-8"}`}
              >
                {thread.error}
              </p>
            ) : null}
            {thread.error && !thread.loaded ? null : (
              <div className="min-h-0 flex-1 overflow-x-auto">
                <ThreadView
                  entries={thread.entries}
                  working={isWorking(data.status)}
                  sent={sent}
                  loading={!thread.loaded}
                  loadingMessage="Reading the run"
                  hasOlder={thread.hasOlder}
                  olderLoading={thread.olderLoading}
                  olderError={thread.olderError}
                  loadOlder={thread.loadOlder}
                  empty="Nothing has happened in this run yet."
                />
              </div>
            )}
          </div>
          {running ? (
            <div className="mt-3 shrink-0">
              <Composer
                value={message}
                onChange={setMessage}
                onSend={() => void send()}
                canSend={message.trim() !== "" && !sending}
                placeholder="Steer the run"
                error={error}
              />
            </div>
          ) : (
            // A run that has ended says so in its own tag in the title row, where every run's state is said; a run that
            // is merely between turns is the one where the field's absence needs explaining.
            open && (
              <div className="mt-3 shrink-0">
                <p className="m-0 text-sm text-secondary" data-testid="run-hint">
                  The run is not working now, so it cannot be steered.
                </p>
              </div>
            )
          )}
        </TabPanel>
      )}
      {terminalSeen && open && (
        // The shell stays open behind the other views, so a glance at the conversation does not lose it.
        <TabPanel
          idBase={views}
          value="terminal"
          className={cn("min-h-0 flex-1 flex-col", shown === "terminal" ? "flex" : "hidden")}
        >
          <Terminal path={`/v1/runs/${id}/terminal`} autoFocus className="min-h-0 flex-1" />
        </TabPanel>
      )}
      {shown === "changes" && (
        <TabPanel
          idBase={views}
          value="changes"
          className="flex min-h-0 flex-1 flex-col"
          testId="run-changes"
        >
          <Changes of={{ run: id }} />
        </TabPanel>
      )}
    </div>
  );
}
