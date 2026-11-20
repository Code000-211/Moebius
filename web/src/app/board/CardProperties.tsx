/**
 * The properties of the card in the dock, in the page's right column (`ui/page/Properties.tsx`): every fact the dock
 * used to list beside the issue — where the card is, who wrote it, what it waits for, its run, checks and cost — and
 * what a person changes about the issue, each edited in place and saved alone (`ui/page/EditableProperty.tsx`): its
 * title, whether it is open, who it is given to (its assignees), its kind and the fields of that kind, its cycle, the milestones it is in, who claimed it, its parent, its blockers and its
 * labels. Each save is a request to the
 * server (`api/calls/board.ts`) that reads the board again once answered, and what the server refuses is said under
 * the property that asked. The dock keeps the issue's text, its change and what a person decides.
 */
import { ArrowUpRightIcon } from "@phosphor-icons/react";
import { useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import {
  addBlocker,
  changeAssignees,
  claim,
  closeIssue,
  editCard,
  release,
  removeBlocker,
  reopenIssue,
  setCardRepositories,
  setDue,
  setEstimate,
  setLabels,
  setParent,
  setPriority,
} from "../../api/calls/board";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem, Priority } from "../../api/types/board";
import { patchCard, putCards } from "../../live/cardCache";
import { EditableProperty, ListProperty } from "../../ui/page/EditableProperty";
import { MarkLabel, type StateMark } from "../../ui/page/MarkLabel";
import { PageProperties, Property } from "../../ui/page/Properties";
import { PriorityBars } from "../../ui/page/StatusIcon";
import { CycleProperty } from "../cycles/CycleProperty";
import { KindProperties } from "../kinds/KindProperties";
import { MilestonesProperty } from "../milestones/MilestonesProperty";
import { AssigneesProperty } from "../people/AssigneesProperty";
import { FollowProperty } from "../people/FollowProperty";
import { WikiPagesProperty } from "../wiki/WikiBacklinks";
import { cardKey } from "./cardKey";
import {
  type CardStatus,
  cardStatus,
  changesOfCard,
  descendants,
  heldLine,
  inRepository,
  isClosed,
  isPlanningLabel,
  mergeStopped,
  PRIORITY_WORDS,
  parentOf,
  STATUS_LABELS,
  stackWords,
  tokens,
  waitingLine,
} from "./model";
import { RepositoriesProperty } from "./RepositoriesProperty";

/** The mark before the word of a card's status: the ring of a run, the icon of what is blocked, nothing otherwise. */
export const STATUS_MARK: Record<CardStatus, StateMark> = {
  held: "blocked",
  live: "live",
  "needs-you": "none",
  stuck: "blocked",
  blocked: "none",
  waiting: "none",
  idle: "none",
};

/** The checks of a change in a word. */
export const CHECKS: Record<string, string> = {
  pending: "Running",
  success: "Green",
  failure: "Failed",
  none: "None",
};

/** The value of the claim that claims the issue for the person looking at it: the server names the caller. */
const ME = "person";

