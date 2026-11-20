/**
 * The Fleet page: which runners are up and what they are running, as a compact list grouped by agent. One row for each
 * card whose latest run is starting, running or waiting and for each chat that is, with the status word and its mark,
 * what the run is for, its current step and how long it has run. The rows read their events when the page opens and
 * follow the stream's `run_event`s while it is open, so the step moves on its own; a row opens the card's board or the
 * chat. A read that fails says what could not be read and offers to try again, rather than saying that nothing is
 * working. A box at the left of a row chooses it (shift-click chooses a range), and a bar at the foot, or a right click,
 * stops every chosen run in one go; Escape lets go of the choice. A row has its own Stop in its right column, shown
 * under the pointer and on focus, and the header pauses and resumes all agents, saying so while they are paused.
 */

import { PauseIcon, PlayIcon, StopIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type MouseEvent, memo, useCallback, useState } from "react";
import { Link, useNavigate } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { chats as readChats, stopRun } from "../../api/calls/chat";
import { run as readRun } from "../../api/calls/fleet";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { inputPath, inputSummary, toolTitle } from "../../chat/conversation";
import { useThread } from "../../chat/useThread";
import { thinkingLabel } from "../../ui/chat/ChatCollapsedThinking";
import { ToolStatusDot } from "../../ui/chat/ToolStatusDot";
import { toolStatus } from "../../ui/chat/Turn";
import { Button } from "../../ui/components/Button";
import { ContextMenu, useContextMenu } from "../../ui/components/ContextMenu";
import { FilterBar } from "../../ui/filters/FilterBar";
import { readFilters } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { cn } from "../../ui/lib/cn";
import { useNow } from "../../ui/lib/useNow";
import { useSelection } from "../../ui/lib/useSelection";
import { Empty } from "../../ui/page/Empty";
import { FadedLine } from "../../ui/page/FadedLine";
import { MarkLabel } from "../../ui/page/MarkLabel";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SelectBox } from "../../ui/page/SelectBox";
import { SelectionBar, type SelectionCommand } from "../../ui/page/SelectionBar";
import { SkeletonList } from "../../ui/page/Skeleton";
import { useOpenCard } from "../board/CardWindowHost";
import {
  afterSteps,
  byAgent,
  context,
  elapsed,
  FILTER_KEYS,
  filtered,
  filterFields,
  latestSteps,
  statusWord,
  thinkingNow,
  thoughtSeconds,
  tokensText,
  type Work,
  working,
  workStartedAt,
} from "./model";
import { usePause } from "./usePause";

/** What a row is chosen by: a card's run and a chat can share a number. */
const choiceKey = (work: Work) => `${work.open.kind}:${work.run}`;

export function FleetPage() {
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const chats = useQuery({ queryKey: keys.chats, queryFn: readChats });
  // How long a run has been going is said in seconds, so this page's clock ticks faster than the rest.
  const now = useNow(5_000);
  const [params, setQuery] = useQueryParams();
  const pause = usePause();
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
  const everything = data.data ? working(data.data, chats.data ?? []) : [];
  const fields = filterFields(
    everything,
    readFilters(
      params,
      FILTER_KEYS.map((key) => ({ key, label: key, options: [] })),
    ),
  );
  const values = readFilters(params, fields);
  const list = filtered(everything, values);
  const set = Object.values(values).some((value) => value !== "");
  return (
    <>
      <PageHeader
        title="Fleet"
        context={
          set && everything.length > 0
            ? `${context(list)} · ${everything.length} in all`
            : context(list)
        }
        actions={
          <Button
            variant="secondary"
            onClick={pause.toggle}
            disabled={pause.busy}
            data-testid="pause-all"
          >
            {pause.paused ? (
              <PlayIcon className="size-4" aria-hidden="true" />
            ) : (
              <PauseIcon className="size-4" aria-hidden="true" />
            )}
            {pause.paused ? "Resume all agents" : "Pause all agents"}
          </Button>
        }
      />
      {pause.paused && (
        <Notice tone="ok" className="mb-4">
          {`All agents are paused by ${pause.paused.by.replace(/^person:/, "@")}. Runs are held and none starts until they are resumed.`}
        </Notice>
      )}
      {pause.error && (
        <Notice tone="error" className="mb-4">
          {pause.error}
        </Notice>
      )}
      <FilterBar
        label="Filter the runs"
        fields={fields}
        values={values}
        onFilter={(key, value) => setQuery({ [key]: value })}
        onClear={() => setQuery(Object.fromEntries(FILTER_KEYS.map((key) => [key, ""])))}
      />
      {list.length === 0 && everything.length > 0 ? (
        <Empty
          title="No run matches"
          body="No agent at work passes these filters. Clear them to see every run."
          action={
            <Button
              variant="secondary"
              onClick={() => setQuery(Object.fromEntries(FILTER_KEYS.map((key) => [key, ""])))}
            >
              Clear filters
            </Button>
          }
        />
      ) : list.length === 0 ? (
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
        <RunGroups list={list} now={now} />
      )}
    </>
  );
}

