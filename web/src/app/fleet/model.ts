/**
 * The logic of the Fleet page, apart from React: which runs are working (the cards whose latest run is starting,
 * running or waiting, and the chats that are), what a tile says of its run, and the last steps of a conversation
 * (the former desktop app's `screens/fleet`).
 */
import type { Snapshot } from "../../api/types/board";
import type { Chat } from "../../api/types/chat";
import type { FinishedRun } from "../../api/types/runs";
import type { WorkflowRunSummary } from "../../api/types/workflows";
import type { Entry, ToolCall } from "../../chat/conversation";
import { HARNESS_PLACE } from "../../chat/places";
import type { FilterField } from "../../ui/filters/model";
import { dayName, fileName } from "../../ui/lib/format";
import { cardKey } from "../board/cardKey";
import { WAITING_FOR_APPROVAL } from "../board/model";

/** Where a click on a tile goes. */
export type OpenTarget =
  | { kind: "card"; repository: number; card: number }
  | { kind: "chat"; chat: number }
  | { kind: "workflow"; run: number };

/** A run that is working, as the snapshot and the chats say. */
export interface Work {
  run: number;
  agent: string;
  /** `code`, `review`, ... or `chat`. */
  kind: string;
  status: string;
  /** Whether the run waits for an answer to a question its agent asked: only a chat can. */
  asks: boolean;
  /** The title of the card, or of the chat. */
  title: string;
  /** The issue's id (`WEB-24`, or `#17` without a project key); a chat has none and says `Chat`. */
  id: string;
  /** The card whose plan waits for approval (an open `approve_issue` item), when the run waits for that. */
  approve: number | null;
  repository: string;
  /** Unix seconds; 0 when the snapshot does not say (the run is read for it). */
  startedAt: number;
  /** The tokens of the card since it was last retried; a chat's are not known. */
  tokens: number | null;
  open: OpenTarget;
  /** What the row says besides its kind, when there is more: a workflow run's project and trigger. */
  detail?: string;
}

const isWorking = (status: string) =>
  status === "starting" || status === "running" || status === "waiting";

/** The statuses of a workflow run that has not ended, as the words of `Work.status`. */
const WORKFLOW_WORKING: Record<string, string> = {
  queued: "starting",
  running: "running",
  waiting: "waiting",
};

/** Whether a workflow run has ended. */
export const workflowEnded = (run: WorkflowRunSummary) => !(run.status in WORKFLOW_WORKING);

/** A workflow run that has not ended as a working run; the project it belongs to stands where a card's repository does. */
function workflowWork(run: WorkflowRunSummary): Work {
  return {
    run: run.id,
    agent: "",
    kind: "workflow",
    status: WORKFLOW_WORKING[run.status] ?? "running",
    asks: false,
    title: run.workflow,
    id: "Workflow",
    approve: null,
    repository: run.project_name ?? "",
    startedAt: run.started_at ?? run.created_at,
    tokens: null,
    open: { kind: "workflow", run: run.id },
    detail: [run.project_name, run.trigger].filter((part) => part).join(" · "),
  };
}

/** Every working run, the oldest first. */
export function working(
  snapshot: Snapshot,
  chats: readonly Chat[],
  workflowRuns: readonly WorkflowRunSummary[] = [],
): Work[] {
  const list: Work[] = [];
  const approving = new Set(
    snapshot.inbox
      .filter((item) => item.kind === "approve_issue" && item.resolved_at === null)
      .map((item) => item.card_id),
  );
  for (const board of snapshot.boards) {
    for (const card of board.cards) {
      const run = card.run;
      if (!run || !isWorking(run.status)) continue;
      list.push({
        run: run.id,
        agent: run.agent,
        kind: run.purpose,
        status: run.status,
        asks: false,
        title: card.title,
        id: cardKey(card),
        approve: run.status === "waiting" && approving.has(card.id) ? card.id : null,
        repository: board.repository.name,
        startedAt: 0,
        tokens: card.tokens > 0 ? card.tokens : null,
        open: { kind: "card", repository: board.repository.id, card: card.id },
      });
    }
  }
  for (const chat of chats) {
    if (!isWorking(chat.status)) continue;
    const place = chat.harness_repository
      ? HARNESS_PLACE
      : (snapshot.boards.find((board) => board.repository.id === chat.repository_id)?.repository
          .name ?? "");
    list.push({
      run: chat.id,
      agent: chat.agent,
      kind: "chat",
      status: chat.status,
      asks: chat.asks,
      title: chat.title === "" ? "Chat" : chat.title,
      id: "Chat",
      approve: null,
      repository: place,
      startedAt: chat.started_at,
      tokens: null,
      open: { kind: "chat", chat: chat.id },
    });
  }
  for (const run of workflowRuns) if (!workflowEnded(run)) list.push(workflowWork(run));
  return list.sort((a, b) => a.run - b.run);
}

