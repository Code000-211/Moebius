/**
 * The Fleet page, with the tabs Active and Finished in its header (`?tab=finished` in the address, so a reload keeps
 * it). Active is the work that is going on, as one virtual list of 36px rows; Finished is the runs that ended, newest
 * first under the day they ended on, with the outcome, the card's id, the tokens and how long they went on. The runs that wait for the person come
 * first, under "Needs you", then the others under "Running". A row has the status icon, the issue's id, the title, one
 * pale line (the kind of run and the step it is at; "Plan ready" for a run that waits) and how long it has gone on. A run
 * that waits for a plan carries a quiet "Approve plan". Stop and the tokens are on the hovered or the selected row only,
 * and are not in the page otherwise. The rows that are drawn read their run's events and follow the stream's
 * `run_event`s, so the step moves on its own; a row opens the card in its window, or the chat. The arrows move the
 * selection, Shift chooses a range, and a bar at the foot, or a right click, stops every chosen run at once; Escape lets
 * go. A read that fails says what could not be read and offers to try again, rather than saying that nothing is working.
 */

import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  type PointerEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  useCallback,
  useId,
  useMemo,
  useState,
} from "react";
import { Link, useNavigate } from "react-router";
import { approve, snapshot as readSnapshot } from "../../api/calls/board";
import { chats as readChats, stopRun } from "../../api/calls/chat";
import { finishedRuns as readFinished, run as readRun } from "../../api/calls/fleet";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { FinishedRun } from "../../api/types/runs";
import { inputPath, inputSummary, toolTitle } from "../../chat/conversation";
import { useThread } from "../../chat/useThread";
import { thinkingLabel } from "../../ui/chat/ChatCollapsedThinking";
import { Button } from "../../ui/components/Button";
import { ContextMenu, useContextMenu } from "../../ui/components/ContextMenu";
import { FilterBar } from "../../ui/filters/FilterBar";
import { readFilters } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { AgentIcon, ErrorIcon, PauseIcon, PlayIcon, ProhibitIcon } from "../../ui/icons";
import { elapsed as age, today } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { useSelection } from "../../ui/lib/useSelection";
import { Empty } from "../../ui/page/Empty";
import { ListRow, type RowPosition } from "../../ui/page/ListRow";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SelectionBar, type SelectionCommand } from "../../ui/page/SelectionBar";
import { StatusIcon } from "../../ui/page/StatusIcon";
import { TabPanel, Tabs } from "../../ui/page/Tabs";
import { useToast } from "../../ui/page/Toast";
import { VirtualList } from "../../ui/page/VirtualList";
import { useOpenCard } from "../board/CardWindowHost";
import {
  afterSteps,
  choiceKey,
  FILTER_KEYS,
  FLEET_TABS,
  filtered,
  filterFields,
  finishedId,
  finishedItems,
  finishedKey,
  items,
  lasted,
  latestSteps,
  needsYou,
  tabOf,
  thinkingNow,
  thoughtSeconds,
  tokensText,
  type Work,
  waitingWord,
  working,
  workStartedAt,
} from "./model";
import { usePause } from "./usePause";

