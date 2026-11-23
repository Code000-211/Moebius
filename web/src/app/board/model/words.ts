/**
 * The words and links of a card: tokens, the small line under its number, the badges, the forge and pull request
 * addresses, the history and the worktree its change was made in.
 */
import type { Board, Card, Change, Repository } from "../../../api/types/board";
import type { HistoryEntry, Run } from "../../../api/types/cards";
import { plural } from "../../../ui/lib/format";
import type { StateMark } from "../../../ui/page/MarkLabel";
import type { StatusState } from "../../../ui/page/StatusIcon";
import { waitingWords } from "./inbox";
import { type CardStatus, changesOfCard, heldLine, inRepository } from "./status";

/** A number of tokens as the board writes it: `96k tok`, `1.2M tok`. */
export function tokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M tok`;
  if (n >= 1_000) return `${Math.floor(n / 1_000)}k tok`;
  return `${n} tok`;
}

/**
 * The small line of a card under its number: in coding and review the agent and how far its rounds are, for a change
 * that waits to merge how many rounds it took; null when there is nothing to say. What a card cost is the dock's
 * business, where the number is read on its own row rather than hung on the card.
 */
export function cardDetail(card: Card, roundLimit: number): string | null {
  if (card.column === "In progress" || card.column === "In review") {
    const parts: string[] = [];
    if (card.run) parts.push(card.run.agent);
    if (card.rounds > 0) {
      parts.push(roundLimit > 0 ? `round ${card.rounds} of ${roundLimit}` : `round ${card.rounds}`);
    }
    return parts.length > 0 ? parts.join(" · ") : null;
  }
  if (card.column === "Ready to merge") {
    return plural(card.rounds, "round", "rounds");
  }
  return null;
}

/** The forge a repository's issues live on, as a person says it; null without one. */
export function forgeName(repository: Repository): string | null {
  switch (repository.forge) {
    case "github":
      return "GitHub";
    case "gitlab":
      return "GitLab";
    case "forgejo":
      return "Forgejo";
    default:
      return null;
  }
}

/**
 * What deleting a card does, in two sentences for the question that asks first: what goes, and — on a forge — that the
 * issue stays open there and is not made a card again.
 */
export function deleteWords(repository: Repository): { removes: string; keeps: string | null } {
  const forge = forgeName(repository);
  return {
    removes:
      "Its runs and their transcripts, changes, history and inbox items are deleted, and so are its worktrees and branches in the clone. A run that is going is stopped first. Cards waiting on this one keep waiting.",
    keeps: forge
      ? `The issue stays open on ${forge}, and Moebius does not make a card of it again. An open pull request or a pushed branch on ${forge} stays too.`
      : null,
  };
}

/** The address of a card's issue on its forge; only GitHub's address is known (the others' hosts are not). */
export function issueUrl(repository: Repository, card: Card): string | null {
  if (repository.forge === "github" && repository.github) {
    return `https://github.com/${repository.github}/issues/${card.number}`;
  }
  return null;
}

/** The address of a pull request on GitHub; the other forges' hosts are not known. */
export function pullUrl(repository: Repository, pullRequest: number): string | null {
  if (repository.forge === "github" && repository.github) {
    return `https://github.com/${repository.github}/pull/${pullRequest}`;
  }
  return null;
}

/** The pull request a change was published as, with the address to open it at; none before it is published. */
export function pullRequestOf(
  repository: Repository,
  change: Change | undefined,
): { number: number; url: string | null; status: Change["state"] } | null {
  if (change?.pull_request == null) return null;
  return {
    number: change.pull_request,
    url: pullUrl(repository, change.pull_request),
    status: change.state,
  };
}

/** The change of a card: its open one, else the latest it ever had. */
export function changeOf(board: Board, card: Card): Change | undefined {
  if (card.change) {
    const open = board.changes.find((change) => change.id === card.change?.id);
    if (open) return open;
  }
  return board.changes
    .filter((change) => change.card_id === card.id)
    .sort((a, b) => b.id - a.id)[0];
}