import { plural } from "../../ui/lib/format";

/** What the filters look at in a run, working or finished. */
export type Facets = Pick<Work, "repository" | "agent" | "kind">;

/** A finished run as the filters see it; `names` says the project of a repository's number. */
export const finishedFacets = (one: FinishedEntry, names: ReadonlyMap<number, string>): Facets =>
  isFinishedWorkflow(one)
    ? { repository: one.workflow.project_name ?? "", agent: "", kind: "workflow" }
    : {
        repository: names.get(one.card.repository_id) ?? "",
        agent: one.run.agent,
        kind: one.run.purpose,
      };

/** The keys of the Fleet's filters in the address, in the order the bar offers them. */
export const FILTER_KEYS = ["project", "agent", "kind"] as const;

/** The fields the Fleet can be filtered by, each offering what the working runs have in it, and the value being filtered by. */
export function filterFields(
  list: readonly Facets[],
  values: Readonly<Record<string, string>>,
): FilterField[] {
  const of = (key: string, pick: (work: Facets) => string): FilterField["options"] => {
    const names = new Set(list.map(pick).filter((name) => name !== ""));
    if ((values[key] ?? "") !== "") names.add(values[key] as string);
    return [...names].sort((a, b) => a.localeCompare(b)).map((name) => [name, name]);
  };
  return [
    { key: "project", label: "Project", options: of("project", (work) => work.repository) },
    { key: "agent", label: "Agent", options: of("agent", (work) => work.agent) },
    { key: "kind", label: "Kind", options: of("kind", (work) => work.kind) },
  ];
}

/** The runs that pass the filters; a filter that is not set lets everything through. */
export function filtered<T extends Facets>(
  list: readonly T[],
  values: Readonly<Record<string, string>>,
): T[] {
  const is = (value: string | undefined, of: string) => !value || value === of;
  return list.filter(
    (work) =>
      is(values.project, work.repository) &&
      is(values.agent, work.agent) &&
      is(values.kind, work.kind),
  );
}

/** Whether a run is at work: starting or running. A run that waits for the person is not working, it is waiting. */
export const atWork = (work: Work) => work.status === "starting" || work.status === "running";

/** One line of the Active list: a run. The list has no group rows; the runs that wait for the person come first. */
export type FleetItem = { type: "row"; key: string; data: Work };

/** What a row is chosen and kept by: a card's run and a chat can share a number. */
export const choiceKey = (work: Work) => `${work.open.kind}:${work.run}`;

/** Whether a run waits for the person. */
export const needsYou = (work: Work) => work.status === "waiting";

/** The list the page draws: the runs that wait for the person first, then the others, each in the order given. */
export function items(list: readonly Work[]): FleetItem[] {
  return [...list.filter(needsYou), ...list.filter((work) => !needsYou(work))].map((work) => ({
    type: "row",
    key: choiceKey(work),
    data: work,
  }));
}

/** The pale words of a run that waits for the person. */
export function waitingWord(work: Work): string {
  if (work.approve !== null) return WAITING_FOR_APPROVAL;
  return work.asks ? "Asks you" : "Your turn";
}