export function FleetPage() {
  const [params, setQuery] = useQueryParams();
  const tab = tabOf(params.get("tab"));
  const pause = usePause();
  const base = useId();
  return (
    <>
      <PageHeader
        title="Fleet"
        tabs={
          <Tabs
            tabs={[...FLEET_TABS]}
            value={tab}
            onChange={(next) => setQuery({ tab: next === "active" ? "" : next })}
            label="Fleet"
            idBase={base}
          />
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
      {/* The list scrolls in the room that is left under the header; the panel itself does not. */}
      <TabPanel
        idBase={base}
        value={tab}
        className="flex h-[calc(100%-3rem)] min-h-40 flex-col outline-none"
      >
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
        {tab === "finished" ? <FinishedRuns /> : <ActiveRuns />}
      </TabPanel>
    </>
  );
}

/** The runs that are going on: the filters, and the list or why it is empty. */
function ActiveRuns() {
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const chats = useQuery({ queryKey: keys.chats, queryFn: readChats });
  // How long a run has been going is said in seconds, so this page's clock ticks faster than the rest.
  const now = useNow(5_000);
  const [params, setQuery] = useQueryParams();
  if (data.isPending || chats.isPending) {
    return (
      <VirtualList
        label="Runs"
        items={[]}
        renderRow={() => null}
        loading
        loadingMessage="Reading the fleet"
      />
    );
  }
  // A page that could not be read says so. Falling through to the empty state would claim that no agent is working,
  // which is the opposite of what happened, so the failure comes first and carries the way back.
  if (data.isError || chats.isError) {
    return (
      <ReadFailed
        error={data.isError ? data.error : chats.error}
        fallback="Could not read what the agents are doing. Try again."
        busy={data.isFetching || chats.isFetching}
        onRetry={() => {
          void data.refetch();
          void chats.refetch();
        }}
      />
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
  return (
    <>
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
          icon={<AgentIcon size={24} />}
          action={
            <Button asChild variant="secondary">
              <Link to="/chat">Start a chat</Link>
            </Button>
          }
        />
      ) : (
        <RunList list={list} now={now} />
      )}
    </>
  );
}

/**
 * The runs that have ended, the newest first under the day they ended on. A row is the outcome, the card's id and
 * title, the pale name of the run's kind, the tokens it used and how long it went on; it opens the card in its window.
 */
function FinishedRuns() {
  const finished = useQuery({ queryKey: keys.finishedRuns, queryFn: () => readFinished() });
  const openCard = useOpenCard();
  const [selected, setSelected] = useState<string | null>(null);
  const rows = useMemo(() => finishedItems(finished.data ?? [], today()), [finished.data]);
  const byKey = useMemo(
    () => new Map((finished.data ?? []).map((one) => [finishedKey(one), one])),
    [finished.data],
  );
  if (finished.isPending) {
    return (
      <VirtualList
        label="Finished runs"
        items={[]}
        renderRow={() => null}
        loading
        loadingMessage="Reading the finished runs"
      />
    );
  }
  if (finished.isError) {
    return (
      <ReadFailed
        error={finished.error}
        fallback="Could not read the finished runs. Try again."
        busy={finished.isFetching}
        onRetry={() => void finished.refetch()}
      />
    );
  }
  if (rows.length === 0) {
    return <Empty title="No run has finished" icon={<AgentIcon size={24} />} />;
  }
  return (
    <VirtualList<FinishedRun>
      label="Finished runs"
      items={rows}
      selectedKey={selected}
      onSelect={setSelected}
      onOpen={(key) => {
        const one = byKey.get(key);
        if (one) openCard(one.card.id);
      }}
      renderRow={(one, state) => (
        <FinishedRow
          one={one}
          at={state.at}
          selected={state.selected}
          onClick={(event) => {
            state.onClick(event);
            openCard(one.card.id);
          }}
        />
      )}
    />
  );
}

const OUTCOMES: Record<string, { name: string; mark: ReactNode }> = {
  succeeded: { name: "Succeeded", mark: <StatusIcon state="done" /> },
  failed: {
    name: "Failed",
    mark: <ErrorIcon size={16} className="shrink-0 text-danger-text" aria-label="Failed" />,
  },
  stopped: {
    name: "Stopped",
    mark: <ProhibitIcon size={16} className="shrink-0 text-muted" aria-label="Stopped" />,
  },
};

function FinishedRow({
  one,
  at,
  selected,
  onClick,
}: {
  one: FinishedRun;
  at: RowPosition;
  selected: boolean;
  onClick: (event: ReactMouseEvent<HTMLElement>) => void;
}) {
  const outcome = OUTCOMES[one.run.status];
  return (
    <ListRow
      mark={
        <span data-testid={`outcome-${one.run.status}`} className="flex shrink-0">
          {outcome?.mark ?? <StatusIcon state="notStarted" />}
        </span>
      }
      id={finishedId(one)}
      title={one.card.title}
      reason={capital(one.run.purpose)}
      action={
        one.tokens > 0 && (
          <span className="text-xs tabular-nums text-muted">{tokensText(one.tokens)}</span>
        )
      }
      age={age(lasted(one))}
      selected={selected}
      at={at}
      onClick={onClick}
      data-key={finishedKey(one)}
    />
  );
}

/** The runs as one virtual list, with the choice of them and what is done with the chosen. */
function RunList({ list, now }: { list: Work[]; now: number }) {
  const queryClient = useQueryClient();
  const toast = useToast();
  const openWork = useOpenWork();
  const menu = useContextMenu();
  const [selected, setSelected] = useState<string | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const rows = useMemo(() => items(list), [list]);
  const order = useMemo(() => list.map(choiceKey), [list]);
  const byKey = useMemo(() => new Map(list.map((work) => [choiceKey(work), work])), [list]);
  const choice = useSelection(order);
  const chosen = list.filter((work) => choice.has(choiceKey(work)));
  const { clear, ensure, toggle } = choice;
  const { open: openMenu } = menu;
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
        toast({
          kind: "confirmation",
          text: `Stopping ${targets.length} run${targets.length === 1 ? "" : "s"}.`,
        });
        clear();
        return;
      }
      const reason = isApiError(failed[0]?.error)
        ? failed[0].error.message
        : "something went wrong";
      toast({
        kind: "error",
        text: `Could not stop ${failed.map((one) => one.work.title).join(", ")}: ${reason}`,
      });
    },
    [queryClient, clear, toast],
  );
  const stop = useCallback((work: Work) => void stopRuns([work]), [stopRuns]);
  const commands: SelectionCommand[] =
    chosen.length === 0
      ? []
      : [{ id: "stop", label: "Stop runs", key: "x", run: () => void stopRuns(chosen) }];

  // The pointer and the right click are read once, at the list, from the row they are on.
  const rowOf = (target: EventTarget | null) =>
    (target as HTMLElement | null)?.closest<HTMLElement>("[data-key]")?.dataset.key ?? null;
  const rightClick = (event: ReactMouseEvent) => {
    const key = rowOf(event.target);
    if (key === null) return;
    setSelected(key);
    ensure(key);
    openMenu(event);
  };
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: the right click is a second way to the menu the bar has
    <div
      className="flex min-h-0 flex-1 flex-col"
      onPointerOver={(event: PointerEvent) => setHovered(rowOf(event.target))}
      onPointerLeave={() => setHovered(null)}
      onContextMenu={rightClick}
    >
      <VirtualList<Work>
        label="Runs"
        items={rows}
        selectedKey={selected}
        onSelect={(key) => {
          clear();
          setSelected(key);
        }}
        onSelectRange={(range) => {
          clear();
          for (const key of range) toggle(key);
        }}
        onOpen={(key) => {
          const work = byKey.get(key);
          if (work) openWork(work);
        }}
        renderRow={(work, state) => (
          <Row
            work={work}
            now={now}
            at={state.at}
            selected={state.selected || choice.has(state.key)}
            tools={state.selected || hovered === state.key}
            onClick={(event) => {
              state.onClick(event);
              if (!event.shiftKey) openWork(work);
            }}
            onStop={stop}
          />
        )}
      />
      <SelectionBar noun="run" count={chosen.length} commands={commands} onClear={clear} />
      <ContextMenu
        point={menu.point}
        title={`${chosen.length} run${chosen.length === 1 ? "" : "s"}`}
        commands={commands}
        onClose={menu.close}
      />
    </div>
  );
}

