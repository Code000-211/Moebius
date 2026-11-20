/**
 * The logic of the Fleet page, apart from React: which runs are working (the cards whose latest run is starting,
 * running or waiting, and the chats that are), what a tile says of its run, and the last steps of a conversation
 * (the former desktop app's `screens/fleet`).
 */
import type { Snapshot } from "../../api/types/board";
import type { Chat } from "../../api/types/chat";
import type { Entry } from "../../chat/conversation";
import { HARNESS_PLACE } from "../../chat/places";
import type { FilterField } from "../../ui/filters/model";

/** Where a click on a tile goes. */
export type OpenTarget =
  | { kind: "card"; repository: number; card: number }
  | { kind: "chat"; chat: number };

/** A run that is working, as the snapshot and the chats say. */
export interface Work {
  run: number;
  agent: string;
  /** `code`, `review`, ... or `chat`. */
  kind: string;
  status: string;
  /** Whether the run waits for an answer to a question its agent asked: only a chat can. */
  asks: boolean;
  /** `#number title` of a card, the title of a chat. */
  title: string;
  repository: string;
  /** Unix seconds; 0 when the snapshot does not say (the run is read for it). */
  startedAt: number;
  /** The tokens of the card since it was last retried; a chat's are not known. */
  tokens: number | null;
  open: OpenTarget;
}

const isWorking = (status: string) =>
  status === "starting" || status === "running" || status === "waiting";

/** Every working run, the oldest first. */
export function working(snapshot: Snapshot, chats: readonly Chat[]): Work[] {
  const list: Work[] = [];
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
        title: `#${card.number} ${card.title}`,
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
      repository: place,
      startedAt: chat.started_at,
      tokens: null,
      open: { kind: "chat", chat: chat.id },
    });
  }
  return list.sort((a, b) => a.run - b.run);
}

import { plural } from "../../ui/lib/format";
import type { StateMark } from "../../ui/page/MarkLabel";

/** The keys of the Fleet's filters in the address, in the order the bar offers them. */
export const FILTER_KEYS = ["project", "agent", "kind"] as const;

/** The fields the Fleet can be filtered by, each offering what the working runs have in it, and the value being filtered by. */
export function filterFields(
  list: readonly Work[],
  values: Readonly<Record<string, string>>,
): FilterField[] {
  const of = (key: string, pick: (work: Work) => string): FilterField["options"] => {
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
export function filtered(list: readonly Work[], values: Readonly<Record<string, string>>): Work[] {
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

/**
 * What the page says under its title: how many agents are at work (starting or running), and how many runs wait for the
 * person, which are not working however long they stay in the list.
 */
export function context(list: readonly Work[]): string {
  const busy = list.filter(atWork).length;
  const waiting = list.length - busy;
  if (waiting === 0)
    return busy > 0 ? `${plural(busy, "agent", "agents")} working` : "Nothing is working.";
  return `${busy > 0 ? `${plural(busy, "agent", "agents")} working` : "Nothing is working"} · ${waiting} waiting for you`;
}

/** The runs grouped by their agent, the agents in the order of their names and each agent's runs in the order given. */
export function byAgent(list: readonly Work[]): Array<[string, Work[]]> {
  const groups = new Map<string, Work[]>();
  for (const work of list) groups.set(work.agent, [...(groups.get(work.agent) ?? []), work]);
  return [...groups].sort(([a], [b]) => a.localeCompare(b));
}

/** How a working run reads at a glance: the word, and the mark before it. */
export interface Status {
  word: string;
  /** The turning ring for work under way; none for a run that waits for the person, said as grey text. */
  mark: StateMark;
}

/**
 * The word for a working run, with the mark that belongs to it. `waiting` is a run that waits for the person, which is
 * why it has no ring: nothing is turning.
 */
export function statusWord(status: string, asks: boolean): Status {
  if (status === "waiting") return { word: asks ? "Asks you" : "Your turn", mark: "none" };
  if (status === "starting") return { word: "Starting", mark: "live" };
  return { word: "Running", mark: "live" };
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
