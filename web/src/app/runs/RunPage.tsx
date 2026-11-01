/**
 * The page of one run (`/runs/:id`): what the run is doing, then the log of what it has done. The head says who runs,
 * where it stands and which card it works for; the conversation is the run's events drawn as the chat draws them
 * (`chat/useThread.ts` follows the event stream, the entries are `ui/chat/`), in a box of its own that scrolls rather
 * than pushing the page wide, with a box under it to steer a running run (`POST /v1/runs/{id}/steer`); the terminal is
 * a shell in the run's worktree while it has one; the changes are what the run changed so far, as a diff. The terminal
 * stays open while another view is shown, so it is not lost by a glance at the conversation.
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
import { isOpen, isWorking } from "../../chat/format";
import { ThreadView } from "../../chat/ThreadView";
import { useThread } from "../../chat/useThread";
import { Changes } from "../../diff/Changes";
import { useServerEvents } from "../../live/LiveProvider";
import { Terminal } from "../../terminal/Terminal";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { Status, type Tone } from "../../ui/page/Status";
import { TabPanel, Tabs } from "../../ui/page/Tabs";

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

  if (run.isPending) return <Loader message="Reading the run" className="py-8" />;
  if (run.isError || !run.data) {
    const missing = isApiError(run.error) && run.error.status === 404;
    return (
      <>
        <PageHeader title={`Run ${id}`} />
        {missing ? (
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

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="run-page">
      <PageHeader title={`Run ${id}`} />
      <div className="mb-3 flex flex-wrap items-center gap-3" data-testid="run-info">
        <Status tone={standing.tone}>{standing.word}</Status>
        <span className="text-base text-text">
          <span className="font-semibold">{data.agent}</span>
          <span className="text-secondary"> · {data.purpose}</span>
        </span>
        {found ? (
          <Link
            to={`/board/${found.repository.id}?card=${found.card.id}`}
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
      <Tabs
        idBase={views}
        tabs={tabs}
        value={shown}
        label="Views of the run"
        className="mb-3"
        onChange={(next) => {
          setTab(next);
          if (next === "terminal") setTerminalSeen(true);
        }}
      />

      {shown === "conversation" && (
        <TabPanel idBase={views} value="conversation" className="flex min-h-0 flex-1 flex-col">
          {/* The log keeps to its own box: a long line scrolls inside it instead of widening the page. */}
          <div
            className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-card bg-surface p-4 shadow-card"
            data-testid="conversation"
          >
            {thread.error ? (
              <p role="alert" className="m-0 py-8 text-center text-base text-error-text">
                {thread.error}
              </p>
            ) : (
              <div className="min-h-0 flex-1 overflow-x-auto">
                <ThreadView
                  entries={thread.entries}
                  working={isWorking(data.status)}
                  sent={sent}
                  empty={
                    thread.loaded ? "Nothing has happened in this run yet." : "Reading the run"
                  }
                />
              </div>
            )}
          </div>
          <div className="mt-3 shrink-0">
            {running ? (
              <Composer
                value={message}
                onChange={setMessage}
                onSend={() => void send()}
                canSend={message.trim() !== "" && !sending}
                placeholder="Steer the run"
                error={error}
              />
            ) : (
              <p className="m-0 text-sm text-secondary" data-testid="run-hint">
                {open
                  ? "The run is not working now, so it cannot be steered."
                  : "This run has ended."}
              </p>
            )}
          </div>
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
          className="min-h-0 flex-1 overflow-y-auto"
          testId="run-changes"
        >
          <Changes of={{ run: id }} />
        </TabPanel>
      )}
    </div>
  );
}