/** Opens what a run belongs to: its card in the window, or its chat. */
function useOpenWork() {
  const navigate = useNavigate();
  const openCard = useOpenCard();
  return useCallback(
    (work: Work) =>
      work.open.kind === "card" ? openCard(work.open.card) : navigate(`/chat/${work.open.chat}`),
    [navigate, openCard],
  );
}

const capital = (word: string) => word.charAt(0).toUpperCase() + word.slice(1);

function Row({
  work,
  now,
  at,
  selected,
  tools,
  onClick,
  onStop,
}: {
  work: Work;
  now: number;
  at: RowPosition;
  selected: boolean;
  /** Whether the pointer is on the row or it is the selected one: Stop and the tokens are drawn only then. */
  tools: boolean;
  onClick: (event: ReactMouseEvent<HTMLElement>) => void;
  onStop: (work: Work) => void;
}) {
  const queryClient = useQueryClient();
  const toast = useToast();
  // Only a row that is drawn follows its run, so the stream carries as many runs as the window shows.
  const thread = useThread(work.run);
  // A card's run says when it started; a chat's start is in the list of chats.
  const run = useQuery({
    queryKey: ["run", work.run],
    queryFn: () => readRun(work.run),
    enabled: work.startedAt === 0,
  });
  const startedAt = work.startedAt || run.data?.started_at || 0;
  const recent = latestSteps(thread.entries);
  const step = recent.at(-1);
  const ended = afterSteps(thread.entries);
  const began = workStartedAt(thread.entries, thread.hasOlder, startedAt);
  const lastAt = thread.entries.at(-1)?.at ?? now;
  const stops = work.status === "waiting" ? lastAt : now;
  const waits = needsYou(work);
  const doing =
    step === undefined
      ? ""
      : step.kind === "tool"
        ? `${toolTitle(step.call)} ${inputPath(step.call.input) ?? inputSummary(step.call.input)}`.trim()
        : thinkingLabel(thoughtSeconds(recent, recent.length - 1, ended), thinkingNow(work.status));
  const reason = waits ? (
    <span className="text-attention-text">{waitingWord(work)}</span>
  ) : (
    [work.open.kind === "chat" ? "" : capital(work.kind), doing]
      .filter((part) => part !== "")
      .join(" · ")
  );
  const approving = async (event: ReactMouseEvent) => {
    event.stopPropagation();
    if (work.approve === null) return;
    try {
      await approve(work.approve);
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
    } catch (error) {
      toast({
        kind: "error",
        text: isApiError(error) ? error.message : "Could not approve the plan.",
      });
    }
  };
  return (
    <ListRow
      status={waits ? "needsYou" : "running"}
      id={work.id}
      title={work.title}
      reason={reason}
      action={
        <>
          {tools && (
            <>
              {work.tokens !== null && (
                <span className="text-xs tabular-nums text-muted">{tokensText(work.tokens)}</span>
              )}
              <Button
                variant="ghost"
                size="sm"
                aria-label={`Stop ${work.title}`}
                onClick={(event) => {
                  event.stopPropagation();
                  onStop(work);
                }}
                className="min-h-6 px-1.5 text-xs"
              >
                Stop
              </Button>
            </>
          )}
          {work.approve !== null && (
            <Button
              variant="secondary"
              size="sm"
              onClick={approving}
              className="min-h-6 px-2 text-xs text-accent-text"
            >
              Approve plan
            </Button>
          )}
        </>
      }
      age={startedAt > 0 ? age(now - startedAt) : began === null ? "" : age(stops - began)}
      selected={selected}
      at={at}
      onClick={onClick}
      data-key={choiceKey(work)}
    />
  );
}
