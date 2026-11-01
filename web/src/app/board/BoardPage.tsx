/**
 * The board of a repository: its columns of the default workflow with their cards (dragged only where a person may
 * move them), the filters, the inspector dock and the form of a new issue. Everything comes from the snapshot, which
 * the event stream keeps current (`live/invalidation.ts`), so a card moves on this page when the server moves it. The
 * chosen card and the filters are in the address (`?card=`, `?column=`, `?agent=`, `?status=`), so a reload and a link
 * keep them.
 */
import { PlusIcon, XIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { snapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import {
  type DropResult,
  KanbanBoard,
  KanbanCard,
  KanbanCards,
  KanbanHeader,
  KanbanProvider,
} from "../../ui/components/KanbanBoard";
import { KanbanCardContent } from "../../ui/components/KanbanCardContent";
import { Loader } from "../../ui/components/Loader";
import { boardPath } from "../pages";
import { Header } from "../shell/Page";
import { Dock } from "./Dock";
import { type Decision, useDecisions } from "./decisions";
import { FilterBar } from "./FilterBar";
import {
  agentsOf,
  boardColumns,
  cardBadges,
  cardDetail,
  cardStatus,
  changeOf,
  dropRule,
  type Filters,
  groupByColumn,
  inboxOf,
  inspectedCard,
  isLive,
  issueUrl,
  nextCard,
  pullUrl,
} from "./model";
import { NewIssueDialog } from "./NewIssueDialog";

function plural(n: number, one: string, many: string): string {
  return n === 0 ? `no ${many}` : n === 1 ? `1 ${one}` : `${n} ${many}`;
}

export function BoardPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const { decide, pending, notice, setNotice } = useDecisions();
  const [creating, setCreating] = useState(false);
  const [rejectAsk, setRejectAsk] = useState(0);

  if (data.isPending) return <Loader message="Reading the board…" className="py-double" />;
  const board = data.data?.boards.find((b) => String(b.repository.id) === id);
  if (!data.data || !board) {
    return (
      <>
        <Header breadcrumb="Projects" title="Board" />
        <p className="text-sm text-normal">There is no such board. Pick one from the sidebar.</p>
      </>
    );
  }
  const inbox = data.data.inbox;
  const filters: Filters = {
    column: params.get("column") ?? "",
    agent: params.get("agent") ?? "",
    status: params.get("status") ?? "",
  };
  const setQuery = (changes: Record<string, string>) =>
    setParams(
      (current) => {
        const next = new URLSearchParams(current);
        for (const [key, value] of Object.entries(changes)) {
          if (value === "") next.delete(key);
          else next.set(key, value);
        }
        return next;
      },
      { replace: true },
    );
  const inspected = inspectedCard(board, inbox, params.get("card"));
  const groups = groupByColumn(board, inbox, filters);
  const waitingChanges = inbox.filter(
    (item) =>
      item.kind === "ready_to_merge" && board.cards.some((card) => card.id === item.card_id),
  ).length;
  const working = board.cards.filter(isLive).length;

  function onDragEnd(result: DropResult) {
    if (!result.destination) return;
    const card = board?.cards.find((c) => String(c.id) === result.draggableId);
    if (!card) return;
    const to = result.destination.droppableId;
    const drop = dropRule(card, to, inboxOf(inbox, card));
    const number = card.number;
    let decision: Decision | null = null;
    if (drop.kind === "approve") decision = { kind: "approve", card: card.id, number };
    if (drop.kind === "retry") decision = { kind: "retry", card: card.id, number };
    if (drop.kind === "merge") decision = { kind: "merge", change: drop.change, number };
    if (decision) {
      setQuery({ card: String(card.id) });
      decide(decision);
    } else if (drop.kind === "reject") {
      // Sending a change back needs a reason: the dock asks for it.
      setQuery({ card: String(card.id) });
      setRejectAsk((n) => n + 1);
    } else if (drop.kind === "refused") {
      setNotice({ tone: "error", text: `Cannot move #${number} to ${to}: ${drop.reason}` });
    }
  }

  return (
    <>
      <div className="flex items-start justify-between gap-base">
        <Header breadcrumb="Projects" title={board.repository.name} />
        <Button size="sm" onClick={() => setCreating(true)}>
          <PlusIcon className="size-icon-sm" weight="bold" />
          New issue
        </Button>
      </div>
      <p className="m-0 mb-[12px] font-mono text-[12.5px] text-normal">
        {plural(working, "run", "runs")} working ·{" "}
        {plural(waitingChanges, "change waits for you", "changes wait for you")} · default workflow
      </p>
      <FilterBar
        repositories={data.data.boards.map((b) => b.repository)}
        repository={board.repository.id}
        onRepository={(repository) => navigate(boardPath(repository))}
        columns={boardColumns(board)}
        agents={agentsOf(board)}
        filters={filters}
        onChange={(next) => setQuery({ ...next })}
      />
      {notice && (
        <div
          role={notice.tone === "error" ? "alert" : "status"}
          className={`mb-[12px] flex items-start justify-between gap-base rounded-lg border px-base py-base text-sm ${
            notice.tone === "error"
              ? "border-red-border bg-red-fill text-error"
              : "border-teal-border bg-teal-fill text-success"
          }`}
        >
          <span className="[overflow-wrap:anywhere]">{notice.text}</span>
          <button
            type="button"
            onClick={() => setNotice(null)}
            aria-label="Dismiss"
            className="shrink-0 rounded-sm p-half hover:bg-card"
          >
            <XIcon className="size-icon-xs" weight="bold" />
          </button>
        </div>
      )}
      <div className="flex min-w-0 flex-col gap-[12px] xl:flex-row xl:items-start">
        <div className="min-w-0 flex-1 overflow-x-auto pb-base">
          <KanbanProvider onDragEnd={onDragEnd}>
            {groups.map(({ column, cards, total }) => (
              <BoardColumn
                key={column.name}
                board={board}
                inbox={inbox}
                name={column.name}
                script={column.script}
                cards={cards}
                total={total}
                inspectedId={inspected?.id}
                onSelect={(card) => setQuery({ card: String(card.id) })}
              />
            ))}
          </KanbanProvider>
        </div>
        {inspected && (
          <div className="w-full shrink-0 xl:sticky xl:top-0 xl:w-[400px]">
            <Dock
              key={inspected.id}
              board={board}
              card={inspected}
              item={inboxOf(inbox, inspected)}
              busy={pending}
              rejectAsk={rejectAsk}
              onDecide={decide}
              onClose={() => setQuery({ card: "none" })}
            />
          </div>
        )}
      </div>
      <NewIssueDialog
        repository={board.repository.id}
        name={board.repository.name}
        open={creating}
        onOpenChange={setCreating}
        onCreated={(number) =>
          setNotice({ tone: "ok", text: `Created issue #${number}. It waits in the Backlog.` })
        }
      />
    </>
  );
}

function BoardColumn({
  board,
  inbox,
  name,
  script,
  cards,
  total,
  inspectedId,
  onSelect,
}: {
  board: Board;
  inbox: InboxItem[];
  name: string;
  script: string | null;
  cards: Card[];
  total: number;
  inspectedId: number | undefined;
  onSelect: (card: Card) => void;
}) {
  const live = cards.some(isLive);
  const waits = cards.some((card) => {
    const item = inboxOf(inbox, card);
    return item !== undefined && item.kind !== "stuck";
  });
  const next = name === "Ready" ? nextCard(cards) : undefined;
  return (
    <KanbanBoard
      name={name}
      className={waits ? "border-amber-border" : live ? "border-live-border" : undefined}
    >
      <KanbanHeader
        name={name}
        count={total}
        detail={script ?? undefined}
        color={waits ? "var(--amber)" : live ? "var(--live)" : "var(--text)"}
      />
      <KanbanCards id={name} className="min-h-12">
        {cards.map((card, index) => {
          const status = cardStatus(card, inboxOf(inbox, card), board);
          const change = changeOf(board, card);
          const coding = card.column === "In progress" || card.column === "In review";
          return (
            <KanbanCard
              key={card.id}
              id={String(card.id)}
              name={`#${card.number} ${card.title}`}
              testId={`card-${card.number}`}
              index={index}
              isSelected={inspectedId === card.id}
              isLive={status === "live"}
              onClick={() => onSelect(card)}
              onKeyDown={(e) => {
                if (e.key === "Enter") onSelect(card);
              }}
            >
              <KanbanCardContent
                displayId={`#${card.number}`}
                issueUrl={issueUrl(board.repository, card)}
                title={card.title}
                detail={cardDetail(card, board.round_limit)}
                badges={cardBadges(card, status, next === card.id)}
                pullRequest={
                  change?.pull_request != null
                    ? {
                        number: change.pull_request,
                        url: pullUrl(board.repository, change.pull_request),
                        status: change.state,
                      }
                    : null
                }
                rounds={coding ? { done: card.rounds, limit: board.round_limit } : null}
                isLoading={status === "live"}
                muted={card.column === "Done"}
              />
            </KanbanCard>
          );
        })}
      </KanbanCards>
    </KanbanBoard>
  );
}