/** A number of tokens: `412k tokens`, `1.5M tokens`. */
export function tokensText(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M tokens`;
  if (n >= 1_000) return `${Math.floor(n / 1_000)}k tokens`;
  return plural(n, "token", "tokens");
}

/** A time as a person reads it: `42s`, `7m 17s`, `1h 02m`. */
export function elapsed(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  if (total < 60) return `${total}s`;
  if (total < 3600) return `${Math.floor(total / 60)}m ${String(total % 60).padStart(2, "0")}s`;
  return `${Math.floor(total / 3600)}h ${String(Math.floor(total / 60) % 60).padStart(2, "0")}m`;
}

const isStep = (entry: Entry) => entry.kind === "thinking" || entry.kind === "tool";

/**
 * Whether the agent of a run is at work this moment. A run that waits for the person is not thinking: the last thing it
 * did has finished, and so has a length to tell.
 */
export function thinkingNow(status: string): boolean {
  return status === "starting" || status === "running";
}

/**
 * What followed the last run of steps, or null when the steps run to the end of the conversation. It is the time the
 * last of them stopped.
 */
export function afterSteps(entries: readonly Entry[]): number | null {
  return entries[entries.findLastIndex(isStep) + 1]?.at ?? null;
}

/**
 * How long the thought at `index` went on, or null when nothing has come after it to say it stopped. An entry keeps the
 * time its first piece arrived, so a thought runs until whatever follows it begins — the next step, or `nextAt`, what
 * followed the last step of all.
 */
export function thoughtSeconds(
  entries: readonly Entry[],
  index: number,
  nextAt: number | null = null,
): number | null {
  const entry = entries[index];
  if (entry?.kind !== "thinking") return null;
  const end = entries[index + 1]?.at ?? nextAt;
  return end === undefined || end === null ? null : Math.max(0, end - entry.at);
}

/** The last run of steps (thinking and tool calls) in the entries; none when there is no step. */
export function latestSteps(entries: readonly Entry[]): Entry[] {
  const end = entries.findLastIndex(isStep) + 1;
  if (end === 0) return [];
  // Walked back over the steps themselves to the thing that came before them. Copying the log to look for it cost
  // more than the walk: a row of the Fleet does this for every tick of its clock.
  let before = end - 1;
  while (before >= 0 && isStep(entries[before] as Entry)) before -= 1;
  return entries.slice(before + 1, end);
}

/**
 * When the work of a run began, or null when that is not known. The entries held are only the newest window of a long
 * run, so their first message or tool call is the start of the run only while nothing is held above them; otherwise
 * the start the run itself reports is the one to measure from, or a length taken from the window would be too short.
 */
export function workStartedAt(
  entries: readonly Entry[],
  hasOlder: boolean,
  startedAt: number,
): number | null {
  if (hasOlder) return startedAt > 0 ? startedAt : null;
  const first = entries.find((entry) => entry.kind === "person" || entry.kind === "tool");
  return first?.at ?? (startedAt > 0 ? startedAt : null);
}

/** The tabs of the page's header, each at an address of its own. */
export const FLEET_SECTIONS = [
  { value: "runs", label: "Runs", path: "/fleet" },
  { value: "runners", label: "Runners", path: "/fleet/runners" },
  { value: "ports", label: "Ports", path: "/fleet/ports" },
] as const;
export type FleetSection = (typeof FLEET_SECTIONS)[number]["value"];

/** The tabs of the Runs tab: the runs that are going on, and the runs that have ended. `finished` is kept in the address. */
export const FLEET_TABS = [
  { value: "active", label: "Active" },
  { value: "finished", label: "Finished" },
] as const;
export type FleetTab = (typeof FLEET_TABS)[number]["value"];

/** The tab the address names (`?tab=finished`); anything else is the Active tab. */
export const tabOf = (param: string | null): FleetTab =>
  param === "finished" ? "finished" : "active";

/** One line of the Finished list: a day's name, or a run that ended. */
export type FinishedItem =
  | { type: "group"; key: string; name: string }
  | { type: "row"; key: string; data: FinishedEntry };

/** A workflow run that has ended, as the Finished list holds it. */
export interface FinishedWorkflow {
  workflow: WorkflowRunSummary;
}

/** What the Finished list holds: a run of a card, or a run of a workflow. */
export type FinishedEntry = FinishedRun | FinishedWorkflow;

export const isFinishedWorkflow = (one: FinishedEntry): one is FinishedWorkflow =>
  "workflow" in one;

/** The workflow runs that have ended, as entries of the Finished list. */
export const finishedWorkflows = (runs: readonly WorkflowRunSummary[]): FinishedWorkflow[] =>
  runs.filter(workflowEnded).map((workflow) => ({ workflow }));

/** What a finished run is chosen and kept by. */
export const finishedKey = (one: FinishedEntry) =>
  isFinishedWorkflow(one) ? `finished-workflow:${one.workflow.id}` : `finished:${one.run.id}`;

/** When a finished run ended, in seconds; null when that is not recorded. */
const endedAt = (one: FinishedEntry): number | null =>
  isFinishedWorkflow(one)
    ? (one.workflow.finished_at ?? one.workflow.started_at ?? one.workflow.created_at)
    : one.run.ended_at;

const idOfEntry = (one: FinishedEntry) => (isFinishedWorkflow(one) ? one.workflow.id : one.run.id);

/** The `YYYY-MM-DD` of the browser's calendar for a Unix time in seconds. */
function dayOf(seconds: number): string {
  const at = new Date(seconds * 1000);
  const month = `${at.getMonth() + 1}`.padStart(2, "0");
  return `${at.getFullYear()}-${month}-${`${at.getDate()}`.padStart(2, "0")}`;
}

/**
 * The flat list of the Finished tab: only the runs that ended, the one that ended last first, under the name of the day
 * they ended on (`Today`, `Yesterday`, `27 Oct`). A run whose end is not recorded is not finished and is left out.
 */
export function finishedItems(list: readonly FinishedEntry[], today: string): FinishedItem[] {
  const ended = list
    .filter((one) => endedAt(one) !== null)
    .sort((a, b) => (endedAt(b) ?? 0) - (endedAt(a) ?? 0) || idOfEntry(b) - idOfEntry(a));
  const out: FinishedItem[] = [];
  let day = "";
  for (const one of ended) {
    const of = dayOf(endedAt(one) ?? 0);
    if (of !== day) {
      day = of;
      out.push({ type: "group", key: `day:${of}`, name: dayName(of, today) });
    }
    out.push({ type: "row", key: finishedKey(one), data: one });
  }
  return out;
}

/** How long a finished run went on, in seconds. */
export const lasted = (one: FinishedEntry) =>
  isFinishedWorkflow(one)
    ? Math.max(0, (endedAt(one) ?? 0) - (one.workflow.started_at ?? one.workflow.created_at))
    : Math.max(0, (one.run.ended_at ?? 0) - one.run.started_at);

/** The card's id the way the rest of the app names it (`WEB-24`, or `#17` for a project without a key). */
export const finishedId = (one: FinishedEntry) =>
  isFinishedWorkflow(one) ? "Workflow" : cardKey(one.card);

/** How a finished run ended, in the words of the outcome marks: a cancelled workflow run is a stopped one. */
export const outcomeOf = (one: FinishedEntry) =>
  isFinishedWorkflow(one)
    ? one.workflow.status === "cancelled"
      ? "stopped"
      : one.workflow.status
    : one.run.status;

const TESTS = /\b(test|tests|pytest|vitest|jest|nextest)\b/;
const CHECKS = /\b(clippy|lint|fmt|format|biome|eslint|tsc|check|typecheck)\b/;
const BUILDS = /\b(build|compile)\b/;
const SEARCHES = /^(rg|grep|find|fd|ls|tree)\b/;
const DIFFS = /^git (diff|log|show|status|blame)\b/;

/**
 * What a step is, in a few plain words: "reading engine.rs", "running the tests". The raw command stays in the run's
 * own page; a row says what the agent is doing, not how.
 */
export function doingWords(call: ToolCall): string {
  const input = call.input;
  switch (input.type) {
    case "read":
      return `reading ${fileName(input.path)}`;
    case "edit":
      return `editing ${fileName(input.path)}`;
    case "write":
      return `writing ${fileName(input.path)}`;
    case "command": {
      const line = (input.command.split("\n")[0] ?? "").trim();
      const word = line.replace(/^(\S+=\S+\s+)*/, "");
      if (DIFFS.test(word)) return "looking at the changes";
      if (TESTS.test(word)) return "running the tests";
      if (CHECKS.test(word)) return "checking the code";
      if (BUILDS.test(word)) return "building";
      if (SEARCHES.test(word)) return "searching the code";
      if (word.startsWith("git ")) return "using git";
      return "running a command";
    }
    case "todos":
      return "planning the work";
    case "questions":
      return "asking a question";
    default:
      return call.mcp ? `using ${call.mcp.server}` : "working";
  }
}
