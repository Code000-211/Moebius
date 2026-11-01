/**
 * The page of one run (`/runs/:id`): who runs, where it stands and the card it works for, then three views of it. The
 * conversation is the run's events drawn as the chat draws them (`chat/useThread.ts` follows the event stream, the
 * entries are `ui/chat/`), with a box under it to steer a running run (`POST /v1/runs/{id}/steer`); the terminal is a
 * shell in the run's worktree while it has one; the changes are what the run changed so far, as a diff. The terminal
 * stays open while another view is shown, so it is not lost by a glance at the conversation.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { Link, useParams } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { run as readRun, steer } from "../../api/calls/runs";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Snapshot } from "../../api/types/board";
import { Composer } from "../../chat/Composer";
import { isOpen, isWorking, statusTag } from "../../chat/format";
import { ThreadView } from "../../chat/ThreadView";
import { useThread } from "../../chat/useThread";
import { Changes } from "../../diff/Changes";
import { useServerEvents } from "../../live/LiveProvider";
import { Terminal } from "../../terminal/Terminal";
import { Tag } from "../../ui/chat/Tag";
import { Loader } from "../../ui/components/Loader";
import { cn } from "../../ui/lib/cn";
import { Header } from "../shell/Page";

type Tab = "conversation" | "terminal" | "changes";

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
        <Header breadcrumb="Runs" title="Run" />
        <p className="text-sm text-normal">There is no such run.</p>
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

  if (run.isPending) return <Loader message="Reading the run…" className="py-double" />;
  if (run.isError || !run.data) {
    const missing = isApiError(run.error) && run.error.status === 404;
    return (
      <>
        <Header breadcrumb="Runs" title={`Run ${id}`} />
        <p role={missing ? undefined : "alert"} className="text-sm text-normal">
          {missing ? "There is no such run." : "Cannot read the run."}
        </p>
      </>
    );
  }
  const data = run.data;
  const open = isOpen(data.status);
  const running = data.status === "running";
  const tag = statusTag(data.status, false) ?? { label: "Waiting", tone: "plain" as const };
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

  const tabs: Array<{ tab: Tab; label: string }> = [
    { tab: "conversation", label: "Conversation" },
    ...(open ? [{ tab: "terminal" as const, label: "Terminal" }] : []),
    { tab: "changes", label: "Changes" },
  ];
  const shown: Tab = tab === "terminal" && !open ? "conversation" : tab;

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="run-page">
      <Header breadcrumb="Runs" title={`Run ${id}`} />
      <div className="mb-[12px] flex flex-wrap items-center gap-base" data-testid="run-info">
        <Tag tone={tag.tone}>{tag.label}</Tag>
        <Tag>
          {data.agent} · {data.purpose}
        </Tag>
        {found ? (
          <Link
            to={`/board/${found.repository.id}?card=${found.card.id}`}
            className="min-w-0 truncate text-sm text-accent hover:underline"
          >
            #{found.card.number} {found.card.title} · {found.repository.name}
          </Link>
        ) : (
          data.card_id !== null && (
            <span className="font-mono text-xs text-low">card {data.card_id}</span>
          )
        )}
        {data.purpose === "chat" && (
          <Link to={`/chat/${id}`} className="text-sm text-accent hover:underline">
            Open as chat
          </Link>
        )}
      </div>
      <div role="tablist" aria-label="Views of the run" className="mb-base flex gap-half">
        {tabs.map((each) => (
          <button
            key={each.tab}
            type="button"
            role="tab"
            aria-selected={shown === each.tab}
            onClick={() => {
              setTab(each.tab);
              if (each.tab === "terminal") setTerminalSeen(true);
            }}
            className={cn(
              "rounded-full border px-plusfifty py-half text-sm font-semibold transition-colors",
              shown === each.tab
                ? "border-line-strong bg-card text-high"
                : "border-transparent text-normal hover:bg-card",
            )}
          >
            {each.label}
          </button>
        ))}
      </div>

      {shown === "conversation" && (
        <>
          <div className="min-h-0 flex-1" data-testid="conversation">
            {thread.error ? (
              <p role="alert" className="py-double text-center text-sm text-error">
                {thread.error}
              </p>
            ) : (
              <ThreadView
                entries={thread.entries}
                working={isWorking(data.status)}
                sent={sent}
                empty={thread.loaded ? "Nothing has happened in this run yet." : "Reading the run…"}
              />
            )}
          </div>
          <div className="mt-base shrink-0">
            {running ? (
              <Composer
                value={message}
                onChange={setMessage}
                onSend={() => void send()}
                canSend={message.trim() !== "" && !sending}
                placeholder="Steer the run"
                error={error}
                running
              />
            ) : (
              <p className="m-0 font-mono text-xs text-low" data-testid="run-hint">
                {open
                  ? "The run is not working now, so it cannot be steered."
                  : "This run has ended."}
              </p>
            )}
          </div>
        </>
      )}
      {terminalSeen && open && (
        <div className={cn("min-h-0 flex-1 flex-col", shown === "terminal" ? "flex" : "hidden")}>
          <Terminal path={`/v1/runs/${id}/terminal`} autoFocus className="min-h-0 flex-1" />
        </div>
      )}
      {shown === "changes" && (
        <div className="min-h-0 flex-1 overflow-y-auto" data-testid="run-changes">
          <Changes of={{ run: id }} />
        </div>
      )}
    </div>
  );
}