/** The runs grouped by agent, with the choice of them and what is done with the chosen. */
function RunGroups({ list, now }: { list: Work[]; now: number }) {
  const queryClient = useQueryClient();
  const menu = useContextMenu();
  const [notice, setNotice] = useState<{ tone: "ok" | "error"; text: string } | null>(null);
  const groups = byAgent(list);
  // The order the rows are drawn in, which a range chosen with Shift follows.
  const choice = useSelection(groups.flatMap(([, runs]) => runs.map(choiceKey)));
  const chosen = list.filter((work) => choice.has(choiceKey(work)));
  // What a row is given keeps its identity, so that a row draws again only when its own run changes.
  const { toggle, ensure } = choice;
  const { open: openMenu } = menu;
  const choose = useCallback(
    (work: Work, range: boolean) => toggle(choiceKey(work), range),
    [toggle],
  );
  const rightClick = useCallback(
    (work: Work, event: MouseEvent) => {
      ensure(choiceKey(work));
      openMenu(event);
    },
    [ensure, openMenu],
  );
  const { clear } = choice;
  const stopRuns = useCallback(
    async (targets: Work[]) => {
      const results = await Promise.allSettled(targets.map((work) => stopRun(work.run)));
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.chats });
      const failed = results.flatMap((result, at) =>
        result.status === "rejected" && targets[at]
          ? [{ work: targets[at], error: result.reason }]
          : [],
      );
      if (failed.length === 0) {
        setNotice({
          tone: "ok",
          text: `Stopping ${targets.length} run${targets.length === 1 ? "" : "s"}.`,
        });
        clear();
        return;
      }
      const reason = isApiError(failed[0]?.error)
        ? failed[0].error.message
        : "something went wrong";
      setNotice({
        tone: "error",
        text: `Could not stop ${failed.map((one) => one.work.title).join(", ")}: ${reason}`,
      });
    },
    [queryClient, clear],
  );
  const stop = useCallback((work: Work) => void stopRuns([work]), [stopRuns]);
  const stopAll = () => stopRuns(chosen);
  const commands: SelectionCommand[] =
    chosen.length === 0
      ? []
      : [{ id: "stop", label: "Stop runs", key: "x", run: () => void stopAll() }];
  return (
    <>
      {notice && (
        <Notice
          tone={notice.tone}
          className="mb-4"
          action={
            <Button variant="ghost" size="sm" onClick={() => setNotice(null)}>
              Dismiss
            </Button>
          }
        >
          {notice.text}
        </Notice>
      )}
      <div className="flex flex-col gap-6">
        {byAgent(list).map(([agent, runs]) => (
          <section
            key={agent}
            aria-labelledby={`fleet-agent-${agent}`}
            className="flex flex-col gap-2"
          >
            <h2
              id={`fleet-agent-${agent}`}
              className="m-0 flex items-baseline gap-2 px-1 text-sm font-semibold text-text"
            >
              <span>{agent}</span>
              <span className="font-normal tabular-nums text-secondary">{runs.length}</span>
            </h2>
            <div className="overflow-hidden rounded-card bg-surface shadow-card">
              {/* biome-ignore lint/a11y/noRedundantRoles: list-style: none drops the list in WebKit */}
              <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
                {runs.map((work) => (
                  <li key={work.run}>
                    <Row
                      work={work}
                      now={now}
                      chosen={choice.has(choiceKey(work))}
                      anyChosen={chosen.length > 0}
                      onChoose={choose}
                      onStop={stop}
                      onContextMenu={rightClick}
                    />
                  </li>
                ))}
              </ul>
            </div>
          </section>
        ))}
      </div>
      <SelectionBar noun="run" count={chosen.length} commands={commands} onClear={choice.clear} />
      <ContextMenu
        point={menu.point}
        title={`${chosen.length} run${chosen.length === 1 ? "" : "s"}`}
        commands={commands}
        onClose={menu.close}
      />
    </>
  );
}

/** Whether two reads of a run say the same: the page makes new objects at every read of the snapshot. */
function sameRow(was: RowProps, now: RowProps): boolean {
  return (
    was.now === now.now &&
    was.chosen === now.chosen &&
    was.anyChosen === now.anyChosen &&
    was.onChoose === now.onChoose &&
    was.onStop === now.onStop &&
    was.onContextMenu === now.onContextMenu &&
    JSON.stringify(was.work) === JSON.stringify(now.work)
  );
}