/** The number of an issue as typed, `12` or `#12`; anything else is refused before it is sent. */
export function issueNumber(typed: string): number {
  const number = Number(typed.trim().replace(/^#/, ""));
  if (!Number.isInteger(number) || number <= 0) {
    throw new Error(`"${typed.trim()}" is not the number of an issue, such as 12.`);
  }
  return number;
}

/** The link-like button that opens another issue of the board in the dock. */
function IssueLink({ card, onOpen }: { card: Card; onOpen: (card: Card) => void }) {
  return (
    <button
      type="button"
      onClick={() => onOpen(card)}
      className="min-h-7 cursor-pointer appearance-none rounded-control bg-transparent px-0 text-left text-sm text-text underline decoration-hairline-strong underline-offset-4 hover:decoration-current"
    >
      {`${cardKey(card)} ${card.title}`}
    </button>
  );
}

/** What a stopped merge of a card's changes left: the repositories merged, the ones still open and why it stopped. */
function MergeStopped({ item }: { item: InboxItem }) {
  const { repository, reason, merged, open } = mergeStopped(item);
  return (
    <>
      <Property name="Merged">
        {merged.length === 0 ? (
          <span className="text-secondary">Nothing yet</span>
        ) : (
          merged.join(", ")
        )}
      </Property>
      <Property name="Still open">
        <span className="flex flex-col gap-0.5 py-0.5">
          <span>{open.join(", ")}</span>
          {repository !== "" && (
            <span className="text-sm text-secondary">
              {`The merge in ${repository} stopped${reason !== "" ? `: ${reason}` : ""}.`}
            </span>
          )}
        </span>
      </Property>
    </>
  );
}

export function CardProperties({
  board,
  card,
  item,
  stopped,
  boards,
  onOpenCard,
}: {
  board: Board;
  card: Card;
  item: InboxItem | undefined;
  /** The card's open `merge_stopped` item, which says what is merged and what is still open. */
  stopped?: InboxItem;
  /** Every board, which names the repositories a card may be given; the card's own when not given. */
  boards?: Board[];
  /** Shows another issue, such as the parent or a sub-issue, in the dock. */
  onOpenCard: (card: Card) => void;
}) {
  const queryClient = useQueryClient();
  // A save that is answered shows the answer at once (the event it makes reads the board again); one that is refused
  // reads the board, so what the property shows is what the server has.
  const saved = async (request: Promise<unknown>) => {
    try {
      putCards(queryClient, [await request]);
    } catch (error) {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.cards });
      throw error;
    }
  };
  const status = cardStatus(card, item, board);
  const parent = parentOf(board, card);
  const below = descendants(board, card);
  const closed = isClosed(card);
  const claimer = card.claimed_by;
  const people: Array<[string, string]> = [[ME, "Me"]];
  if (claimer !== null && claimer !== ME) people.unshift([claimer, claimer]);

  return (
    <PageProperties label={`Properties of #${card.number}`}>
      <EditableProperty
        name="Title"
        editor={{
          kind: "text",
          value: card.title,
          onSave: (title) => saved(editCard(card.id, title, card.body)),
        }}
      >
        {card.title}
      </EditableProperty>
      <Property name="Status">
        <MarkLabel mark={STATUS_MARK[status]} word={STATUS_LABELS[status]} />
      </Property>
      <EditableProperty
        name="State"
        editor={{
          kind: "choice",
          options: [
            ["open", "Open"],
            ["closed", "Closed"],
          ],
          value: closed ? "closed" : "open",
          onSave: (next) => saved(next === "closed" ? closeIssue(card.id) : reopenIssue(card.id)),
        }}
      >
        {closed ? "Closed" : "Open"}
      </EditableProperty>
      <Property name="Column">{card.column}</Property>
      <Property name="Repository">{board.repository.name}</Property>
      <RepositoriesProperty
        card={card}
        own={board.repository.name}
        boards={boards ?? [board]}
        onSave={(next) => saved(setCardRepositories(card.id, next))}
      />
      <Property name="Author">{card.author}</Property>
      <KindProperties card={card} saved={saved} />
      <AssigneesProperty
        assignees={card.assignees}
        onSave={(next) => saved(changeAssignees(card.id, card.assignees, next))}
      />
      <CycleProperty
        card={card.id}
        project={card.project_id}
        onSaved={() => {
          void queryClient.invalidateQueries({ queryKey: keys.cycles });
        }}
      />
      <MilestonesProperty
        card={card.id}
        onSaved={() => {
          void queryClient.invalidateQueries({ queryKey: keys.milestones });
        }}
      />
      <FollowProperty card={card.id} />
      {card.project_key !== "" && <WikiPagesProperty cardName={cardKey(card)} />}
      <EditableProperty
        name="Priority"
        editor={{
          kind: "choice",
          options: PRIORITY_WORDS,
          value: card.priority,
          onSave: (next) => saved(setPriority(card.id, next as Priority)),
        }}
      >
        <span className="inline-flex items-center gap-2">
          <PriorityBars priority={card.priority} decorative />
          {PRIORITY_WORDS.find(([name]) => name === card.priority)?.[1] ?? card.priority}
        </span>
      </EditableProperty>
      <EditableProperty
        name="Due date"
        editor={{
          kind: "date",
          value: card.due,
          onSave: (next) => saved(setDue(card.id, next)),
        }}
      >
        {card.due ?? <span className="text-secondary">No date</span>}
      </EditableProperty>
      <EditableProperty
        name="Estimate"
        editor={{
          kind: "number",
          value: card.estimate,
          invalid: (points) =>
            Number.isInteger(points) && points >= 1 && points <= 1000
              ? null
              : "Use a whole number from 1 to 1000.",
          onSave: (next) => saved(setEstimate(card.id, next)),
        }}
      >
        {card.estimate === null ? (
          <span className="text-secondary">No estimate</span>
        ) : (
          `${card.estimate} ${card.estimate === 1 ? "point" : "points"}`
        )}
      </EditableProperty>
      <EditableProperty
        name="Claim"
        editor={{
          kind: "person",
          people,
          none: "No one",
          value: claimer,
          onSave: (next) => saved(next === null ? release(card.id) : claim(card.id)),
        }}
      >
        {claimer ?? <span className="text-secondary">No one</span>}
      </EditableProperty>
      <EditableProperty
        name="Parent"
        editor={{
          kind: "number",
          value: card.parent,
          onSave: (next) => saved(setParent(card.id, next)),
        }}
        after={
          parent && (
            <button
              type="button"
              aria-label={`Open #${parent.number} ${parent.title}`}
              title={`Open #${parent.number}`}
              onClick={() => onOpenCard(parent)}
              className="inline-flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-control bg-transparent text-secondary hover:bg-hover hover:text-text"
            >
              <ArrowUpRightIcon className="size-3.5" aria-hidden="true" weight="bold" />
            </button>
          )
        }
      >
        {parent ? (
          `#${parent.number} ${parent.title}`
        ) : card.parent !== null ? (
          `#${card.parent}`
        ) : (
          <span className="text-secondary">None</span>
        )}
      </EditableProperty>
      {below.length > 0 && (
        <Property name="Sub-issues">
          <span className="flex flex-col gap-0.5 py-1">
            <span className="text-sm text-secondary">
              {stackWords({
                shown: below,
                total: below.length,
                done: below.filter(isClosed).length,
              })}
            </span>
            {below.map((child) => (
              <span key={child.id} className="flex flex-wrap items-baseline gap-x-2">
                <IssueLink card={child} onOpen={onOpenCard} />
                <span className="text-sm text-secondary">
                  {isClosed(child) ? "Closed" : child.column}
                </span>
              </span>
            ))}
          </span>
        </Property>
      )}
      <ListProperty
        name="Waiting on"
        items={card.blocked_by.map((n) => ({
          key: String(n),
          label: `#${n}`,
          remove: `Remove blocker #${n}`,
        }))}
        empty="No issue"
        addLabel="Add a blocker by its number"
        onAdd={(typed) =>
          saved(
            addBlocker(card.id, issueNumber(typed)).then((blocked_by) =>
              patchCard(queryClient, card.id, { blocked_by }),
            ),
          )
        }
        onRemove={(key) =>
          saved(
            removeBlocker(card.id, Number(key)).then((blocked_by) =>
              patchCard(queryClient, card.id, { blocked_by }),
            ),
          )
        }
      />
      <ListProperty
        name="Labels"
        items={card.labels
          .filter((name) => !isPlanningLabel(name))
          .map((name) => ({
            key: name,
            label: name,
            remove: `Remove label ${name}`,
          }))}
        empty="None"
        addLabel="Add a label"
        onAdd={(typed) => saved(setLabels(card.id, [...card.labels, typed]))}
        onRemove={(key) =>
          saved(
            setLabels(
              card.id,
              card.labels.filter((l) => l !== key),
            ),
          )
        }
      />
      <hr className="my-1 border-0 border-t border-hairline" />
      <Property name="Run">
        {card.run ? (
          <Link
            to={`/runs/${card.run.id}`}
            className="text-text underline decoration-hairline-strong underline-offset-4 hover:decoration-current"
          >
            {`${card.run.agent} · ${card.run.purpose}`}
          </Link>
        ) : (
          <span className="text-secondary">No run has started</span>
        )}
      </Property>
      {card.held && <Property name="Blocked">{heldLine(card.held)}</Property>}
      <Property name="Why it waits">
        {waitingLine(board.waiting[String(card.id)], card.refusals)}
      </Property>
      {changesOfCard(card).map((change) => (
        <Property key={change.id} name={`Checks${inRepository(card, change)}`}>
          {CHECKS[change.checks] ?? change.checks}
        </Property>
      ))}
      {stopped && <MergeStopped item={stopped} />}
      <Property name="Cost">
        <span className="tabular-nums">{tokens(card.tokens)}</span>
      </Property>
      {card.note && <Property name="Note">{card.note}</Property>}
    </PageProperties>
  );
}