/**
 * What a card waits for, as one line: the reason its column script gave, the rules that refused it, both, or
 * "Nothing". A card with nothing to say is not stuck, and an empty row read as a missing fact is worse than the word.
 */
export function waitingLine(reason: string | undefined, refusals: number): string {
  const parts = [reason];
  if (refusals > 0) parts.push(`${plural(refusals, "rule", "rules")} broken`);
  const line = parts.filter((part) => part !== undefined && part !== "").join(" · ");
  return line === "" ? "Nothing" : line;
}

/** What a person reads of one entry of a card's history. */
export function describeHistory(entry: HistoryEntry): string {
  const data = entry.data;
  const word = (key: string): string => {
    const value = data[key];
    return typeof value === "string"
      ? value
      : value === undefined || value === null
        ? ""
        : String(value);
  };
  switch (entry.kind) {
    case "moved":
      return typeof data.note === "string" && data.note !== ""
        ? `Moved from ${word("from")} to ${word("to")}: ${data.note}`
        : `Moved from ${word("from")} to ${word("to")}`;
    case "run_started":
      return `Run ${word("run")} started: ${word("agent")}, ${word("purpose")}`;
    case "run_ended":
      return `Run ${word("run")} ${word("status")}`;
    case "decided":
      return `${word("decider")} decided ${word("choice")}: ${word("reason")}`;
    case "attempt_judged":
      return `Attempt ${word("run")} ${word("result")}${word("output") ? `: ${word("output")}` : ""}`;
    case "attempts_settled":
      return data.picked === null || data.picked === undefined
        ? "No attempt passed"
        : `Attempt ${word("picked")} was kept${word("reason") ? `: ${word("reason")}` : ""}`;
    case "inbox_added":
      return `Asked you: ${word("title")}`;
    case "inbox_resolved":
      return `Answered: ${word("title")}`;
    default:
      return entry.kind.replaceAll("_", " ");
  }
}

export interface CardBadge {
  name: string;
  /** The mark before the word (`ui/page/MarkLabel`); none says the state as grey text alone. */
  mark: StateMark;
  /** Said after the word: the agent of a live card. */
  detail?: string;
  /** More about the state, for a tooltip; the word is the state and carries it without. */
  hint?: string;
}

/** The badges of a card: whether it is blocked, what runs, what stops it, how its change stands, who it waits for. What it cost is the dock's, not the card's. */
export function cardBadges(card: Card, status: CardStatus, next: boolean): CardBadge[] {
  const badges: CardBadge[] = [];
  if (card.held) badges.push({ name: "Blocked", mark: "blocked", hint: heldLine(card.held) });
  if (status === "live") badges.push({ name: "Live", mark: "live", detail: card.run?.agent });
  if (card.blocked_by.length > 0) {
    const numbers = card.blocked_by.map((n) => `#${n}`);
    const name =
      numbers.length <= 2
        ? `Waiting on ${numbers.join(", ")}`
        : `Waiting on ${numbers[0]} and ${numbers.length - 1} more`;
    badges.push({ name, mark: "none" });
  }
  if (card.refusals > 0) {
    badges.push({
      name: plural(card.refusals, "rule broken", "rules broken"),
      mark: "none",
    });
  }
  for (const change of changesOfCard(card)) {
    const where = inRepository(card, change);
    switch (change.checks) {
      case "pending":
        badges.push({ name: `CI running${where}`, mark: "none" });
        break;
      case "success":
        badges.push({ name: `Green${where}`, mark: "none" });
        break;
      case "failure":
        badges.push({ name: `CI failed${where}`, mark: "none" });
        break;
    }
  }
  if (status === "stuck") badges.push({ name: "Stuck", mark: "blocked" });
  if (status === "needs-you") badges.push({ name: "Your turn", mark: "none" });
  if (next) badges.push({ name: "Next", mark: "none" });
  return badges;
}