type RowProps = {
  work: Work;
  now: number;
  chosen: boolean;
  anyChosen: boolean;
  onChoose: (work: Work, range: boolean) => void;
  onStop: (work: Work) => void;
  onContextMenu: (work: Work, event: MouseEvent) => void;
};

const Row = memo(function Row({
  work,
  now,
  chosen,
  anyChosen,
  onChoose,
  onStop,
  onContextMenu,
}: RowProps) {
  const navigate = useNavigate();
  const openCard = useOpenCard();
  const thread = useThread(work.run);
  // A card's run says when it started; a chat's start is in the list of chats.
  const run = useQuery({
    queryKey: ["run", work.run],
    queryFn: () => readRun(work.run),
    enabled: work.startedAt === 0,
  });
  const startedAt = work.startedAt || run.data?.started_at || 0;
  // A thought says how long it went on, which only the step after it can say, so it is measured against the lines of
  // the whole run; the row shows the last of them, the step the run is at.
  const recent = latestSteps(thread.entries);
  const step = recent.at(-1);
  const atWork = thinkingNow(work.status);
  // What came after the last step — the answer, usually — is what says the last of them stopped.
  const ended = afterSteps(thread.entries);
  const status = statusWord(work.status, work.asks);
  const open = () =>
    work.open.kind === "card" ? openCard(work.open.card) : navigate(`/chat/${work.open.chat}`);
  const about = [work.repository, work.tokens === null ? "" : tokensText(work.tokens)]
    .filter((part) => part !== "")
    .join(" · ");
  const began = workStartedAt(thread.entries, thread.hasOlder, startedAt);
  const lastAt = thread.entries.at(-1)?.at ?? now;
  const stops = work.status === "waiting" ? lastAt : now;
  const worked = began === null ? null : elapsed(stops - began);
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: the right click is a second way to the menu the bar has
    <div className="group relative" onContextMenu={(event) => onContextMenu(work, event)}>
      <SelectBox
        label={`Choose ${work.title}`}
        checked={chosen}
        anyChosen={anyChosen}
        onToggle={(range) => onChoose(work, range)}
        className="absolute top-1/2 left-3 z-10 -translate-y-1/2"
      />
      <button
        type="button"
        data-testid="fleet-row"
        onClick={open}
        className={cn(
          "flex min-h-12 w-full cursor-pointer flex-wrap items-center gap-x-4 gap-y-1 py-2 pr-20 pl-10 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover",
          chosen && "bg-selected",
        )}
      >
        <MarkLabel mark={status.mark} word={status.word} className="w-28 shrink-0" />
        <span className="flex min-w-0 flex-1 basis-60 flex-col">
          <span className="truncate text-base text-text">{work.title}</span>
          <span className="flex min-w-0 gap-1 text-sm text-secondary">
            <span className="shrink-0">{work.kind}</span>
            {about && <span className="min-w-0 truncate">{about}</span>}
          </span>
        </span>
        <span className="flex min-w-0 flex-1 basis-60 items-center gap-2 text-sm text-secondary">
          {step === undefined ? (
            <span>No step yet</span>
          ) : step.kind === "tool" ? (
            <>
              <ToolStatusDot status={toolStatus(step.call)} className="shrink-0" />
              <span className="shrink-0 font-mono text-text">{toolTitle(step.call)}</span>
              {/* One line, as in a conversation: a command is read by its end, so what a line that runs out hides is
                its beginning, and the whole of it stays in the tooltip. */}
              <FadedLine
                text={inputSummary(step.call.input)}
                title={inputPath(step.call.input) ?? inputSummary(step.call.input)}
                className="flex-1"
              />
            </>
          ) : (
            <>
              <span
                aria-hidden="true"
                className={cn("size-2 shrink-0 rounded-full", atWork ? "bg-progress" : "bg-track")}
              />
              <span>{thinkingLabel(thoughtSeconds(recent, recent.length - 1, ended), atWork)}</span>
            </>
          )}
        </span>
        <span className="w-16 shrink-0 text-right text-sm tabular-nums text-secondary">
          {startedAt > 0 ? elapsed(now - startedAt) : (worked ?? "")}
        </span>
      </button>
      <Button
        variant="ghost"
        size="sm"
        aria-label={`Stop ${work.title}`}
        onClick={() => onStop(work)}
        className="absolute top-1/2 right-2 z-10 -translate-y-1/2 opacity-0 transition-opacity duration-(--motion-fast) ease-out focus-visible:opacity-100 group-hover:opacity-100"
      >
        <StopIcon className="size-4" aria-hidden="true" />
        Stop
      </Button>
    </div>
  );
}, sameRow);