/**
 * The worktree the card's change was made in, or null. It is the worktree of the run that made the change: the picked
 * attempt when the card was tried several times at once, else the latest `code` run that is not an attempt. A later
 * review run is not it. A worktree exists only while its run is starting, running or waiting, so a finished run names
 * none, because its directory is gone.
 */
export function changeWorktree(card: Card, runs: Run[]): string | null {
  const attempts = card.attempts ?? [];
  const picked = attempts.find((attempt) => attempt.picked)?.run;
  const isAttempt = new Set(attempts.map((attempt) => attempt.run));
  const made =
    picked !== undefined
      ? runs.find((run) => run.id === picked)
      : runs.findLast((run) => run.purpose === "code" && !isAttempt.has(run.id));
  const live =
    made?.status === "starting" || made?.status === "running" || made?.status === "waiting";
  return live ? (made?.worktree ?? null) : null;
}

/** The icon a card wears: what needs a person or runs wins over the column the card lies in. */
export function cardIcon(card: Card, status: CardStatus): StatusState {
  if (status === "needs-you" || status === "stuck") return "needsYou";
  if (status === "live") return "running";
  switch (card.column) {
    case "Backlog":
      return "backlog";
    case "Not started":
      return "notStarted";
    case "In progress":
      return "inProgress";
    case "In review":
      return "inReview";
    case "Ready to merge":
      return "readyToMerge";
    case "Done":
      return "done";
    default:
      return "notStarted";
  }
}

/**
 * The person whose work a card is that is none of this server's people: who opened the pull request it follows, else who
 * took the issue on another server; none when the card is this server's own.
 */
export function elsewhereBy(card: Card): string | null {
  return card.change?.outside_author ?? card.taken_by ?? null;
}

/** What a run is doing, in a word: "coding", "reviewing"; a purpose this does not know is said as it is. */
function doingWords(purpose: string): string {
  const p = purpose.toLowerCase();
  if (p === "code" || p === "attempt") return "coding";
  if (p.startsWith("review")) return "reviewing";
  if (p === "plan") return "planning";
  if (p === "research") return "researching";
  return p;
}

/** What a card's second line is: grey while it is quiet work, orange when it needs a person. */
export type CardActivity = {
  text: string;
  tone: "quiet" | "needs-you";
  /** More about it, as the line's tooltip. */
  hint?: string;
};

/**
 * The one pale line under a card's title: what the agent is doing while it runs, that the card needs a person, or —
 * since nothing else on the card would say it — that a person blocked it or that it waits on other issues. A card that
 * is none of these has no line.
 */
export function cardActivity(
  card: Card,
  status: CardStatus,
  waiting = waitingWords(undefined),
): CardActivity | null {
  if (status === "held" && card.held) {
    return { text: "Blocked", tone: "quiet", hint: heldLine(card.held) };
  }
  if (status === "live" && card.run) {
    const round = card.rounds > 0 ? `round ${card.rounds}` : null;
    return {
      text: [doingWords(card.run.purpose), round].filter(Boolean).join(", "),
      tone: "quiet",
      hint: card.run.agent,
    };
  }
  if (status === "stuck") return { text: "Stuck", tone: "needs-you" };
  // A pull request someone on another server opened, which the card follows: the owner is asked to review it.
  const author = card.change?.outside_author;
  if (status === "needs-you" && author) {
    return { text: `Review @${author}'s pull request`, tone: "needs-you", hint: waiting };
  }
  if (status === "needs-you") return { text: waiting, tone: "needs-you" };
  if (author) return { text: `Pull request by @${author}`, tone: "quiet" };
  if (card.taken_by) {
    return {
      text: `Taken by @${card.taken_by}`,
      tone: "quiet",
      hint: "Someone on another server took this issue; this server leaves it alone",
    };
  }
  if (status === "blocked") {
    const numbers = card.blocked_by.map((n) => `#${n}`);
    return {
      text:
        numbers.length <= 2
          ? `Waiting on ${numbers.join(", ")}`
          : `Waiting on ${numbers[0]} and ${numbers.length - 1} more`,
      tone: "quiet",
    };
  }
  return null;
}
